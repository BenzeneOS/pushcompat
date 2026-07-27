//! Database storage for app registrations

use std::collections::HashSet;

use anyhow::{
   Context,
   Result,
};
use rusqlite::params;
use tokio_rusqlite::Connection;

use crate::types::{
   AppId,
   ConnectorToken,
   Cursor,
   InstallId,
   InstallSecret,
   MessageId,
   MessageKind,
   Transport,
};

const SCHEMA_VERSION: i32 = 4;

#[derive(Debug, Clone)]
pub struct Registration {
   pub install_id:          InstallId,
   pub app_id:              AppId,
   pub secret_hash:         String,
   pub endpoint:            String,
   pub fcm_token:           Option<String>,
   pub firebase_app_id:     String,
   pub firebase_project_id: String,
   pub firebase_api_key:    String,
   pub cert_sha1:           Option<String>,
   pub app_version:         Option<i32>,
   pub app_version_name:    Option<String>,
   pub target_sdk:          Option<i32>,
   pub transport:           Transport,
}

#[derive(Debug, Clone)]
pub struct OutboxMessage {
   pub id:              MessageId,
   pub install_id:      InstallId,
   pub app_id:          AppId,
   pub kind:            MessageKind,
   pub connector_token: Option<ConnectorToken>,
   pub payload:         Vec<u8>,
   pub endpoint:        Option<String>,
   pub attempts:        u32,
}

#[derive(Debug, Clone)]
pub struct UnifiedPushRegistration {
   pub install_id:      InstallId,
   pub app_id:          AppId,
   pub connector_token: ConnectorToken,
   pub vapid_pubkey:    Option<String>,
}

pub struct Database {
   conn: Connection,
}

fn registration_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Registration> {
   Ok(Registration {
      install_id:          row.get(0)?,
      app_id:              row.get(1)?,
      secret_hash:         row.get(2)?,
      endpoint:            row.get(3)?,
      fcm_token:           row.get(4)?,
      firebase_app_id:     row.get(5)?,
      firebase_project_id: row.get(6)?,
      firebase_api_key:    row.get(7)?,
      cert_sha1:           row.get(8)?,
      app_version:         row.get(9)?,
      app_version_name:    row.get(10)?,
      target_sdk:          row.get(11)?,
      transport:           row.get(12)?,
   })
}

const CREATE_TABLES: &str = include_str!("../sql/schema.sql");

fn column_exists(conn: &rusqlite::Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
   let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
   let mut rows = stmt.query([])?;
   while let Some(row) = rows.next()? {
      if row.get::<_, String>(1)? == column {
         return Ok(true);
      }
   }
   Ok(false)
}

fn init_schema(conn: &mut rusqlite::Connection) -> rusqlite::Result<()> {
   let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
   if version >= SCHEMA_VERSION {
      return Ok(());
   }

   let tx = conn.transaction()?;

   tx.execute_batch(CREATE_TABLES)?;
   if version == 1 {
      tx.execute(
         "ALTER TABLE registrations
             ADD COLUMN transport TEXT NOT NULL DEFAULT 'unified_push'",
         [],
      )?;
   }
   if (1..=2).contains(&version) {
      tx.execute(
         "ALTER TABLE installations ADD COLUMN install_secret TEXT",
         [],
      )?;
   }
   tx.execute(
      "INSERT OR IGNORE INTO installations (install_id, secret_hash)
         SELECT install_id, secret_hash FROM registrations",
      [],
   )?;
   if !column_exists(&tx, "unified_push_registrations", "vapid_pubkey")? {
      tx.execute(
         "ALTER TABLE unified_push_registrations ADD COLUMN vapid_pubkey TEXT",
         [],
      )?;
   }
   tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
   tx.commit()
}

impl Database {
   pub async fn new(path: impl AsRef<std::path::Path>) -> Result<Self> {
      let conn = Connection::open(path)
         .await
         .context("Failed to open database")?;
      conn
         .call(|conn| -> rusqlite::Result<_> { Ok(init_schema(conn)?) })
         .await
         .context("Failed to initialize database schema")?;
      Ok(Self { conn })
   }

   pub async fn claim_installation(
      &self,
      install_id: &InstallId,
      secret_hash: &str,
      install_secret: &InstallSecret,
   ) -> Result<bool> {
      let install_id = install_id.as_ref().to_owned();
      let secret_hash = secret_hash.to_string();
      let install_secret = install_secret.expose().to_owned();
      let claimed = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let changed = conn.execute(
               "INSERT INTO installations (install_id, secret_hash, install_secret)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(install_id) DO UPDATE SET
                        install_secret = excluded.install_secret,
                        updated_at = CURRENT_TIMESTAMP
                     WHERE installations.secret_hash = excluded.secret_hash",
               params![install_id, secret_hash, install_secret],
            )?;
            Ok(changed > 0)
         })
         .await
         .context("Failed to claim installation")?;
      Ok(claimed)
   }

   pub async fn verify_installation(
      &self,
      install_id: &str,
      secret: &InstallSecret,
   ) -> Result<bool> {
      let install_id = install_id.to_string();
      let queried_install_id = install_id.clone();
      let candidate_secret = secret.expose().to_owned();
      let stored_hash = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let result = conn.query_row(
               "SELECT secret_hash FROM installations WHERE install_id = ?1",
               [queried_install_id],
               |row| row.get::<_, String>(0),
            );
            match result {
               Ok(hash) => Ok(Some(hash)),
               Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
               Err(e) => Err(e),
            }
         })
         .await
         .context("Failed to verify installation")?;
      let verified = crate::auth::verify_secret(secret, stored_hash.as_deref().unwrap_or(""));
      if verified {
         let install_id = install_id.to_string();
         self
            .conn
            .call(move |conn| -> rusqlite::Result<_> {
               conn.execute(
                  "UPDATE installations
                         SET install_secret = ?2, updated_at = CURRENT_TIMESTAMP
                         WHERE install_id = ?1",
                  params![install_id, candidate_secret],
               )?;
               Ok(())
            })
            .await
            .context("Failed to refresh installation secret")?;
      }
      Ok(verified)
   }

   pub async fn installation_secret(
      &self,
      install_id: &InstallId,
   ) -> Result<Option<InstallSecret>> {
      let install_id = install_id.as_ref().to_owned();
      let secret = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let result = conn.query_row(
               "SELECT install_secret FROM installations WHERE install_id = ?1",
               [install_id],
               |row| row.get::<_, Option<String>>(0),
            );
            match result {
               Ok(secret) => Ok(secret),
               Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
               Err(e) => Err(e),
            }
         })
         .await
         .context("Failed to load installation secret")?;
      Ok(secret.map(InstallSecret::from))
   }

   pub async fn touch_installation(&self, install_id: &InstallId) -> Result<()> {
      let install_id = install_id.clone();
      self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            conn.execute(
               "UPDATE installations SET updated_at = CURRENT_TIMESTAMP
                     WHERE install_id = ?1",
               [install_id],
            )?;
            Ok(())
         })
         .await
         .context("Failed to touch installation")?;
      Ok(())
   }

   pub async fn register_unified_push(
      &self,
      install_id: &InstallId,
      app_id: &AppId,
      connector_token: &ConnectorToken,
      endpoint_token: &str,
      vapid_pubkey: Option<&str>,
   ) -> Result<Option<String>> {
      let install_id = install_id.as_ref().to_owned();
      let app_id = app_id.as_ref().to_owned();
      let connector_token = connector_token.as_ref().to_owned();
      let endpoint_token = endpoint_token.to_string();
      let vapid_pubkey = vapid_pubkey.map(str::to_owned);
      let result = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let tx = conn.transaction()?;
            let existing = tx.query_row(
               "SELECT app_id, endpoint_token
                     FROM unified_push_registrations
                     WHERE install_id = ?1 AND connector_token = ?2",
               params![install_id, connector_token],
               |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            );
            let resolved = match existing {
               Ok((owner, endpoint)) if owner == app_id => {
                  tx.execute(
                     "UPDATE unified_push_registrations
                             SET updated_at = CURRENT_TIMESTAMP,
                                 vapid_pubkey = ?3
                             WHERE install_id = ?1 AND connector_token = ?2",
                     params![install_id, connector_token, vapid_pubkey],
                  )?;
                  Some(endpoint)
               },
               Ok(_) => None,
               Err(rusqlite::Error::QueryReturnedNoRows) => {
                  tx.execute(
                     "INSERT INTO unified_push_registrations
                             (install_id, app_id, connector_token, endpoint_token, vapid_pubkey)
                             VALUES (?1, ?2, ?3, ?4, ?5)",
                     params![
                        install_id,
                        app_id,
                        connector_token,
                        endpoint_token,
                        vapid_pubkey
                     ],
                  )?;
                  Some(endpoint_token)
               },
               Err(e) => return Err(e),
            };
            tx.commit()?;
            Ok(resolved)
         })
         .await
         .context("Failed to register UnifiedPush connector")?;
      Ok(result)
   }

   pub async fn unregister_unified_push(
      &self,
      install_id: &InstallId,
      app_id: &AppId,
      connector_token: &ConnectorToken,
   ) -> Result<bool> {
      let install_id = install_id.as_ref().to_owned();
      let app_id = app_id.as_ref().to_owned();
      let connector_token = connector_token.as_ref().to_owned();
      let deleted = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let tx = conn.transaction()?;
            let changed = tx.execute(
               "DELETE FROM unified_push_registrations
                     WHERE install_id = ?1 AND app_id = ?2 AND connector_token = ?3",
               params![install_id, app_id, connector_token],
            )?;
            if changed > 0 {
               tx.execute(
                  "DELETE FROM outbox
                         WHERE install_id = ?1 AND app_id = ?2
                           AND connector_token = ?3",
                  params![install_id, app_id, connector_token],
               )?;
            }
            tx.commit()?;
            Ok(changed > 0)
         })
         .await
         .context("Failed to unregister UnifiedPush connector")?;
      Ok(deleted)
   }

   pub async fn delete_stale_unified_push_registrations(
      &self,
      install_id: &InstallId,
      retained_tokens: &[ConnectorToken],
   ) -> Result<usize> {
      let install_id = install_id.as_ref().to_owned();
      let retained_tokens = retained_tokens
         .iter()
         .map(|token| token.as_ref().to_owned())
         .collect::<HashSet<_>>();
      let deleted = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let tx = conn.transaction()?;
            let stored_tokens = {
               let mut stmt = tx.prepare(
                  "SELECT connector_token FROM unified_push_registrations
                        WHERE install_id = ?1",
               )?;
               let rows = stmt.query_map([&install_id], |row| row.get::<_, String>(0))?;
               rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            let stale_tokens = stored_tokens
               .into_iter()
               .filter(|token| !retained_tokens.contains(token));
            let mut deleted = 0;
            for connector_token in stale_tokens {
               tx.execute(
                  "DELETE FROM outbox
                        WHERE install_id = ?1 AND connector_token = ?2",
                  params![install_id, connector_token],
               )?;
               deleted += tx.execute(
                  "DELETE FROM unified_push_registrations
                        WHERE install_id = ?1 AND connector_token = ?2",
                  params![install_id, connector_token],
               )?;
            }
            tx.commit()?;
            Ok(deleted)
         })
         .await
         .context("Failed to delete stale UnifiedPush registrations")?;
      Ok(deleted)
   }

   pub async fn get_unified_push_endpoint(
      &self,
      endpoint_token: &str,
   ) -> Result<Option<UnifiedPushRegistration>> {
      let endpoint_token = endpoint_token.to_string();
      let registration = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let result = conn.query_row(
               "SELECT install_id, app_id, connector_token, vapid_pubkey
                     FROM unified_push_registrations
                     WHERE endpoint_token = ?1",
               [endpoint_token],
               |row| {
                  Ok(UnifiedPushRegistration {
                     install_id:      row.get(0)?,
                     app_id:          row.get(1)?,
                     connector_token: row.get(2)?,
                     vapid_pubkey:    row.get(3)?,
                  })
               },
            );
            match result {
               Ok(registration) => Ok(Some(registration)),
               Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
               Err(error) => Err(error),
            }
         })
         .await
         .context("Failed to load UnifiedPush endpoint")?;
      Ok(registration)
   }

   pub async fn enqueue_fcm_message(
      &self,
      install_id: &InstallId,
      app_id: &AppId,
      transport: Transport,
      persistent_id: Option<&str>,
      payload: &[u8],
   ) -> Result<Option<MessageId>> {
      let install_id = install_id.as_ref().to_owned();
      let app_id = app_id.as_ref().to_owned();
      let kind = MessageKind::Fcm.as_ref().to_owned();
      let transport = transport.as_ref().to_owned();
      let persistent_id = persistent_id.map(str::to_string);
      let payload = payload.to_vec();
      let message_id = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let tx = conn.transaction()?;
            let was_acked = if let Some(pid) = persistent_id.as_deref() {
               tx.query_row(
                  "SELECT EXISTS(
                            SELECT 1 FROM acked_messages
                            WHERE install_id = ?1 AND app_id = ?2 AND persistent_id = ?3
                         )",
                  params![install_id, app_id, pid],
                  |row| row.get::<_, bool>(0),
               )?
            } else {
               false
            };

            let mut message_id = None;
            if !was_acked && !payload.is_empty() {
               tx.execute(
                  "INSERT OR IGNORE INTO outbox
                         (install_id, app_id, kind, transport, persistent_id, payload)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                  params![install_id, app_id, kind, transport, persistent_id, payload],
               )?;
               message_id = Some(tx.query_row(
                  "SELECT id FROM outbox
                         WHERE install_id = ?1 AND app_id = ?2
                           AND (
                               (?3 IS NOT NULL AND persistent_id = ?3)
                               OR (?3 IS NULL AND id = last_insert_rowid())
                           )
                         ORDER BY id DESC LIMIT 1",
                  params![install_id, app_id, persistent_id],
                  |row| Ok(MessageId::new(row.get(0)?)),
               )?);
            }

            if let Some(pid) = persistent_id {
               tx.execute(
                  "INSERT OR IGNORE INTO acked_messages
                         (install_id, app_id, persistent_id)
                         VALUES (?1, ?2, ?3)",
                  params![install_id, app_id, pid],
               )?;
               tx.execute(
                  "DELETE FROM acked_messages
                         WHERE install_id = ?1 AND app_id = ?2
                           AND rowid NOT IN (
                               SELECT rowid FROM acked_messages
                               WHERE install_id = ?1 AND app_id = ?2
                               ORDER BY rowid DESC LIMIT 500
                           )",
                  params![install_id, app_id],
               )?;
            }
            tx.commit()?;
            Ok(message_id)
         })
         .await
         .context("Failed to enqueue FCM message")?;
      Ok(message_id)
   }

   pub async fn enqueue_unified_push_message(
      &self,
      install_id: &InstallId,
      app_id: &AppId,
      connector_token: &ConnectorToken,
      payload: &[u8],
   ) -> Result<MessageId> {
      let install_id = install_id.as_ref().to_owned();
      let app_id = app_id.as_ref().to_owned();
      let connector_token = connector_token.as_ref().to_owned();
      let kind = MessageKind::UnifiedPush.as_ref().to_owned();
      let transport = Transport::WebSocket.as_ref().to_owned();
      let payload = payload.to_vec();
      let id = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            conn.execute(
               "INSERT INTO outbox
                     (install_id, app_id, kind, transport, connector_token, payload)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
               params![
                  install_id,
                  app_id,
                  kind,
                  transport,
                  connector_token,
                  payload
               ],
            )?;
            Ok(MessageId::new(conn.last_insert_rowid()))
         })
         .await
         .context("Failed to enqueue UnifiedPush message")?;
      Ok(id)
   }

   pub async fn next_socket_message(
      &self,
      install_id: &InstallId,
      cursor: Cursor,
   ) -> Result<Option<OutboxMessage>> {
      let install_id = install_id.clone();
      let message = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let result = conn.query_row(
               "SELECT id, install_id, app_id, kind, connector_token, payload, attempts
                     FROM outbox
                     WHERE install_id = ?1 AND transport = ?2 AND id > ?3
                     ORDER BY id LIMIT 1",
               params![install_id, Transport::WebSocket, cursor],
               |row| {
                  Ok(OutboxMessage {
                     id:              row.get(0)?,
                     install_id:      row.get(1)?,
                     app_id:          row.get(2)?,
                     kind:            row.get(3)?,
                     connector_token: row.get(4)?,
                     payload:         row.get(5)?,
                     endpoint:        None,
                     attempts:        row.get(6)?,
                  })
               },
            );
            match result {
               Ok(message) => Ok(Some(message)),
               Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
               Err(error) => Err(error),
            }
         })
         .await
         .context("Failed to load socket outbox")?;
      Ok(message)
   }

   pub async fn due_unified_push_messages(&self, limit: usize) -> Result<Vec<OutboxMessage>> {
      let row_limit = i64::try_from(limit).context("UnifiedPush delivery limit exceeds i64")?;
      let messages = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let mut stmt = conn.prepare(
               "SELECT o.id, o.install_id, o.app_id, o.kind, o.connector_token,
                            o.payload, r.endpoint, o.attempts
                     FROM outbox o
                     JOIN registrations r
                       ON r.install_id = o.install_id AND r.app_id = o.app_id
                     WHERE o.transport = ?1
                       AND o.next_attempt_at <= CURRENT_TIMESTAMP
                     ORDER BY o.id LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![Transport::UnifiedPush, row_limit], |row| {
               Ok(OutboxMessage {
                  id:              row.get(0)?,
                  install_id:      row.get(1)?,
                  app_id:          row.get(2)?,
                  kind:            row.get(3)?,
                  connector_token: row.get(4)?,
                  payload:         row.get(5)?,
                  endpoint:        Some(row.get(6)?),
                  attempts:        row.get(7)?,
               })
            })?;
            rows.collect()
         })
         .await
         .context("Failed to load due UnifiedPush messages")?;
      Ok(messages)
   }

   pub async fn ack_socket_message(&self, install_id: &InstallId, id: MessageId) -> Result<bool> {
      let install_id = install_id.clone();
      let transport = Transport::WebSocket.as_ref().to_owned();
      let deleted = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let changed = conn.execute(
               "DELETE FROM outbox
                     WHERE id = ?1 AND install_id = ?2 AND transport = ?3",
               params![id, install_id, transport],
            )?;
            Ok(changed == 1)
         })
         .await
         .context("Failed to acknowledge socket message")?;
      Ok(deleted)
   }

   pub async fn ack_socket_through(&self, install_id: &InstallId, cursor: Cursor) -> Result<usize> {
      let install_id = install_id.clone();
      let transport = Transport::WebSocket.as_ref().to_owned();
      let deleted = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let changed = conn.execute(
               "DELETE FROM outbox
                     WHERE install_id = ?1 AND transport = ?2 AND id <= ?3",
               params![install_id, transport, cursor],
            )?;
            Ok(changed)
         })
         .await
         .context("Failed to apply socket resume cursor")?;
      Ok(deleted)
   }

   pub async fn delete_outbox_message(&self, id: MessageId) -> Result<()> {
      self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            conn.execute("DELETE FROM outbox WHERE id = ?1", [id])?;
            Ok(())
         })
         .await
         .context("Failed to delete outbox message")?;
      Ok(())
   }

   pub async fn defer_outbox_message(&self, id: MessageId, seconds: i64) -> Result<()> {
      self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            conn.execute(
               "UPDATE outbox
                     SET attempts = attempts + 1,
                         next_attempt_at = datetime('now', ?2)
                     WHERE id = ?1",
               params![id, format!("+{seconds} seconds")],
            )?;
            Ok(())
         })
         .await
         .context("Failed to defer outbox message")?;
      Ok(())
   }

   pub async fn max_outbox_id(&self) -> Result<MessageId> {
      let id = self
         .conn
         .call(|conn| -> rusqlite::Result<_> {
            conn.query_row("SELECT COALESCE(MAX(id), 0) FROM outbox", [], |row| {
               row.get::<_, MessageId>(0)
            })
         })
         .await
         .context("Failed to load maximum outbox id")?;
      Ok(id)
   }

   /// Returns false when the row exists but is owned by a different secret,
   /// which means a concurrent request claimed the pair first.
   pub async fn save_registration(&self, reg: &Registration) -> Result<bool> {
      let reg = reg.clone();
      let saved = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let changed = conn.execute(
               "INSERT INTO registrations
                     (install_id, app_id, secret_hash, endpoint, fcm_token, firebase_app_id,
                      firebase_project_id, firebase_api_key, cert_sha1, app_version,
                      app_version_name, target_sdk, transport)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                     ON CONFLICT(install_id, app_id) DO UPDATE SET
                        secret_hash = excluded.secret_hash,
                        endpoint = excluded.endpoint,
                        fcm_token = excluded.fcm_token,
                        firebase_app_id = excluded.firebase_app_id,
                        firebase_project_id = excluded.firebase_project_id,
                        firebase_api_key = excluded.firebase_api_key,
                        cert_sha1 = excluded.cert_sha1,
                        app_version = excluded.app_version,
                        app_version_name = excluded.app_version_name,
                        target_sdk = excluded.target_sdk,
                        transport = excluded.transport,
                        updated_at = CURRENT_TIMESTAMP
                     WHERE registrations.secret_hash = excluded.secret_hash",
               params![
                  reg.install_id.as_ref(),
                  reg.app_id.as_ref(),
                  reg.secret_hash,
                  reg.endpoint,
                  reg.fcm_token,
                  reg.firebase_app_id,
                  reg.firebase_project_id,
                  reg.firebase_api_key,
                  reg.cert_sha1,
                  reg.app_version,
                  reg.app_version_name,
                  reg.target_sdk,
                  reg.transport.as_ref(),
               ],
            )?;
            Ok(changed > 0)
         })
         .await
         .context("Failed to save registration")?;
      Ok(saved)
   }

   pub async fn get_registration(
      &self,
      install_id: &InstallId,
      app_id: &AppId,
   ) -> Result<Option<Registration>> {
      let install_id = install_id.clone();
      let app_id = app_id.clone();
      let result = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let result = conn.query_row(
               "SELECT install_id, app_id, secret_hash, endpoint, fcm_token,
                            firebase_app_id, firebase_project_id, firebase_api_key,
                            cert_sha1, app_version, app_version_name, target_sdk, transport
                     FROM registrations WHERE install_id = ?1 AND app_id = ?2",
               params![install_id, app_id],
               registration_from_row,
            );
            match result {
               Ok(registration) => Ok(Some(registration)),
               Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
               Err(error) => Err(error),
            }
         })
         .await
         .context("Failed to get registration")?;
      Ok(result)
   }

   pub async fn delete_registration(&self, install_id: &InstallId, app_id: &AppId) -> Result<()> {
      let install_id = install_id.as_ref().to_owned();
      let app_id = app_id.as_ref().to_owned();
      let kind = MessageKind::Fcm.as_ref().to_owned();
      self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            for table in ["registrations", "fcm_sessions", "acked_messages"] {
               conn.execute(
                  &format!("DELETE FROM {table} WHERE install_id = ?1 AND app_id = ?2"),
                  params![install_id, app_id],
               )?;
            }
            conn.execute(
               "DELETE FROM outbox
                     WHERE install_id = ?1 AND app_id = ?2 AND kind = ?3",
               params![install_id, app_id, kind],
            )?;
            Ok(())
         })
         .await
         .context("Failed to delete registration")?;
      Ok(())
   }

   pub async fn list_registrations(&self) -> Result<Vec<Registration>> {
      let result = self
         .conn
         .call(|conn| -> rusqlite::Result<_> {
            let mut stmt = conn.prepare(
               "SELECT install_id, app_id, secret_hash, endpoint, fcm_token,
                            firebase_app_id, firebase_project_id, firebase_api_key,
                            cert_sha1, app_version, app_version_name, target_sdk, transport
                     FROM registrations",
            )?;
            let rows = stmt.query_map([], registration_from_row)?;
            rows.collect()
         })
         .await
         .context("Failed to list registrations")?;
      Ok(result)
   }

   pub async fn count_registrations(&self) -> Result<usize> {
      let count = self
         .conn
         .call(|conn| -> rusqlite::Result<_> {
            let count = conn.query_row("SELECT COUNT(*) FROM registrations", [], |row| {
               row.get::<_, i64>(0)
            })?;
            Ok(count as usize)
         })
         .await
         .context("Failed to count registrations")?;
      Ok(count)
   }

   /// Reap rows whose shim has stopped heartbeating (app data cleared, app
   /// uninstalled). Every row has a secret and therefore a shim that
   /// re-registers daily, so silence past the cutoff means the install is
   /// genuinely gone.
   pub async fn prune_stale(&self, max_age_days: u32) -> Result<Vec<(InstallId, AppId)>> {
      let pruned = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let cutoff = format!("-{max_age_days} days");
            let mut stmt = conn.prepare(
               "SELECT install_id, app_id FROM registrations
                     WHERE updated_at < datetime('now', ?1)",
            )?;
            let pairs = stmt
               .query_map([&cutoff], |row| {
                  Ok((row.get::<_, InstallId>(0)?, row.get::<_, AppId>(1)?))
               })?
               .collect::<rusqlite::Result<Vec<_>>>()?;
            let kind = MessageKind::Fcm.as_ref();
            for (install_id, app_id) in &pairs {
               for table in ["registrations", "fcm_sessions", "acked_messages"] {
                  conn.execute(
                     &format!("DELETE FROM {table} WHERE install_id = ?1 AND app_id = ?2"),
                     params![install_id.as_ref(), app_id.as_ref()],
                  )?;
               }
               conn.execute(
                  "DELETE FROM outbox
                         WHERE install_id = ?1 AND app_id = ?2 AND kind = ?3",
                  params![install_id.as_ref(), app_id.as_ref(), kind],
               )?;
            }
            Ok(pairs)
         })
         .await
         .context("Failed to prune stale registrations")?;
      Ok(pruned)
   }

   /// Most recent acks first; the list is capped because MCS login carries it
   /// inline.
   pub async fn recent_acks(
      &self,
      install_id: &InstallId,
      app_id: &AppId,
      limit: usize,
   ) -> Result<Vec<String>> {
      let install_id = install_id.as_ref().to_owned();
      let app_id = app_id.as_ref().to_owned();
      let result = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let mut stmt = conn.prepare(
               "SELECT persistent_id FROM acked_messages
                     WHERE install_id = ?1 AND app_id = ?2
                     ORDER BY rowid DESC LIMIT ?3",
            )?;
            let rows =
               stmt.query_map(params![install_id, app_id, limit as i64], |row| row.get(0))?;
            let mut ids = Vec::new();
            for row in rows {
               ids.push(row?);
            }
            Ok(ids)
         })
         .await
         .context("Failed to load acks")?;
      Ok(result)
   }

   pub async fn save_fcm_session(
      &self,
      install_id: &InstallId,
      app_id: &AppId,
      data: &str,
   ) -> Result<()> {
      let install_id = install_id.as_ref().to_owned();
      let app_id = app_id.as_ref().to_owned();
      let data = data.to_string();
      self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            conn.execute(
               "INSERT OR REPLACE INTO fcm_sessions (install_id, app_id, registration_data)
                     VALUES (?1, ?2, ?3)",
               params![install_id, app_id, data],
            )?;
            Ok(())
         })
         .await
         .context("Failed to save FCM session")?;
      Ok(())
   }

   pub async fn get_fcm_session(
      &self,
      install_id: &InstallId,
      app_id: &AppId,
   ) -> Result<Option<String>> {
      let install_id = install_id.as_ref().to_owned();
      let app_id = app_id.as_ref().to_owned();
      let result = self
         .conn
         .call(move |conn| -> rusqlite::Result<_> {
            let result: Result<String, _> = conn.query_row(
               "SELECT registration_data FROM fcm_sessions
                     WHERE install_id = ?1 AND app_id = ?2",
               params![install_id, app_id],
               |row| row.get(0),
            );

            match result {
               Ok(data) => Ok(Some(data)),
               Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
               Err(e) => Err(e),
            }
         })
         .await
         .context("Failed to get FCM session")?;
      Ok(result)
   }
}

#[cfg(test)]
mod tests {
   use super::*;

   async fn fresh_db() -> Database {
      Database::new(":memory:").await.unwrap()
   }

   #[tokio::test]
   async fn unified_push_registration_can_clear_vapid_key() {
      let db = fresh_db().await;
      let install_id = InstallId::try_from("0123456789abcdef").unwrap();
      let app_id = AppId::from("com.app");
      let connector_token = ConnectorToken::try_from("connector-1").unwrap();
      let secret = InstallSecret::from("secret");

      assert!(
         db.claim_installation(&install_id, "secret-hash", &secret)
            .await
            .unwrap()
      );
      assert_eq!(
         db.register_unified_push(
            &install_id,
            &app_id,
            &connector_token,
            "endpoint-1",
            Some("vapid-key"),
         )
         .await
         .unwrap(),
         Some("endpoint-1".to_string())
      );
      assert_eq!(
         db.get_unified_push_endpoint("endpoint-1")
            .await
            .unwrap()
            .unwrap()
            .vapid_pubkey
            .as_deref(),
         Some("vapid-key")
      );

      assert_eq!(
         db.register_unified_push(&install_id, &app_id, &connector_token, "endpoint-2", None,)
            .await
            .unwrap(),
         Some("endpoint-1".to_string())
      );
      assert_eq!(
         db.get_unified_push_endpoint("endpoint-1")
            .await
            .unwrap()
            .unwrap()
            .vapid_pubkey,
         None
      );
   }

   #[tokio::test]
   async fn unified_push_reconcile_prunes_only_one_installation() {
      let db = fresh_db().await;
      let first_install = InstallId::try_from("0123456789abcdef").unwrap();
      let second_install = InstallId::try_from("fedcba9876543210").unwrap();
      let app_id = AppId::from("com.app");
      let keyed_app_id = AppId::from("im.molly.app");
      let first_token = ConnectorToken::try_from("first-token").unwrap();
      let stale_token = ConnectorToken::try_from("stale-token").unwrap();
      let keyed_token = ConnectorToken::try_from("keyed-token").unwrap();
      let other_install_token = ConnectorToken::try_from("other-install-token").unwrap();
      let secret = InstallSecret::from("secret");

      assert!(
         db.claim_installation(&first_install, "first-hash", &secret)
            .await
            .unwrap()
      );
      assert!(
         db.claim_installation(&second_install, "second-hash", &secret)
            .await
            .unwrap()
      );
      for (install_id, app_id, connector_token, endpoint_token, vapid) in [
         (
            &first_install,
            &app_id,
            &first_token,
            "first-endpoint",
            None,
         ),
         (
            &first_install,
            &app_id,
            &stale_token,
            "stale-endpoint",
            None,
         ),
         (
            &first_install,
            &keyed_app_id,
            &keyed_token,
            "keyed-endpoint",
            Some("vapid-key"),
         ),
         (
            &second_install,
            &app_id,
            &other_install_token,
            "other-endpoint",
            None,
         ),
      ] {
         db.register_unified_push(install_id, app_id, connector_token, endpoint_token, vapid)
            .await
            .unwrap()
            .unwrap();
      }

      assert_eq!(
         db.delete_stale_unified_push_registrations(&first_install, &[
            first_token.clone(),
            keyed_token.clone()
         ],)
            .await
            .unwrap(),
         1
      );
      assert!(
         db.get_unified_push_endpoint("stale-endpoint")
            .await
            .unwrap()
            .is_none()
      );
      assert_eq!(
         db.get_unified_push_endpoint("keyed-endpoint")
            .await
            .unwrap()
            .unwrap()
            .vapid_pubkey
            .as_deref(),
         Some("vapid-key")
      );
      assert!(
         db.get_unified_push_endpoint("other-endpoint")
            .await
            .unwrap()
            .is_some()
      );
   }

   #[test]
   fn v3_database_gains_vapid_pubkey_without_losing_rows() {
      let mut conn = rusqlite::Connection::open_in_memory().unwrap();
      conn
         .execute_batch(
            "CREATE TABLE unified_push_registrations (
                install_id TEXT NOT NULL,
                app_id TEXT NOT NULL,
                connector_token TEXT NOT NULL,
                endpoint_token TEXT NOT NULL UNIQUE,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (install_id, connector_token)
             );
             INSERT INTO unified_push_registrations
                (install_id, app_id, connector_token, endpoint_token)
                VALUES ('0123456789abcdef', 'com.app', 'connector-1', 'endpoint-1');",
         )
         .unwrap();
      conn.pragma_update(None, "user_version", 3).unwrap();

      init_schema(&mut conn).unwrap();

      assert!(column_exists(&conn, "unified_push_registrations", "vapid_pubkey").unwrap());

      let version: i32 = conn
         .query_row("PRAGMA user_version", [], |row| row.get(0))
         .unwrap();
      assert_eq!(version, 4);

      let (app_id, endpoint_token, vapid_pubkey): (String, String, Option<String>) = conn
         .query_row(
            "SELECT app_id, endpoint_token, vapid_pubkey
                  FROM unified_push_registrations WHERE install_id = ?1",
            ["0123456789abcdef"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
         )
         .unwrap();
      assert_eq!(app_id, "com.app");
      assert_eq!(endpoint_token, "endpoint-1");
      assert_eq!(vapid_pubkey, None);

      conn.pragma_update(None, "user_version", 3).unwrap();
      init_schema(&mut conn).unwrap();
      let version: i32 = conn
         .query_row("PRAGMA user_version", [], |row| row.get(0))
         .unwrap();
      assert_eq!(version, 4);
      let row_count: i64 = conn
         .query_row(
            "SELECT COUNT(*) FROM unified_push_registrations",
            [],
            |row| row.get(0),
         )
         .unwrap();
      assert_eq!(row_count, 1);
   }

   #[tokio::test]
   async fn socket_outbox_replays_until_ack_and_deduplicates_fcm() {
      let db = fresh_db().await;
      let install_id = InstallId::try_from("0123456789abcdef").unwrap();
      let app_id = AppId::from("com.app");
      let id = db
         .enqueue_fcm_message(
            &install_id,
            &app_id,
            Transport::WebSocket,
            Some("persistent-1"),
            br#"{"google.message_id":"message-1"}"#,
         )
         .await
         .unwrap()
         .unwrap();

      assert_eq!(
         db.enqueue_fcm_message(
            &install_id,
            &app_id,
            Transport::WebSocket,
            Some("persistent-1"),
            b"duplicate",
         )
         .await
         .unwrap(),
         None
      );
      let pending = db
         .next_socket_message(&install_id, Cursor::default())
         .await
         .unwrap()
         .unwrap();
      assert_eq!(pending.id, id);
      assert_eq!(pending.kind, MessageKind::Fcm);
      assert!(db.ack_socket_message(&install_id, id).await.unwrap());
      assert!(
         db.next_socket_message(&install_id, Cursor::default())
            .await
            .unwrap()
            .is_none()
      );
   }
}
