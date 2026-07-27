use std::{
   collections::HashMap,
   sync::Arc,
};

use aes_gcm::{
   Aes256Gcm,
   KeyInit,
   Nonce,
   aead::Aead,
};
use anyhow::{
   Context,
   Result,
};
use data_encoding::BASE64;
use fastwebsockets::{
   FragmentCollector,
   Frame,
   OpCode,
   Payload,
   WebSocketError,
   upgrade::UpgradeFut,
};
use hkdf::Hkdf;
use hmac::{
   Hmac,
   Mac,
};
use rand::RngCore;
use serde::{
   Deserialize,
   Serialize,
};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use tokio::sync::mpsc;
use tracing::{
   error,
   info,
   warn,
};

use crate::{
   db::{
      Database,
      OutboxMessage,
   },
   socket::{
      SocketEvent,
      SocketHub,
   },
   types::{
      AppId,
      ConnectorToken,
      Cursor,
      InstallId,
      InstallSecret,
      MessageId,
      MessageKind,
      PayloadKey,
   },
};

const KEY_CONTEXT: &[u8] = b"pushcompat websocket payload v2";
const NONCE_BYTES: usize = 32;
const GCM_NONCE_BYTES: usize = 12;

type HmacSha256 = Hmac<Sha256>;

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerFrame {
   Hello {
      nonce: String,
   },
   Attached {
      install_id: String,
   },
   Detached {
      install_id: String,
   },
   AttachError {
      install_id: String,
      error:      &'static str,
   },
   Message {
      install_id: String,
      id:         MessageId,
      ciphertext: String,
   },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientFrame {
   Attach {
      install_id: String,
      proof:      String,
      #[serde(default)]
      cursor:     Cursor,
   },
   Detach {
      install_id: String,
   },
   Ack {
      install_id: String,
      id:         MessageId,
   },
}

#[derive(Serialize)]
struct ProtectedPayload<'a> {
   kind:            &'a MessageKind,
   app_id:          &'a AppId,
   #[serde(skip_serializing_if = "Option::is_none")]
   connector_token: Option<&'a ConnectorToken>,
   payload:         String,
}

enum AttachmentState {
   Ready(Cursor),
   AwaitingAck(MessageId),
}

struct Attachment {
   state: AttachmentState,
   key:   PayloadKey,
}

struct V2Connection {
   connection_id: u64,
   event_sender:  mpsc::UnboundedSender<SocketEvent>,
   attachments:   HashMap<InstallId, Attachment>,
   nonce:         [u8; NONCE_BYTES],
   db:            Arc<Database>,
   hub:           Arc<SocketHub>,
}

impl V2Connection {
   fn new(db: Arc<Database>, hub: Arc<SocketHub>) -> (Self, mpsc::UnboundedReceiver<SocketEvent>) {
      let mut nonce = [0_u8; NONCE_BYTES];
      rand::rngs::OsRng.fill_bytes(&mut nonce);
      Self::with_nonce(db, hub, nonce)
   }

   fn with_nonce(
      db: Arc<Database>,
      hub: Arc<SocketHub>,
      nonce: [u8; NONCE_BYTES],
   ) -> (Self, mpsc::UnboundedReceiver<SocketEvent>) {
      let (connection_id, event_sender, events) = hub.new_connection();
      (
         Self {
            connection_id,
            event_sender,
            attachments: HashMap::new(),
            nonce,
            db,
            hub,
         },
         events,
      )
   }

   fn hello(&self) -> ServerFrame {
      ServerFrame::Hello {
         nonce: BASE64.encode(&self.nonce),
      }
   }

   async fn attach(&mut self, install_id: &InstallId, proof: &str, cursor: Cursor) -> Result<bool> {
      let Some(secret) = self.db.installation_secret(install_id).await? else {
         return Ok(false);
      };
      if !verify_attach_proof(&secret, &self.nonce, install_id, proof) {
         return Ok(false);
      }

      let maximum_cursor = self.db.max_outbox_id().await?;
      let cursor = cursor.clamp(maximum_cursor);
      self.db.ack_socket_through(install_id, cursor).await?;
      self.db.touch_installation(install_id).await?;
      self
         .hub
         .attach(install_id.as_ref(), self.connection_id, &self.event_sender);
      self.attachments.insert(install_id.clone(), Attachment {
         state: AttachmentState::Ready(cursor),
         key:   derive_payload_key(&secret)?,
      });
      info!(
         "WebSocket v2 connection {} attached {}",
         self.connection_id, install_id
      );
      Ok(true)
   }

   fn detach(&mut self, install_id: &InstallId) -> bool {
      let removed = self.attachments.remove(install_id).is_some();
      if removed {
         self.hub.detach(install_id.as_ref(), self.connection_id);
         info!(
            "WebSocket v2 connection {} detached {}",
            self.connection_id, install_id
         );
      }
      removed
   }

   async fn acknowledge(&mut self, install_id: &InstallId, id: MessageId) -> Result<bool> {
      let Some(attachment) = self.attachments.get(install_id) else {
         return Ok(false);
      };
      if !matches!(attachment.state, AttachmentState::AwaitingAck(expected) if expected == id) {
         return Ok(false);
      }
      if !self.db.ack_socket_message(install_id, id).await? {
         return Ok(false);
      }
      let attachment = self
         .attachments
         .get_mut(install_id)
         .expect("attachment vanished while acknowledging");
      attachment.state = AttachmentState::Ready(id.into());
      Ok(true)
   }

   async fn pending_messages(&mut self) -> Result<Vec<ServerFrame>> {
      let install_ids = self.attachments.keys().cloned().collect::<Vec<_>>();
      let mut frames = Vec::new();
      for install_id in install_ids {
         let Some(attachment) = self.attachments.get(&install_id) else {
            continue;
         };
         let AttachmentState::Ready(cursor) = attachment.state else {
            continue;
         };
         let key = attachment.key.clone();
         let Some(message) = self.db.next_socket_message(&install_id, cursor).await? else {
            continue;
         };
         let ciphertext = encrypt_message(&install_id, &message, &key)?;
         let message_id = message.id;
         if let Some(attachment) = self.attachments.get_mut(&install_id) {
            attachment.state = AttachmentState::AwaitingAck(message_id);
         }
         frames.push(ServerFrame::Message {
            install_id: install_id.to_string(),
            id: message_id,
            ciphertext,
         });
      }
      Ok(frames)
   }

   fn shutdown(&mut self) {
      for install_id in self.attachments.keys().cloned().collect::<Vec<_>>() {
         self.detach(&install_id);
      }
   }
}

pub(crate) async fn run_socket(upgrade: UpgradeFut, db: Arc<Database>, hub: Arc<SocketHub>) {
   let mut socket = match upgrade.await {
      Ok(socket) => socket,
      Err(e) => {
         warn!("WebSocket v2 upgrade failed: {e}");
         return;
      },
   };
   socket.set_max_message_size(64 * 1024);
   let mut socket = FragmentCollector::new(socket);
   let (mut connection, mut events) = V2Connection::new(db, hub);
   let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(30));
   heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

   if send_frame(&mut socket, &connection.hello()).await.is_err() {
      return;
   }
   info!(
      "WebSocket v2 connection {} opened",
      connection.connection_id
   );

   'connection: loop {
      let pending = match connection.pending_messages().await {
         Ok(pending) => pending,
         Err(e) => {
            error!(
               "Failed to load WebSocket v2 outbox for connection {}: {e}",
               connection.connection_id
            );
            break;
         },
      };
      for frame in pending {
         if send_frame(&mut socket, &frame).await.is_err() {
            break 'connection;
         }
      }

      tokio::select! {
          incoming = socket.read_frame() => {
              match incoming {
                  Ok(frame) if frame.opcode == OpCode::Text => {
                      let frame = match serde_json::from_slice::<ClientFrame>(&frame.payload) {
                          Ok(frame) => frame,
                          Err(e) => {
                              warn!(
                                  "Invalid WebSocket v2 message on connection {}: {e}",
                                  connection.connection_id
                              );
                              break;
                          }
                      };
                      match frame {
                          ClientFrame::Attach {
                              install_id,
                              proof,
                              cursor,
                          } => {
                              let attached = match InstallId::try_from(install_id.as_str()) {
                                  Ok(parsed) => connection.attach(&parsed, &proof, cursor).await,
                                  Err(_) => Ok(false),
                              };
                              match attached {
                                  Ok(true) => {
                                      if send_frame(
                                          &mut socket,
                                          &ServerFrame::Attached { install_id },
                                      ).await.is_err() {
                                          break;
                                      }
                                  }
                                  Ok(false) => {
                                      if send_frame(
                                          &mut socket,
                                          &ServerFrame::AttachError {
                                              install_id,
                                              error: "invalid proof",
                                          },
                                      ).await.is_err() {
                                          break;
                                      }
                                  }
                                  Err(e) => {
                                      error!(
                                          "WebSocket v2 attach failed on connection {}: {e}",
                                          connection.connection_id
                                      );
                                      break;
                                  }
                              }
                          }
                          ClientFrame::Detach { install_id } => {
                              let detached = InstallId::try_from(install_id.as_str())
                                  .is_ok_and(|parsed| connection.detach(&parsed));
                              if detached &&
                                  send_frame(
                                      &mut socket,
                                      &ServerFrame::Detached { install_id },
                                  ).await.is_err()
                              {
                                  break;
                              }
                          }
                          ClientFrame::Ack { install_id, id } => {
                              let parsed = InstallId::try_from(install_id.as_str());
                              let acknowledged = match &parsed {
                                  Ok(parsed) => connection.acknowledge(parsed, id).await,
                                  Err(_) => Ok(false),
                              };
                              match acknowledged {
                                  Ok(true) => {}
                                  Ok(false) => {
                                      warn!(
                                          "Invalid WebSocket v2 ack for {install_id}/{id}"
                                      );
                                      if let Ok(parsed) = &parsed {
                                          connection.detach(parsed);
                                      }
                                      if send_frame(
                                          &mut socket,
                                          &ServerFrame::Detached { install_id },
                                      ).await.is_err() {
                                          break;
                                      }
                                  }
                                  Err(e) => {
                                      error!(
                                          "WebSocket v2 ack failed for {install_id}/{id}: {e}"
                                      );
                                      break;
                                  }
                              }
                          }
                      }
                  }
                  Ok(frame) if frame.opcode == OpCode::Pong => {}
                  Ok(frame) if frame.opcode == OpCode::Close => break,
                  Ok(_) | Err(_) => break,
              }
          }
          event = events.recv() => {
              match event {
                  Some(SocketEvent::Wake) => {}
                  Some(SocketEvent::Replaced(install_id)) => {
                      let detached = InstallId::try_from(install_id.as_str())
                          .is_ok_and(|parsed| connection.detach(&parsed));
                      if detached &&
                          send_frame(
                              &mut socket,
                              &ServerFrame::Detached { install_id },
                          ).await.is_err()
                      {
                          break;
                      }
                  }
                  None => break,
              }
          }
          _ = heartbeat.tick() => {
              if send_ping(&mut socket).await.is_err() {
                  break;
              }
          }
      }
   }

   let _ = socket.write_frame(Frame::close(1000, &[])).await;
   let mut attached_install_ids = connection
      .attachments
      .keys()
      .map(ToString::to_string)
      .collect::<Vec<_>>();
   attached_install_ids.sort();
   connection.shutdown();
   info!(
      "WebSocket v2 connection {} closed with installs {:?}",
      connection.connection_id, attached_install_ids
   );
}

async fn send_frame<S>(
   socket: &mut FragmentCollector<S>,
   frame: &ServerFrame,
) -> Result<(), WebSocketError>
where
   S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
   let text = serde_json::to_string(frame).expect("socket v2 frame serialization failed");
   socket
      .write_frame(Frame::text(Payload::Owned(text.into_bytes())))
      .await
}

async fn send_ping<S>(socket: &mut FragmentCollector<S>) -> Result<(), WebSocketError>
where
   S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
   socket
      .write_frame(Frame::new(true, OpCode::Ping, None, Payload::Borrowed(&[])))
      .await
}

fn verify_attach_proof(
   secret: &InstallSecret,
   nonce: &[u8; NONCE_BYTES],
   install_id: &InstallId,
   encoded_proof: &str,
) -> bool {
   let Ok(proof) = BASE64.decode(encoded_proof.as_bytes()) else {
      return false;
   };
   let expected = attach_proof(secret, nonce, install_id);
   bool::from(expected.as_slice().ct_eq(&proof))
}

fn attach_proof(
   secret: &InstallSecret,
   nonce: &[u8; NONCE_BYTES],
   install_id: &InstallId,
) -> Vec<u8> {
   let mut mac = <HmacSha256 as hmac::KeyInit>::new_from_slice(secret.expose().as_bytes())
      .expect("HMAC accepts keys of any length");
   mac.update(nonce);
   mac.update(install_id.as_ref().as_bytes());
   mac.finalize().into_bytes().to_vec()
}

fn derive_payload_key(secret: &InstallSecret) -> Result<PayloadKey> {
   let mut key = [0_u8; 32];
   Hkdf::<Sha256>::new(None, secret.expose().as_bytes())
      .expand(KEY_CONTEXT, &mut key)
      .map_err(|_| anyhow::anyhow!("invalid WebSocket v2 HKDF output length"))?;
   Ok(PayloadKey::new(key))
}

fn message_aad(install_id: &InstallId, id: MessageId) -> Vec<u8> {
   let mut aad = Vec::with_capacity(install_id.as_ref().len() + 1 + std::mem::size_of::<i64>());
   aad.extend_from_slice(install_id.as_ref().as_bytes());
   aad.push(0);
   aad.extend_from_slice(&id.get().to_be_bytes());
   aad
}

fn encrypt_message(
   install_id: &InstallId,
   message: &OutboxMessage,
   key: &PayloadKey,
) -> Result<String> {
   let mut nonce = [0_u8; GCM_NONCE_BYTES];
   rand::rngs::OsRng.fill_bytes(&mut nonce);
   encrypt_message_with_nonce(install_id, message, key, nonce)
}

fn encrypt_message_with_nonce(
   install_id: &InstallId,
   message: &OutboxMessage,
   key: &PayloadKey,
   nonce: [u8; GCM_NONCE_BYTES],
) -> Result<String> {
   let plaintext = serde_json::to_vec(&ProtectedPayload {
      kind:            &message.kind,
      app_id:          &message.app_id,
      connector_token: message.connector_token.as_ref(),
      payload:         BASE64.encode(&message.payload),
   })
   .context("failed to serialize protected socket payload")?;
   let cipher = Aes256Gcm::new_from_slice(key.expose()).expect("AES-256 key length is fixed");
   let nonce_array = Nonce::try_from(nonce.as_slice()).expect("GCM nonce length is fixed");
   let encrypted = cipher
      .encrypt(&nonce_array, aes_gcm::aead::Payload {
         msg: &plaintext,
         aad: &message_aad(install_id, message.id),
      })
      .map_err(|_| anyhow::anyhow!("failed to encrypt socket payload"))?;
   let mut encoded = Vec::with_capacity(nonce.len() + encrypted.len());
   encoded.extend_from_slice(&nonce);
   encoded.extend_from_slice(&encrypted);
   Ok(BASE64.encode(&encoded))
}

#[cfg(test)]
mod tests {
   use aes_gcm::aead::Payload;
   use data_encoding::HEXLOWER;
   use sha2::{
      Digest,
      Sha256,
   };

   use super::*;
   use crate::db::Database;

   const INSTALL_A: &str = "0123456789abcdef0123456789abcdef";
   const INSTALL_B: &str = "fedcba9876543210fedcba9876543210";
   const SECRET_A: &str = "secret-a-secret-a-secret-a-secret-a";
   const SECRET_B: &str = "secret-b-secret-b-secret-b-secret-b";
   const VECTOR_CIPHERTEXT: &str = concat!(
      "AAECAwQFBgcICQoL9AHZKcFxQYT4PWLEcS4fdmCVgb0aETzWBkDgiUOblScCS4tz",
      "05xeD5t8o2EMmF0aEutZsUDfk9NiQYCmQ9LaSdKQfBsvoymIJXYAb5OJ5Jb7G+sx",
      "loRCGaP99D37xRNGvoG58/d2R0HbOQxjOll9vbjI",
   );
   const VECTOR_ID: i64 = 0x0102_0304_0506_0708;
   const VECTOR_NONCE: [u8; GCM_NONCE_BYTES] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
   const VECTOR_PLAINTEXT: &str = concat!(
      r#"{"kind":"fcm","app_id":"com.example.app","#,
      r#""connector_token":"connector-token","payload":"aGVsbG8="}"#,
   );

   async fn fresh_connection() -> (Arc<Database>, Arc<SocketHub>, V2Connection) {
      let db = Arc::new(Database::new(":memory:").await.unwrap());
      let hub = Arc::new(SocketHub::new());
      let (connection, _events) =
         V2Connection::with_nonce(db.clone(), hub.clone(), [7_u8; NONCE_BYTES]);
      (db, hub, connection)
   }

   async fn claim(db: &Database, install_id: &InstallId, secret: &InstallSecret) {
      db.claim_installation(
         install_id,
         &HEXLOWER.encode(&Sha256::digest(secret.expose().as_bytes())),
         secret,
      )
      .await
      .unwrap();
   }

   fn proof(secret: &str, install_id: &str) -> String {
      let secret = InstallSecret::from(secret);
      let install_id = InstallId::try_from(install_id).unwrap();
      BASE64.encode(&attach_proof(&secret, &[7_u8; NONCE_BYTES], &install_id))
   }

   fn decrypt(
      install_id: &str,
      id: MessageId,
      ciphertext: &str,
      secret: &str,
   ) -> serde_json::Value {
      let encoded = BASE64.decode(ciphertext.as_bytes()).unwrap();
      let (nonce, encrypted) = encoded.split_at(GCM_NONCE_BYTES);
      let install_id = InstallId::try_from(install_id).unwrap();
      let key = derive_payload_key(&InstallSecret::from(secret)).unwrap();
      let cipher = Aes256Gcm::new_from_slice(key.expose()).unwrap();
      let nonce = Nonce::try_from(nonce).unwrap();
      let plaintext = cipher
         .decrypt(&nonce, Payload {
            msg: encrypted,
            aad: &message_aad(&install_id, id),
         })
         .unwrap();
      serde_json::from_slice(&plaintext).unwrap()
   }

   #[test]
   fn payload_crypto_matches_cross_language_vector() {
      let install_id = InstallId::try_from(INSTALL_A).unwrap();
      let message = OutboxMessage {
         id:              MessageId::new(VECTOR_ID),
         install_id:      install_id.clone(),
         app_id:          AppId::from("com.example.app"),
         kind:            MessageKind::Fcm,
         connector_token: Some(ConnectorToken::try_from("connector-token").unwrap()),
         payload:         b"hello".to_vec(),
         endpoint:        None,
         attempts:        0,
      };
      let key = derive_payload_key(&InstallSecret::from(SECRET_A)).unwrap();
      let ciphertext =
         encrypt_message_with_nonce(&install_id, &message, &key, VECTOR_NONCE).unwrap();
      assert_eq!(ciphertext, VECTOR_CIPHERTEXT);

      let encoded = BASE64.decode(VECTOR_CIPHERTEXT.as_bytes()).unwrap();
      assert_eq!(&encoded[..GCM_NONCE_BYTES], VECTOR_NONCE);
      let plaintext = decrypt(
         INSTALL_A,
         MessageId::new(VECTOR_ID),
         VECTOR_CIPHERTEXT,
         SECRET_A,
      );
      assert_eq!(
         plaintext,
         serde_json::from_str::<serde_json::Value>(VECTOR_PLAINTEXT).unwrap()
      );
   }

   #[tokio::test]
   async fn bad_attach_proof_is_rejected() {
      let (db, hub, mut connection) = fresh_connection().await;
      let install_id = InstallId::try_from(INSTALL_A).unwrap();
      let secret = InstallSecret::from(SECRET_A);
      claim(&db, &install_id, &secret).await;

      assert!(
         !connection
            .attach(&install_id, &proof(SECRET_B, INSTALL_A), Cursor::default(),)
            .await
            .unwrap()
      );
      assert!(!hub.is_attached(INSTALL_A));
   }

   #[tokio::test]
   async fn two_attached_installs_route_only_their_encrypted_payloads() {
      let (db, hub, mut connection) = fresh_connection().await;
      let install_a = InstallId::try_from(INSTALL_A).unwrap();
      let install_b = InstallId::try_from(INSTALL_B).unwrap();
      let app_a = AppId::from("com.example.a");
      let app_b = AppId::from("com.example.b");
      claim(&db, &install_a, &InstallSecret::from(SECRET_A)).await;
      claim(&db, &install_b, &InstallSecret::from(SECRET_B)).await;
      db.enqueue_fcm_message(
         &install_a,
         &app_a,
         crate::types::Transport::WebSocket,
         Some("a-1"),
         b"payload-a",
      )
      .await
      .unwrap();
      db.enqueue_fcm_message(
         &install_b,
         &app_b,
         crate::types::Transport::WebSocket,
         Some("b-1"),
         b"payload-b",
      )
      .await
      .unwrap();

      assert!(
         connection
            .attach(&install_a, &proof(SECRET_A, INSTALL_A), Cursor::default(),)
            .await
            .unwrap()
      );
      assert!(
         connection
            .attach(&install_b, &proof(SECRET_B, INSTALL_B), Cursor::default(),)
            .await
            .unwrap()
      );
      assert_eq!(hub.active_count(), 2);

      let frames = connection.pending_messages().await.unwrap();
      assert_eq!(frames.len(), 2);
      let mut routed = HashMap::new();
      for frame in frames {
         let ServerFrame::Message {
            install_id,
            id,
            ciphertext,
         } = frame
         else {
            panic!("expected message frame");
         };
         let secret = if install_id == INSTALL_A {
            SECRET_A
         } else {
            SECRET_B
         };
         let plaintext = decrypt(&install_id, id, &ciphertext, secret);
         routed.insert(install_id, plaintext);
      }
      assert_eq!(routed[INSTALL_A]["app_id"], "com.example.a");
      assert_eq!(routed[INSTALL_A]["payload"], "cGF5bG9hZC1h");
      assert_eq!(routed[INSTALL_B]["app_id"], "com.example.b");
      assert_eq!(routed[INSTALL_B]["payload"], "cGF5bG9hZC1i");
   }

   #[tokio::test]
   async fn detach_leaves_outbox_for_cursor_replay_on_reattach() {
      let (db, hub, mut connection) = fresh_connection().await;
      let install_id = InstallId::try_from(INSTALL_A).unwrap();
      let app_id = AppId::from("com.example.a");
      claim(&db, &install_id, &InstallSecret::from(SECRET_A)).await;
      assert!(
         connection
            .attach(&install_id, &proof(SECRET_A, INSTALL_A), Cursor::default(),)
            .await
            .unwrap()
      );
      assert!(connection.detach(&install_id));
      assert!(!hub.is_attached(INSTALL_A));

      let message_id = db
         .enqueue_fcm_message(
            &install_id,
            &app_id,
            crate::types::Transport::WebSocket,
            Some("a-offline"),
            b"offline",
         )
         .await
         .unwrap()
         .unwrap();
      assert!(connection.pending_messages().await.unwrap().is_empty());
      assert_eq!(
         db.next_socket_message(&install_id, Cursor::default())
            .await
            .unwrap()
            .unwrap()
            .id,
         message_id
      );

      assert!(
         connection
            .attach(&install_id, &proof(SECRET_A, INSTALL_A), Cursor::default(),)
            .await
            .unwrap()
      );
      let frames = connection.pending_messages().await.unwrap();
      assert!(matches!(
          frames.as_slice(),
          [ServerFrame::Message { id, .. }] if *id == message_id
      ));
   }
}
