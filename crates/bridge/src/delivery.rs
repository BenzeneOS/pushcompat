use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Notify;
use tracing::{
   error,
   info,
};

use crate::{
   db::{
      Database,
      OutboxMessage,
   },
   socket::SocketHub,
   types::{
      AppId,
      ConnectorToken,
      InstallId,
      MessageId,
      Transport,
   },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryTarget {
   UnifiedPush { endpoint: String },
   WebSocket,
}

impl DeliveryTarget {
   pub fn from_parts(transport: Transport, endpoint: &str) -> Result<Self> {
      match transport {
         Transport::UnifiedPush if !endpoint.is_empty() => {
            Ok(Self::UnifiedPush {
               endpoint: endpoint.to_string(),
            })
         },
         Transport::WebSocket => Ok(Self::WebSocket),
         Transport::UnifiedPush => anyhow::bail!("UnifiedPush endpoint is missing"),
      }
   }

   pub fn transport(&self) -> Transport {
      match self {
         Self::UnifiedPush { .. } => Transport::UnifiedPush,
         Self::WebSocket => Transport::WebSocket,
      }
   }
}

pub struct DeliveryManager {
   db:          Arc<Database>,
   http_client: reqwest::Client,
   socket_hub:  Arc<SocketHub>,
   up_wake:     Notify,
}

impl DeliveryManager {
   pub fn new(db: Arc<Database>, socket_hub: Arc<SocketHub>) -> Self {
      pushcompat_listener::install_crypto_provider();
      Self {
         db,
         http_client: pushcompat_listener::http_client_builder()
            .http1_only()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build delivery HTTP client"),
         socket_hub,
         up_wake: Notify::new(),
      }
   }

   pub fn start(self: &Arc<Self>) {
      let manager = self.clone();
      tokio::spawn(async move {
         manager.run_unified_push_delivery().await;
      });
   }

   pub async fn enqueue_fcm(
      &self,
      install_id: &InstallId,
      app_id: &AppId,
      target: &DeliveryTarget,
      persistent_id: Option<&str>,
      payload: &[u8],
   ) -> Result<Option<MessageId>> {
      let message_id = self
         .db
         .enqueue_fcm_message(
            install_id,
            app_id,
            target.transport(),
            persistent_id,
            payload,
         )
         .await?;

      if message_id.is_some() {
         match target {
            DeliveryTarget::UnifiedPush { .. } => self.up_wake.notify_one(),
            DeliveryTarget::WebSocket => self.socket_hub.wake(install_id.as_ref()),
         }
      }

      Ok(message_id)
   }

   pub async fn enqueue_unified_push(
      &self,
      install_id: &InstallId,
      app_id: &AppId,
      connector_token: &ConnectorToken,
      payload: &[u8],
   ) -> Result<MessageId> {
      let message_id = self
         .db
         .enqueue_unified_push_message(install_id, app_id, connector_token, payload)
         .await?;
      self.socket_hub.wake(install_id.as_ref());
      Ok(message_id)
   }

   async fn run_unified_push_delivery(self: Arc<Self>) {
      loop {
         match self.db.due_unified_push_messages(32).await {
            Ok(messages) if messages.is_empty() => {
               tokio::select! {
                   _ = self.up_wake.notified() => {}
                   _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
               }
            },
            Ok(messages) => {
               for message in messages {
                  self.deliver_to_unified_push(message).await;
               }
            },
            Err(e) => {
               error!("Failed to load UnifiedPush outbox: {e}");
               tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            },
         }
      }
   }

   async fn deliver_to_unified_push(&self, message: OutboxMessage) {
      let Some(endpoint) = message.endpoint.as_deref() else {
         error!(
            "UnifiedPush outbox row {} has no endpoint for {}/{}",
            message.id, message.install_id, message.app_id
         );
         let _ = self.db.delete_outbox_message(message.id).await;
         return;
      };

      match forward_to_up(endpoint, &message.payload, &self.http_client).await {
         Ok(()) => {
            if let Err(e) = self.db.delete_outbox_message(message.id).await {
               error!("Failed to finish outbox row {}: {e}", message.id);
            } else {
               info!(
                  "Forwarded outbox row {} to UnifiedPush for {}/{}",
                  message.id, message.install_id, message.app_id
               );
            }
         },
         Err(e) => {
            let shift = message.attempts.min(6);
            let retry_seconds = 5_i64 * (1_i64 << shift);
            error!(
               "UnifiedPush delivery failed for outbox row {}: {}; retrying in {}s",
               message.id, e, retry_seconds
            );
            if let Err(db_error) = self
               .db
               .defer_outbox_message(message.id, retry_seconds)
               .await
            {
               error!("Failed to defer outbox row {}: {db_error}", message.id);
            }
         },
      }
   }
}

async fn forward_to_up(endpoint: &str, body: &[u8], http_client: &reqwest::Client) -> Result<()> {
   const MAX_ATTEMPTS: u32 = 4;
   let mut delay = std::time::Duration::from_millis(500);
   let mut last = Option::<String>::None;

   for attempt in 1..=MAX_ATTEMPTS {
      let result = http_client
         .post(endpoint)
         .header("Content-Type", "application/octet-stream")
         .body(body.to_vec())
         .send()
         .await;

      match result {
         Ok(response) if response.status().is_success() => return Ok(()),
         Ok(response) => {
            let status = response.status();
            if status.is_client_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
               anyhow::bail!("UP endpoint returned {status}");
            }
            last = Some(format!("UP endpoint returned {status}"));
         },
         Err(e) => last = Some(e.to_string()),
      }

      if attempt < MAX_ATTEMPTS {
         tokio::time::sleep(delay).await;
         delay *= 3;
      }
   }

   anyhow::bail!(last.unwrap_or_else(|| "delivery failed".into()))
}
