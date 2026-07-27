use std::{
   collections::HashMap,
   sync::{
      Mutex,
      atomic::{
         AtomicU64,
         Ordering,
      },
   },
};

use axum::{
   extract::{
      Query,
      State,
   },
   http::StatusCode,
   response::{
      IntoResponse,
      Response,
   },
};
use fastwebsockets::{
   WebSocketError,
   upgrade::IncomingUpgrade,
};
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{
   error,
   warn,
};

use crate::{
   AppState,
   socket_v2,
};

#[derive(Clone)]
pub(crate) enum SocketEvent {
   Wake,
   Replaced(String),
}

struct AttachedSocket {
   connection_id: u64,
   sender:        mpsc::UnboundedSender<SocketEvent>,
}

pub struct SocketHub {
   next_id:     AtomicU64,
   attachments: Mutex<HashMap<String, AttachedSocket>>,
}

impl SocketHub {
   pub fn new() -> Self {
      Self {
         next_id:     AtomicU64::new(1),
         attachments: Mutex::new(HashMap::new()),
      }
   }

   pub(crate) fn new_connection(
      &self,
   ) -> (
      u64,
      mpsc::UnboundedSender<SocketEvent>,
      mpsc::UnboundedReceiver<SocketEvent>,
   ) {
      let id = self.next_id.fetch_add(1, Ordering::Relaxed);
      let (sender, receiver) = mpsc::unbounded_channel();
      (id, sender, receiver)
   }

   pub(crate) fn attach(
      &self,
      install_id: &str,
      connection_id: u64,
      sender: &mpsc::UnboundedSender<SocketEvent>,
   ) {
      let mut attachments = self.attachments.lock().expect("socket hub lock poisoned");
      if let Some(previous) = attachments.insert(install_id.to_string(), AttachedSocket {
         connection_id,
         sender: sender.clone(),
      }) {
         if previous.connection_id != connection_id {
            let _ = previous
               .sender
               .send(SocketEvent::Replaced(install_id.to_string()));
         }
      }
   }

   pub(crate) fn detach(&self, install_id: &str, connection_id: u64) {
      let mut attachments = self.attachments.lock().expect("socket hub lock poisoned");
      if attachments
         .get(install_id)
         .is_some_and(|socket| socket.connection_id == connection_id)
      {
         attachments.remove(install_id);
      }
   }

   pub fn wake(&self, install_id: &str) {
      let attachments = self.attachments.lock().expect("socket hub lock poisoned");
      if let Some(socket) = attachments.get(install_id) {
         let _ = socket.sender.send(SocketEvent::Wake);
      }
   }

   pub fn active_count(&self) -> usize {
      self
         .attachments
         .lock()
         .expect("socket hub lock poisoned")
         .len()
   }

   #[cfg(test)]
   pub(crate) fn is_attached(&self, install_id: &str) -> bool {
      self
         .attachments
         .lock()
         .expect("socket hub lock poisoned")
         .contains_key(install_id)
   }
}

impl Default for SocketHub {
   fn default() -> Self {
      Self::new()
   }
}

#[derive(Deserialize)]
pub struct SocketQuery {
   version: Option<u8>,
}

pub async fn upgrade(
   State(state): State<AppState>,
   Query(query): Query<SocketQuery>,
   ws: IncomingUpgrade,
) -> Result<Response, (StatusCode, String)> {
   if query.version != Some(2) {
      let received = query
         .version
         .map_or_else(|| "missing".to_string(), |version| version.to_string());
      return Err((
         StatusCode::BAD_REQUEST,
         format!("socket protocol version 2 required, received {received}"),
      ));
   }

   let db = state.db.clone();
   let hub = state.socket_hub.clone();
   let (response, future) = ws.upgrade().map_err(invalid_upgrade)?;
   tokio::spawn(socket_v2::run_socket(future, db, hub));
   Ok(response.into_response())
}

fn invalid_upgrade(error: WebSocketError) -> (StatusCode, String) {
   warn!("Invalid WebSocket upgrade: {error}");
   (
      StatusCode::BAD_REQUEST,
      "invalid WebSocket upgrade".to_string(),
   )
}

#[allow(dead_code)]
fn internal(e: anyhow::Error) -> (StatusCode, String) {
   error!("WebSocket database error: {e}");
   (
      StatusCode::INTERNAL_SERVER_ERROR,
      "database error".to_string(),
   )
}
