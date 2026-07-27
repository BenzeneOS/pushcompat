//! FCM listener management
//!
//! Manages FCM connections for registered apps and forwards messages to UP
//! endpoints.

use std::{
   collections::HashMap,
   sync::Arc,
};

use anyhow::Result;
use futures_util::StreamExt;
use pushcompat_listener::{
   AppRegistration,
   AppRegistrationState,
   DeviceSession,
   DeviceSessionState,
   FcmCredentials,
   Message,
   MessageTag,
   decode_login_response,
};
use serde::{
   Deserialize,
   Serialize,
};
use tokio::{
   io::AsyncWriteExt,
   sync::mpsc,
};
use tracing::{
   error,
   info,
   warn,
};

use crate::{
   db::Database,
   delivery::{
      DeliveryManager,
      DeliveryTarget,
   },
   types::{
      AppId,
      InstallId,
   },
};

/// MCS carries the ack list inline in the login packet, so it cannot grow
/// unbounded.
const MAX_ACK_HISTORY: usize = 500;

pub struct FcmManager {
   /// Active listeners by (install_id, app_id)
   listeners:   HashMap<(InstallId, AppId), ListenerHandle>,
   /// HTTP client for FCM registration
   http_client: reqwest::Client,
}

#[derive(PartialEq, Eq)]
struct ListenerConfig {
   firebase_app_id:     String,
   firebase_project_id: String,
   firebase_api_key:    String,
   cert_sha1:           Option<String>,
   app_version:         Option<i32>,
   app_version_name:    Option<String>,
   target_sdk:          Option<i32>,
   target:              DeliveryTarget,
}

struct ListenerHandle {
   /// Channel to stop the listener
   stop_tx:   mpsc::Sender<()>,
   /// FCM token for this registration
   fcm_token: String,
   config:    ListenerConfig,
   task:      Option<tokio::task::JoinHandle<()>>,
}

#[derive(Serialize, Deserialize)]
struct StoredFcmRegistration {
   gcm_session: DeviceSessionState,
   gcm_token:   StoredGcmToken,
   credentials: FcmCredentials,
}

#[derive(Serialize, Deserialize)]
struct StoredGcmToken {
   token: String,
}

impl StoredFcmRegistration {
   fn snapshot(device: &DeviceSession, app: &AppRegistration) -> Self {
      let app = app.state();
      Self {
         gcm_session: device.state(),
         gcm_token:   StoredGcmToken {
            token: app.fcm_token,
         },
         credentials: app.credentials,
      }
   }

   fn restore(self) -> (DeviceSession, AppRegistration) {
      (
         DeviceSession::restore(self.gcm_session),
         AppRegistration::restore(AppRegistrationState {
            fcm_token:   self.gcm_token.token,
            credentials: self.credentials,
         }),
      )
   }
}

impl FcmManager {
   pub fn new() -> Self {
      pushcompat_listener::install_crypto_provider();
      Self {
         listeners:   HashMap::new(),
         http_client: pushcompat_listener::http_client_builder()
            .http1_only()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client"),
      }
   }

   pub fn active_count(&self) -> usize {
      self.listeners.len()
   }

   fn running_token(&self, key: &(InstallId, AppId), config: &ListenerConfig) -> Option<String> {
      self
         .listeners
         .get(key)
         .filter(|handle| &handle.config == config)
         .map(|handle| handle.fcm_token.clone())
   }

   pub async fn start_listener(
      &mut self,
      install_id: InstallId,
      app_id: AppId,
      firebase_app_id: String,
      firebase_project_id: String,
      firebase_api_key: String,
      cert_sha1: Option<String>,
      app_version: Option<i32>,
      app_version_name: Option<String>,
      target_sdk: Option<i32>,
      target: DeliveryTarget,
      db: Arc<Database>,
      delivery: Arc<DeliveryManager>,
   ) -> Result<String> {
      let key = (install_id.clone(), app_id.clone());
      let label = format!("{install_id}/{app_id}");
      let config = ListenerConfig {
         firebase_app_id: firebase_app_id.clone(),
         firebase_project_id: firebase_project_id.clone(),
         firebase_api_key: firebase_api_key.clone(),
         cert_sha1: cert_sha1.clone(),
         app_version,
         app_version_name: app_version_name.clone(),
         target_sdk,
         target: target.clone(),
      };
      if let Some(token) = self.running_token(&key, &config) {
         info!("Keeping unchanged FCM listener for {}", label);
         return Ok(token);
      }

      if let Some(handle) = self.listeners.remove(&key) {
         let _ = handle.stop_tx.try_send(());
         if let Some(task) = handle.task {
            task.abort();
         }
      }

      // Extract sender_id from firebase_app_id
      // Format: "1:<sender_id>:android:<hash>"
      let sender_id = extract_sender_id(&firebase_app_id)?;

      // Build FCM credentials
      let credentials = FcmCredentials {
         sender_id: sender_id.clone(),
         api_key: firebase_api_key,
         app_id: firebase_app_id,
         project_id: firebase_project_id,
         package_name: app_id.as_ref().to_owned(),
         cert_sha1,
         app_version,
         app_version_name,
         target_sdk,
      };

      // Try to load existing session first
      let (device, app_registration) =
         if let Ok(Some(session_json)) = db.get_fcm_session(&install_id, &app_id).await {
            match serde_json::from_str::<StoredFcmRegistration>(&session_json) {
               Ok(existing) => {
                  let restored = existing.restore();
                  info!(
                     "Reusing existing FCM session for {} (token: {}...)",
                     label,
                     &restored.1.fcm_token()[..20.min(restored.1.fcm_token().len())]
                  );
                  restored
               },
               Err(e) => {
                  warn!(
                     "Failed to deserialize saved session for {}: {}, re-registering",
                     label, e
                  );
                  register_app(&self.http_client, credentials.clone()).await?
               },
            }
         } else {
            info!(
               "Registering with FCM for app: {} (sender_id: {}, cert: {})",
               label,
               sender_id,
               credentials.cert_sha1.as_deref().unwrap_or("none")
            );
            register_app(&self.http_client, credentials.clone()).await?
         };

      let fcm_token = app_registration.fcm_token().to_string();
      info!(
         "Got FCM token for {}: {}...",
         label,
         &fcm_token[..20.min(fcm_token.len())]
      );

      // Save registration for reconnection
      let stored_registration = StoredFcmRegistration::snapshot(&device, &app_registration);
      if let Ok(reg_json) = serde_json::to_string(&stored_registration) {
         let _ = db.save_fcm_session(&install_id, &app_id, &reg_json).await;
      }

      // Create stop channel
      let (stop_tx, stop_rx) = mpsc::channel(1);

      // Clone values for the listener task
      let fcm_token_clone = fcm_token.clone();

      // Spawn listener task
      let db_for_listener = db.clone();
      let task = tokio::spawn(async move {
         run_listener(
            install_id.clone(),
            app_id.clone(),
            sender_id,
            device,
            target,
            db_for_listener,
            delivery,
            stop_rx,
         )
         .await;
      });

      self.listeners.insert(key, ListenerHandle {
         stop_tx,
         fcm_token: fcm_token_clone,
         config,
         task: Some(task),
      });

      Ok(fcm_token)
   }

   pub fn stop_listener(&mut self, install_id: &InstallId, app_id: &AppId) {
      if let Some(handle) = self.listeners.remove(&(install_id.clone(), app_id.clone())) {
         let _ = handle.stop_tx.try_send(());
         if let Some(task) = handle.task {
            task.abort();
         }
         info!("Stopped FCM listener for {}/{}", install_id, app_id);
      }
   }
}

async fn register_app(
   http: &reqwest::Client,
   credentials: FcmCredentials,
) -> Result<(DeviceSession, AppRegistration), pushcompat_listener::Error> {
   let device = DeviceSession::fresh(http).await?;
   let app = AppRegistration::register(http, &device, credentials).await?;
   Ok((device, app))
}

/// Extract sender_id from Firebase app ID
/// Format: "1:<sender_id>:android:<hash>" or "1:<sender_id>:web:<hash>"
fn extract_sender_id(firebase_app_id: &str) -> Result<String> {
   let parts: Vec<&str> = firebase_app_id.split(':').collect();
   if parts.len() >= 4
      && parts[0] == "1"
      && !parts[1].is_empty()
      && parts[1].bytes().all(|byte| byte.is_ascii_digit())
      && matches!(parts[2], "android" | "web")
      && !parts[3].is_empty()
   {
      Ok(parts[1].to_string())
   } else {
      anyhow::bail!("Invalid firebase_app_id format: {}", firebase_app_id)
   }
}

async fn run_listener(
   install_id: InstallId,
   app_id: AppId,
   sender_id: String,
   device: DeviceSession,
   target: DeliveryTarget,
   db: Arc<Database>,
   delivery: Arc<DeliveryManager>,
   mut stop_rx: mpsc::Receiver<()>,
) {
   let label = format!("{install_id}/{app_id}");
   info!("Starting FCM listener for {}", label);

   // Seeded from disk: MCS replays everything it has not seen acked, so a restart
   // with an empty list re-delivers the entire backlog at once.
   let mut persistent_ids = db
      .recent_acks(&install_id, &app_id, MAX_ACK_HISTORY)
      .await
      .unwrap_or_default();
   // The query is newest-first, while this in-memory queue evicts from the
   // front as newer ids arrive.
   persistent_ids.reverse();
   info!(
      "Restored {} acked message ids for {label}",
      persistent_ids.len(),
   );

   loop {
      // Check if we should stop
      if stop_rx.try_recv().is_ok() {
         info!("FCM listener stopped for {}", label);
         break;
      }

      // Connect to mtalk.google.com
      info!(
         "Sending {} persistent ids in MCS login for {label}",
         persistent_ids.len(),
      );
      let mut stream = match device.connect(persistent_ids.clone()).await {
         Ok(stream) => stream,
         Err(e) => {
            error!("FCM connection failed for {label}: {e}");
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            continue;
         },
      };

      info!("FCM connection established for {label}");
      let mut last_acked_stream_id = None;

      // Listen for messages
      loop {
         tokio::select! {
             _ = stop_rx.recv() => {
                 info!("FCM listener stopped for {}", label);
                 return;
             }

             msg = stream.next() => {
                 let stream_id = stream.last_stream_id_received();
                 match msg {
                     Some(Ok(Message::Data(data))) => {
                         let payload_len = data.raw_data.as_ref().map(|d| d.len()).unwrap_or(0);
                         info!(
                             "Received FCM message for {}: {} bytes, persistent_id: {:?}, from: {:?}",
                             label,
                             payload_len,
                             data.persistent_id,
                             data.from
                         );
                         let is_redelivery = data
                             .persistent_id
                             .as_ref()
                             .is_some_and(|pid| persistent_ids.contains(pid));
                         if is_redelivery {
                             info!(
                                 "Received redelivered FCM message for {label}: persistent_id={:?}",
                                 data.persistent_id,
                             );
                         }

                         // Rebuild what an Android FCM intent carries: the app payload
                         // plus the stanza metadata. `google.message_id` in particular is
                         // mandatory — the Firebase SDK drops messages that lack it — and it
                         // lives on the stanza, not in app_data.
                         let body = build_intent_payload(&data, &sender_id);
                         if body.is_empty() {
                             warn!("Empty payload in FCM message for {}", label);
                         }

                         if let Err(e) = delivery
                             .enqueue_fcm(
                                 &install_id,
                                 &app_id,
                                 &target,
                                 data.persistent_id.as_deref(),
                                 &body,
                             )
                             .await
                         {
                             error!("Failed to persist FCM message for {}: {}", label, e);
                             break;
                         }

                         if let Some(pid) = &data.persistent_id {
                             if !is_redelivery {
                                 persistent_ids.push(pid.clone());
                                 if persistent_ids.len() > MAX_ACK_HISTORY {
                                     persistent_ids.remove(0);
                                 }
                             }
                         }

                         let ack = pushcompat_listener::new_stream_ack(stream_id);
                         if let Err(e) = stream.write_all(&ack).await {
                             error!("Failed to acknowledge FCM message for {}: {}", label, e);
                             break;
                         }
                         last_acked_stream_id = Some(stream_id);

                     }

                     Some(Ok(Message::HeartbeatPing)) => {
                         let ack = pushcompat_listener::new_heartbeat_ack(stream_id);
                         if let Err(e) = stream.write_all(&ack).await {
                             error!("Failed to send heartbeat ack for {}: {}", label, e);
                             break; // Reconnect
                         }
                         last_acked_stream_id = Some(stream_id);
                     }

                     Some(Ok(Message::Other(tag, body))) => {
                         if tag == MessageTag::LoginResponse as u8 {
                             match decode_login_response(&body) {
                                 Ok(response) => info!(
                                     "MCS login response for {label}: id={}, error_code={:?}, error_message={:?}, error_type={:?}, stream_id={:?}, last_stream_id_received={:?}, server_timestamp={:?}",
                                     response.id,
                                     response.error_code,
                                     response.error_message,
                                     response.error_type,
                                     response.stream_id,
                                     response.last_stream_id_received,
                                     response.server_timestamp,
                                 ),
                                 Err(e) => warn!(
                                     "Failed to decode MCS login response for {label}: {e}",
                                 ),
                             }
                         } else {
                             warn!("Unknown FCM message type {} for {}", tag, label);
                         }
                     }

                     Some(Err(e)) => {
                         error!("FCM receive error for {}: {}", label, e);
                         break; // Reconnect
                     }

                     None => {
                         warn!("FCM stream ended for {}", label);
                         break; // Reconnect
                     }
                 }
             }
         }
      }

      // Wait before reconnecting
      warn!(
         "FCM connection lost for {label}, last acknowledged stream id: {:?}, reconnecting in \
          5s...",
         last_acked_stream_id,
      );
      tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
   }
}

fn build_intent_payload(data: &pushcompat_listener::DataMessage, sender_id: &str) -> Vec<u8> {
   let mut fields = serde_json::Map::new();
   for (key, value) in &data.app_data {
      fields.insert(key.clone(), serde_json::Value::String(value.clone()));
   }
   if let Some(raw) = &data.raw_data {
      match serde_json::from_slice::<serde_json::Value>(raw) {
         Ok(serde_json::Value::Object(raw_fields)) => {
            for (key, value) in raw_fields {
               let value = match value {
                  serde_json::Value::String(value) => value,
                  value => value.to_string(),
               };
               fields.insert(key, serde_json::Value::String(value));
            }
         },
         _ => {
            fields.insert(
               "message".into(),
               serde_json::Value::String(String::from_utf8_lossy(raw).into_owned()),
            );
         },
      }
   }

   let message_id = data.id.clone().or_else(|| data.persistent_id.clone());
   for (key, value) in [
      ("google.message_id", message_id),
      (
         "from",
         Some(data.from.clone().unwrap_or_else(|| sender_id.to_string())),
      ),
      ("google.c.sender.id", Some(sender_id.to_string())),
      ("collapse_key", data.collapse_key.clone()),
   ] {
      if let Some(value) = value {
         fields.insert(key.into(), serde_json::Value::String(value));
      }
   }
   serde_json::to_vec(&fields).unwrap_or_default()
}

impl Default for FcmManager {
   fn default() -> Self {
      Self::new()
   }
}
