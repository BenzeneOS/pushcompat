//! PushCompat bridge - FCM compatibility relay server
//!
//! This server:
//! 1. Accepts app registrations with Firebase credentials and UP endpoints
//! 2. Maintains FCM connections for each registered app
//! 3. Forwards FCM messages to UP endpoints as raw bytes

mod auth;
mod db;
mod delivery;
mod fcm;
mod socket;
mod socket_v2;
mod types;
mod validate;
mod vapid;

use std::{
   collections::HashMap,
   path::{
      Path as FsPath,
      PathBuf,
   },
   sync::{
      Arc,
      Mutex,
      atomic::{
         AtomicU64,
         Ordering,
      },
   },
   time::{
      SystemTime,
      UNIX_EPOCH,
   },
};

use axum::{
   Json,
   Router,
   body::Bytes,
   extract::{
      DefaultBodyLimit,
      Path,
      State,
   },
   http::{
      HeaderMap,
      StatusCode,
   },
   routing::{
      get,
      post,
   },
};
use data_encoding::HEXLOWER;
use pound::Parse;
use rand::RngCore;
use serde::{
   Deserialize,
   Serialize,
};
use tokio::{
   net::TcpListener,
   sync::RwLock,
};
use tracing::{
   error,
   info,
   level_filters::LevelFilter,
   warn,
};

use crate::types::{
   AppId,
   ConnectorToken,
   Transport,
};

#[derive(Debug, Parse)]
#[pound(name = "pushcompat-bridge")]
struct Cli {
   /// HTTP server port
   #[pound(long, default = "8080", min = "1")]
   port: u16,

   /// SQLite database path
   #[pound(long, default = "pushcompat.db")]
   db_path: PathBuf,

   /// Allowed UnifiedPush endpoint host; repeat for multiple hosts
   #[pound(long)]
   endpoint_host: Vec<String>,

   /// Public origin required as the VAPID `aud` claim, e.g.
   /// https://push.benzeneos.org. VAPID enforcement is disabled while unset
   #[pound(long)]
   public_origin: Option<String>,

   /// Kill switch: VAPID enforcement is skipped while this file exists
   #[pound(long)]
   vapid_kill_file: Option<PathBuf>,

   /// Maximum log level
   #[pound(long, default = "info", parse = "parse_log_level")]
   log_level: LevelFilter,
}

fn parse_log_level(value: &str) -> Result<LevelFilter, &'static str> {
   value
      .parse()
      .map_err(|_| "expected off, error, warn, info, debug, or trace")
}

#[derive(Clone)]
pub(crate) struct AppState {
   db:                 Arc<db::Database>,
   fcm_manager:        Arc<RwLock<fcm::FcmManager>>,
   delivery:           Arc<delivery::DeliveryManager>,
   socket_hub:         Arc<socket::SocketHub>,
   endpoint_allowlist: Arc<Vec<String>>,
   vapid_origin:       Option<Arc<str>>,
   vapid_kill_file:    Option<Arc<FsPath>>,
   vapid_counters:     Arc<VapidCounters>,
}

impl AppState {
   fn enforcing_origin(&self) -> Option<&str> {
      enforcing_origin(
         self.vapid_origin.as_deref(),
         self.vapid_kill_file.as_deref(),
      )
   }

   fn vapid_enforcing(&self) -> bool {
      self.enforcing_origin().is_some()
   }
}

/// The origin to require as the `aud` claim, or `None` while enforcement is
/// unconfigured or suspended by the kill file. The kill file is re-checked per
/// request so enforcement can be dropped without a redeploy.
fn enforcing_origin<'a>(origin: Option<&'a str>, kill_file: Option<&FsPath>) -> Option<&'a str> {
   if kill_file.is_some_and(FsPath::exists) {
      return None;
   }
   origin
}

/// Counter key for the one rejection reason that is not an RFC 8292 outcome.
const REASON_BAD_ENCODING: &str = "bad_encoding";

#[derive(Default)]
struct VapidCounters {
   accepted: AtomicU64,
   rejected: Mutex<HashMap<&'static str, u64>>,
}

impl VapidCounters {
   fn record_accepted(&self) {
      self.accepted.fetch_add(1, Ordering::Relaxed);
   }

   fn record_reason(&self, reason: &'static str) {
      if let Ok(mut rejected) = self.rejected.lock() {
         *rejected.entry(reason).or_default() += 1;
      }
   }

   fn snapshot(&self) -> (u64, HashMap<String, u64>) {
      let rejected = self
         .rejected
         .lock()
         .map(|rejected| {
            rejected
               .iter()
               .map(|(reason, count)| ((*reason).to_string(), *count))
               .collect()
         })
         .unwrap_or_default();
      (self.accepted.load(Ordering::Relaxed), rejected)
   }
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
   /// UnifiedPush endpoint URL. Omitted by ROM clients using the socket
   /// transport.
   #[serde(default)]
   endpoint:            Option<String>,
   #[serde(default)]
   transport:           Option<String>,
   /// FCM token from the app (not used for server-side FCM, but stored for
   /// reference)
   #[serde(default)]
   fcm_token:           Option<String>,
   /// App package name
   app_id:              String,
   /// Opaque random per-install identifier (paired with the bearer secret)
   #[serde(default)]
   install_id:          Option<String>,
   /// Firebase credentials (required for initial registration)
   #[serde(default)]
   firebase_app_id:     Option<String>,
   #[serde(default)]
   firebase_project_id: Option<String>,
   #[serde(default)]
   firebase_api_key:    Option<String>,
   /// Original APK signing certificate SHA1 (lowercase hex, no colons)
   #[serde(default)]
   cert_sha1:           Option<String>,
   /// App version code (versionCode from APK)
   #[serde(default)]
   app_version:         Option<i32>,
   /// App version name (versionName from APK)
   #[serde(default)]
   app_version_name:    Option<String>,
   /// Target SDK version from APK
   #[serde(default)]
   target_sdk:          Option<i32>,
}

#[derive(Debug, Deserialize)]
struct UnregisterRequest {
   app_id:     String,
   #[serde(default)]
   install_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct RegisterResponse {
   success:   bool,
   message:   String,
   #[serde(skip_serializing_if = "Option::is_none")]
   fcm_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
   status:             String,
   registered_apps:    usize,
   active_connections: usize,
   active_sockets:     usize,
   vapid_enforcing:    bool,
   vapid_accepted:     u64,
   vapid_rejected:     HashMap<String, u64>,
}

#[derive(Debug, Deserialize)]
struct DistributorRegisterRequest {
   install_id:      String,
   app_id:          String,
   connector_token: String,
   #[serde(default)]
   vapid:           Option<String>,
}

#[derive(Debug, Serialize)]
struct DistributorRegisterResponse {
   endpoint_token: String,
}

#[derive(Debug, Deserialize)]
struct DistributorUnregisterRequest {
   install_id:      String,
   app_id:          String,
   connector_token: String,
}

#[derive(Debug, Deserialize)]
struct DistributorReconcileRequest {
   install_id:    String,
   registrations: Vec<DistributorReconcileEntry>,
}

#[derive(Debug, Deserialize)]
struct DistributorReconcileEntry {
   app_id:          String,
   connector_token: String,
   #[serde(default)]
   vapid:           Option<String>,
}

#[derive(Debug, Serialize)]
struct DistributorReconcileResponse {
   endpoints: Vec<DistributorReconcileResult>,
}

#[derive(Debug, Serialize)]
struct DistributorReconcileResult {
   connector_token: String,
   endpoint_token:  String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
   let cli = Cli::parse();
   tracing_subscriber::fmt()
      .with_max_level(cli.log_level)
      .init();

   let endpoint_allowlist = cli
      .endpoint_host
      .into_iter()
      .map(|host| host.trim().to_string())
      .filter(|host| !host.is_empty())
      .collect::<Vec<String>>();
   if !endpoint_allowlist.is_empty() {
      info!("Endpoint allowlist: {endpoint_allowlist:?}");
   }

   let db = Arc::new(db::Database::new(&cli.db_path).await?);

   let socket_hub = Arc::new(socket::SocketHub::new());
   let delivery = Arc::new(delivery::DeliveryManager::new(
      db.clone(),
      socket_hub.clone(),
   ));
   delivery.start();

   let fcm_manager = Arc::new(RwLock::new(fcm::FcmManager::new()));

   if cli.public_origin.is_none() {
      warn!("No --public-origin set: VAPID enforcement is DISABLED for web push endpoints");
   }
   if let Some(kill_file) = cli.vapid_kill_file.as_deref() {
      // Path::exists() reports false for an unreadable parent, so a kill switch
      // installed somewhere the process cannot traverse would never fire and
      // never say why.
      match std::fs::metadata(kill_file) {
         Ok(_) => {
            warn!(
               "VAPID kill file {} is present: enforcement is SUSPENDED",
               kill_file.display()
            );
         },
         Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            info!(
               "VAPID kill switch armed: create {} to suspend enforcement",
               kill_file.display()
            );
         },
         Err(e) => {
            warn!(
               "VAPID kill file {} is unreachable ({e}): the kill switch will never take effect",
               kill_file.display()
            );
         },
      }
   }
   let state = AppState {
      db,
      fcm_manager,
      delivery,
      socket_hub,
      endpoint_allowlist: Arc::new(endpoint_allowlist),
      vapid_origin: cli.public_origin.map(Arc::from),
      vapid_kill_file: cli.vapid_kill_file.map(Arc::from),
      vapid_counters: Arc::new(VapidCounters::default()),
   };

   // Restore existing registrations
   restore_registrations(state.clone()).await?;

   // Rows orphaned by an app-data clear never unregister themselves; the shim
   // heartbeats via re-register, so anything silent for 90 days is dead.
   let prune_state = state.clone();
   tokio::spawn(async move {
      let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
      loop {
         interval.tick().await;
         match prune_state.db.prune_stale(90).await {
            Ok(pruned) => {
               for (install_id, app_id) in pruned {
                  prune_state
                     .fcm_manager
                     .write()
                     .await
                     .stop_listener(&install_id, &app_id);
                  info!("Pruned stale registration {}/{}", install_id, app_id);
               }
            },
            Err(e) => error!("Prune failed: {}", e),
         }
      }
   });

   let app = Router::new()
      .route("/health", get(health))
      .route("/register", post(register))
      .route("/unregister", post(unregister))
      .route("/socket", get(socket::upgrade))
      .route("/distributor/register", post(distributor_register))
      .route("/distributor/unregister", post(distributor_unregister))
      .route("/distributor/reconcile", post(distributor_reconcile))
      .route("/up/{endpoint_token}", post(unified_push_message))
      .layer(DefaultBodyLimit::max(64 * 1024))
      .with_state(state);

   let addr = format!("[::]:{}", cli.port);
   info!("PushCompat bridge listening on {addr}");

   let listener = TcpListener::bind(&addr).await?;
   axum::serve(listener, app).await?;

   Ok(())
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
   let apps = state.db.count_registrations().await.unwrap_or(0);
   let connections = state.fcm_manager.read().await.active_count();
   let (vapid_accepted, vapid_rejected) = state.vapid_counters.snapshot();

   Json(HealthResponse {
      status: "ok".to_string(),
      registered_apps: apps,
      active_connections: connections,
      active_sockets: state.socket_hub.active_count(),
      vapid_enforcing: state.vapid_enforcing(),
      vapid_accepted,
      vapid_rejected,
   })
}

async fn register(
   State(state): State<AppState>,
   headers: HeaderMap,
   Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, String)> {
   info!("Registration request for app: {}", req.app_id);

   let app_id = AppId::from(req.app_id.as_str());
   let transport = Transport::try_from(
      req.transport
         .as_deref()
         .unwrap_or(Transport::UnifiedPush.as_ref()),
   )
   .map_err(|_| {
      (
         StatusCode::BAD_REQUEST,
         "unsupported delivery transport".to_string(),
      )
   })?;
   let endpoint = match transport {
      Transport::UnifiedPush => {
         let endpoint = req.endpoint.as_deref().ok_or_else(|| {
            (
               StatusCode::BAD_REQUEST,
               "UnifiedPush endpoint required".to_string(),
            )
         })?;
         if let Err(reason) = validate::validate_endpoint(endpoint, &state.endpoint_allowlist) {
            return Err((StatusCode::BAD_REQUEST, reason));
         }
         endpoint.to_string()
      },
      Transport::WebSocket => String::new(),
   };
   let target = delivery::DeliveryTarget::from_parts(transport, &endpoint)
      .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

   let secret = auth::bearer_secret(&headers);
   let identity = auth::authorize_register(
      &state.db,
      &app_id,
      req.install_id.as_deref(),
      secret.as_ref(),
   )
   .await?;

   let existing = state
      .db
      .get_registration(&identity.install_id, &app_id)
      .await
      .map_err(|e| {
         error!("Database error: {}", e);
         (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
         )
      })?;

   let (firebase_app_id, firebase_project_id, firebase_api_key) = match (
      &req.firebase_app_id,
      &req.firebase_project_id,
      &req.firebase_api_key,
   ) {
      (Some(app_id), Some(project_id), Some(api_key)) => {
         (app_id.clone(), project_id.clone(), api_key.clone())
      },
      _ => {
         match &existing {
            Some(reg) => {
               (
                  reg.firebase_app_id.clone(),
                  reg.firebase_project_id.clone(),
                  reg.firebase_api_key.clone(),
               )
            },
            None => {
               return Err((
                  StatusCode::BAD_REQUEST,
                  "Firebase credentials required for first registration".to_string(),
               ));
            },
         }
      },
   };

   let registration = db::Registration {
      install_id: identity.install_id.clone(),
      app_id: app_id.clone(),
      secret_hash: identity.secret_hash.clone(),
      endpoint,
      fcm_token: req.fcm_token.clone(),
      firebase_app_id,
      firebase_project_id,
      firebase_api_key,
      cert_sha1: req.cert_sha1.clone(),
      app_version: req.app_version,
      app_version_name: req.app_version_name.clone(),
      target_sdk: req.target_sdk,
      transport,
   };

   match state.db.save_registration(&registration).await {
      // The write re-checks ownership, so losing a concurrent claim surfaces
      // here rather than silently overwriting whoever won it.
      Ok(true) => {},
      Ok(false) => {
         warn!("Lost claim race for {}/{}", identity.install_id, app_id);
         return Err((StatusCode::UNAUTHORIZED, "invalid credentials".to_string()));
      },
      Err(e) => {
         error!("Failed to save registration: {}", e);
         return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to save registration".to_string(),
         ));
      },
   }

   let fcm_token = match state
      .fcm_manager
      .write()
      .await
      .start_listener(
         registration.install_id.clone(),
         registration.app_id.clone(),
         registration.firebase_app_id.clone(),
         registration.firebase_project_id.clone(),
         registration.firebase_api_key.clone(),
         registration.cert_sha1.clone(),
         registration.app_version,
         registration.app_version_name.clone(),
         registration.target_sdk,
         target,
         state.db.clone(),
         state.delivery.clone(),
      )
      .await
   {
      Ok(token) => {
         info!(
            "FCM listener started for {}/{}",
            identity.install_id, app_id
         );
         Some(token)
      },
      Err(e) => {
         error!("Failed to start FCM listener for {}: {e}", req.app_id);
         // Still return success since registration was saved
         None
      },
   };

   Ok(Json(RegisterResponse {
      success: true,
      message: "Registration successful".to_string(),
      fcm_token,
   }))
}

async fn unregister(
   State(state): State<AppState>,
   headers: HeaderMap,
   Json(req): Json<UnregisterRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
   let app_id = AppId::from(req.app_id.as_str());
   let secret = auth::bearer_secret(&headers);
   let install_id = auth::authorize_unregister(
      &state.db,
      &app_id,
      req.install_id.as_deref(),
      secret.as_ref(),
   )
   .await?;

   info!("Unregister request for {install_id}/{app_id}");

   state
      .fcm_manager
      .write()
      .await
      .stop_listener(&install_id, &app_id);

   if let Err(e) = state.db.delete_registration(&install_id, &app_id).await {
      error!("Failed to delete registration: {e}");
   }

   Ok(Json(serde_json::json!({
       "success": true,
       "message": "Unregistered"
   })))
}

async fn distributor_register(
   State(state): State<AppState>,
   headers: HeaderMap,
   Json(req): Json<DistributorRegisterRequest>,
) -> Result<Json<DistributorRegisterResponse>, (StatusCode, String)> {
   let requested_install_id = req.install_id.clone();
   let requested_app_id = req.app_id.clone();
   let result = async {
      let connector_token = ConnectorToken::try_from(req.connector_token).map_err(|_| {
         (
            StatusCode::BAD_REQUEST,
            "invalid connector token".to_string(),
         )
      })?;
      let app_id = AppId::from(req.app_id.as_str());

      if let Some(key) = req.vapid.as_deref()
         && vapid::decode_public_key(key).is_none()
      {
         return Err((StatusCode::BAD_REQUEST, "invalid vapid key".to_string()));
      }

      let secret = auth::bearer_secret(&headers);
      let identity =
         auth::authorize_register(&state.db, &app_id, Some(&req.install_id), secret.as_ref())
            .await?;

      let mut random = [0_u8; 20];
      rand::rngs::OsRng.fill_bytes(&mut random);
      let candidate = HEXLOWER.encode(&random);
      let endpoint_token = state
         .db
         .register_unified_push(
            &identity.install_id,
            &app_id,
            &connector_token,
            &candidate,
            req.vapid.as_deref(),
         )
         .await
         .map_err(database_error)?
         .ok_or_else(|| {
            (
               StatusCode::CONFLICT,
               "connector token belongs to another app".to_string(),
            )
         })?;

      info!(
         "Registered UnifiedPush endpoint for {}/{}",
         identity.install_id, app_id
      );
      Ok(Json(DistributorRegisterResponse { endpoint_token }))
   }
   .await;

   if let Err((status, reason)) = &result {
      warn!(
         "UnifiedPush register rejected for {requested_install_id}/{requested_app_id} with status \
          {} because {reason}",
         status.as_u16()
      );
   }
   result
}

async fn distributor_reconcile(
   State(state): State<AppState>,
   headers: HeaderMap,
   Json(req): Json<DistributorReconcileRequest>,
) -> Result<Json<DistributorReconcileResponse>, (StatusCode, String)> {
   let requested_install_id = req.install_id.clone();
   let requested_app_ids = req
      .registrations
      .iter()
      .map(|entry| entry.app_id.clone())
      .collect::<Vec<_>>();
   let result = async {
      let DistributorReconcileRequest {
         install_id,
         registrations,
      } = req;
      if registrations.len() > 256 {
         return Err((
            StatusCode::BAD_REQUEST,
            "too many registrations".to_string(),
         ));
      }
      let mut validated = Vec::with_capacity(registrations.len());
      for entry in registrations {
         let app_id = AppId::from(entry.app_id.as_str());
         let connector_token = ConnectorToken::try_from(entry.connector_token).map_err(|_| {
            (
               StatusCode::BAD_REQUEST,
               "invalid connector token".to_string(),
            )
         })?;
         if let Some(key) = entry.vapid.as_deref()
            && vapid::decode_public_key(key).is_none()
         {
            return Err((StatusCode::BAD_REQUEST, "invalid vapid key".to_string()));
         }
         validated.push((app_id, connector_token, entry.vapid));
      }
      let secret = auth::bearer_secret(&headers);
      let empty_app_id = AppId::from("");
      let identity =
         auth::authorize_register(&state.db, &empty_app_id, Some(&install_id), secret.as_ref())
            .await?;
      let retained_tokens = validated
         .iter()
         .map(|(_, connector_token, _)| connector_token.clone())
         .collect::<Vec<_>>();
      let mut endpoints = Vec::with_capacity(validated.len());
      for (app_id, connector_token, vapid) in validated {
         let mut random = [0_u8; 20];
         rand::rngs::OsRng.fill_bytes(&mut random);
         let candidate = HEXLOWER.encode(&random);
         if let Some(endpoint_token) = state
            .db
            .register_unified_push(
               &identity.install_id,
               &app_id,
               &connector_token,
               &candidate,
               vapid.as_deref(),
            )
            .await
            .map_err(database_error)?
         {
            info!(
               "Reconciled UnifiedPush endpoint for {}/{}",
               identity.install_id, app_id
            );
            endpoints.push(DistributorReconcileResult {
               connector_token: connector_token.as_ref().to_owned(),
               endpoint_token,
            });
         } else {
            warn!(
               "Skipped UnifiedPush reconcile entry for {}/{} because the connector token belongs \
                to another app",
               identity.install_id, app_id
            );
         }
      }
      let deleted = state
         .db
         .delete_stale_unified_push_registrations(&identity.install_id, &retained_tokens)
         .await
         .map_err(database_error)?;
      if deleted > 0 {
         info!(
            "Removed {deleted} stale UnifiedPush registrations for {}",
            identity.install_id
         );
      }
      info!(
         "Completed UnifiedPush reconcile for {} with {} registrations",
         identity.install_id,
         endpoints.len()
      );
      Ok(Json(DistributorReconcileResponse { endpoints }))
   }
   .await;

   if let Err((status, reason)) = &result {
      warn!(
         "UnifiedPush reconcile rejected for {requested_install_id} and apps \
          {requested_app_ids:?} with status {} because {reason}",
         status.as_u16()
      );
   }
   result
}

async fn distributor_unregister(
   State(state): State<AppState>,
   headers: HeaderMap,
   Json(req): Json<DistributorUnregisterRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
   let requested_install_id = req.install_id.clone();
   let requested_app_id = req.app_id.clone();
   let result = async {
      let app_id = AppId::from(req.app_id.as_str());
      let secret = auth::bearer_secret(&headers);
      let install_id =
         auth::authorize_unregister(&state.db, &app_id, Some(&req.install_id), secret.as_ref())
            .await?;
      let Ok(connector_token) = ConnectorToken::try_from(req.connector_token) else {
         warn!(
            "Ignored UnifiedPush unregister for {install_id}/{app_id} because the connector token \
             was invalid"
         );
         return Ok(StatusCode::NO_CONTENT);
      };
      state
         .db
         .unregister_unified_push(&install_id, &app_id, &connector_token)
         .await
         .map_err(database_error)?;
      info!("Unregistered UnifiedPush endpoint for {install_id}/{app_id}");
      Ok(StatusCode::NO_CONTENT)
   }
   .await;

   if let Err((status, reason)) = &result {
      warn!(
         "UnifiedPush unregister rejected for {requested_install_id}/{requested_app_id} with \
          status {} because {reason}",
         status.as_u16()
      );
   }
   result
}

async fn unified_push_message(
   State(state): State<AppState>,
   Path(endpoint_token): Path<String>,
   headers: HeaderMap,
   body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
   if body.is_empty() || body.len() > 4096 {
      warn!(
         "UnifiedPush message rejected with status {} because the body was outside the allowed \
          size",
         StatusCode::PAYLOAD_TOO_LARGE.as_u16()
      );
      return Err((
         StatusCode::PAYLOAD_TOO_LARGE,
         "UnifiedPush messages must contain 1 to 4096 bytes".to_string(),
      ));
   }
   let registration = match state.db.get_unified_push_endpoint(&endpoint_token).await {
      Ok(Some(registration)) => registration,
      Ok(None) => {
         warn!(
            "UnifiedPush message rejected with status {} because the endpoint was not found",
            StatusCode::NOT_FOUND.as_u16()
         );
         return Err((StatusCode::NOT_FOUND, "endpoint not found".to_string()));
      },
      Err(error) => {
         let rejection = database_error(error);
         warn!(
            "UnifiedPush message rejected with status {} because {}",
            rejection.0.as_u16(),
            rejection.1
         );
         return Err(rejection);
      },
   };
   // Keyless registrations are native UnifiedPush clients, which send neither
   // header; only web push subscriptions carry a key and are held to RFC 8291.
   if let Some(stored_key) = registration.vapid_pubkey.as_deref()
      && let Some(origin) = state.enforcing_origin()
   {
      let encoding = headers
         .get("content-encoding")
         .and_then(|value| value.to_str().ok());
      if !encoding.is_some_and(|value| value.eq_ignore_ascii_case("aes128gcm")) {
         state.vapid_counters.record_reason(REASON_BAD_ENCODING);
         warn!(
            "UnifiedPush message rejected for {}/{} with status {} because {REASON_BAD_ENCODING}",
            registration.install_id,
            registration.app_id,
            StatusCode::UNSUPPORTED_MEDIA_TYPE.as_u16()
         );
         return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "web push requires Content-Encoding: aes128gcm".to_string(),
         ));
      }
      let now = SystemTime::now()
         .duration_since(UNIX_EPOCH)
         .unwrap_or_default()
         .as_secs();
      let authorization = headers
         .get("authorization")
         .and_then(|value| value.to_str().ok());
      if let Err(rejection) = vapid::validate(authorization, stored_key, origin, now) {
         state.vapid_counters.record_reason(rejection.as_str());
         warn!(
            "UnifiedPush message rejected for {}/{} with status {} because {}",
            registration.install_id,
            registration.app_id,
            StatusCode::FORBIDDEN.as_u16(),
            rejection.as_str()
         );
         return Err((StatusCode::FORBIDDEN, rejection.as_str().to_string()));
      }
      state.vapid_counters.record_accepted();
   }
   if let Err(error) = state
      .delivery
      .enqueue_unified_push(
         &registration.install_id,
         &registration.app_id,
         &registration.connector_token,
         &body,
      )
      .await
   {
      let rejection = database_error(error);
      warn!(
         "UnifiedPush message rejected for {}/{} with status {} because {}",
         registration.install_id,
         registration.app_id,
         rejection.0.as_u16(),
         rejection.1
      );
      return Err(rejection);
   }
   info!(
      "Accepted UnifiedPush message for {}/{}",
      registration.install_id, registration.app_id
   );
   Ok(StatusCode::CREATED)
}

fn database_error(e: anyhow::Error) -> (StatusCode, String) {
   error!("Database error: {e}");
   (
      StatusCode::INTERNAL_SERVER_ERROR,
      "database error".to_string(),
   )
}

async fn restore_registrations(state: AppState) -> anyhow::Result<()> {
   let registrations = state.db.list_registrations().await?;

   info!("Restoring {} registrations", registrations.len());

   for reg in registrations {
      let db = state.db.clone();
      let label = format!("{}/{}", reg.install_id, reg.app_id);

      let result = state
         .fcm_manager
         .write()
         .await
         .start_listener(
            reg.install_id,
            reg.app_id,
            reg.firebase_app_id,
            reg.firebase_project_id,
            reg.firebase_api_key,
            reg.cert_sha1,
            reg.app_version,
            reg.app_version_name,
            reg.target_sdk,
            delivery::DeliveryTarget::from_parts(reg.transport, &reg.endpoint)?,
            db,
            state.delivery.clone(),
         )
         .await;

      if let Err(e) = result {
         error!("Failed to restore FCM listener for {label}: {e}");
      }
   }

   Ok(())
}

#[cfg(test)]
mod tests {
   use std::path::{
      Path,
      PathBuf,
   };

   use super::*;
   use crate::types::InstallId;

   const ORIGIN: &str = "https://push.benzeneos.org";
   const TEST_KEY: &str =
      "BEl62iUYgUivxIkv69yViEuiBIa-Ib9-SkTtWJIapNQFmFXBnBdrgrGVDT6IJ8kNJ8LdaKqp9wLB6pRs5eDvUKk";

   fn existing_path() -> PathBuf {
      Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
   }

   #[test]
   fn enforcement_requires_an_origin_and_no_kill_file() {
      assert_eq!(enforcing_origin(Some(ORIGIN), None), Some(ORIGIN));
      assert_eq!(
         enforcing_origin(Some(ORIGIN), Some(Path::new("/nonexistent/vapid-kill"))),
         Some(ORIGIN)
      );
      assert_eq!(enforcing_origin(None, None), None);
   }

   #[test]
   fn kill_file_suspends_enforcement() {
      let kill_file = existing_path();
      assert!(kill_file.exists());
      assert_eq!(enforcing_origin(Some(ORIGIN), Some(&kill_file)), None);
   }

   async fn state_with_registration(vapid_pubkey: Option<&str>) -> (AppState, String) {
      let db = Arc::new(db::Database::new(":memory:").await.unwrap());
      let install_id = InstallId::try_from("0123456789abcdef").unwrap();
      let app_id = AppId::from("com.example.app");
      let connector_token = ConnectorToken::try_from("connector".to_string()).unwrap();
      let endpoint_token = db
         .register_unified_push(
            &install_id,
            &app_id,
            &connector_token,
            "endpointtoken",
            vapid_pubkey,
         )
         .await
         .unwrap()
         .unwrap();
      let socket_hub = Arc::new(socket::SocketHub::new());
      let state = AppState {
         db: db.clone(),
         fcm_manager: Arc::new(RwLock::new(fcm::FcmManager::new())),
         delivery: Arc::new(delivery::DeliveryManager::new(db, socket_hub.clone())),
         socket_hub,
         endpoint_allowlist: Arc::new(Vec::new()),
         vapid_origin: Some(Arc::from(ORIGIN)),
         vapid_kill_file: None,
         vapid_counters: Arc::new(VapidCounters::default()),
      };
      (state, endpoint_token)
   }

   async fn post(
      state: AppState,
      endpoint_token: String,
      headers: HeaderMap,
   ) -> Result<StatusCode, (StatusCode, String)> {
      unified_push_message(
         State(state),
         Path(endpoint_token),
         headers,
         Bytes::from_static(b"ciphertext"),
      )
      .await
   }

   /// Native UnifiedPush clients send no headers at all; enforcement must not
   /// reach them.
   #[tokio::test]
   async fn keyless_registration_accepts_a_bare_body() {
      let (state, endpoint_token) = state_with_registration(None).await;
      let result = post(state.clone(), endpoint_token, HeaderMap::new()).await;
      assert_eq!(result, Ok(StatusCode::CREATED));
      let (accepted, rejected) = state.vapid_counters.snapshot();
      assert_eq!((accepted, rejected.len()), (0, 0));
   }

   #[tokio::test]
   async fn keyed_registration_requires_aes128gcm() {
      let (state, endpoint_token) = state_with_registration(Some(TEST_KEY)).await;
      let mut headers = HeaderMap::new();
      headers.insert("authorization", "vapid t=x, k=y".parse().unwrap());
      let result = post(state.clone(), endpoint_token.clone(), headers.clone()).await;
      assert_eq!(
         result.map_err(|(status, _)| status),
         Err(StatusCode::UNSUPPORTED_MEDIA_TYPE)
      );

      headers.insert("content-encoding", "aesgcm".parse().unwrap());
      let result = post(state.clone(), endpoint_token, headers).await;
      assert_eq!(
         result.map_err(|(status, _)| status),
         Err(StatusCode::UNSUPPORTED_MEDIA_TYPE)
      );
      let (_, rejected) = state.vapid_counters.snapshot();
      assert_eq!(rejected.get(REASON_BAD_ENCODING), Some(&2));
   }

   #[tokio::test]
   async fn keyed_registration_rejects_an_unsigned_body() {
      let (state, endpoint_token) = state_with_registration(Some(TEST_KEY)).await;
      let mut headers = HeaderMap::new();
      headers.insert("content-encoding", "aes128gcm".parse().unwrap());
      let result = post(state.clone(), endpoint_token, headers).await;
      assert_eq!(
         result.map_err(|(status, _)| status),
         Err(StatusCode::FORBIDDEN)
      );
      let (accepted, rejected) = state.vapid_counters.snapshot();
      assert_eq!(accepted, 0);
      assert_eq!(rejected.get("missing_header"), Some(&1));
   }

   /// `vapid_accepted` is the signal an operator watches to confirm web push
   /// recovered, so a silent zero would read as "still broken".
   #[tokio::test]
   async fn keyed_registration_accepts_a_signed_body() {
      let exp = SystemTime::now()
         .duration_since(UNIX_EPOCH)
         .unwrap()
         .as_secs()
         + 3600;
      let (authorization, key) = vapid::test_support::signed_header(ORIGIN, exp);
      let (state, endpoint_token) = state_with_registration(Some(&key)).await;
      let mut headers = HeaderMap::new();
      headers.insert("content-encoding", "aes128gcm".parse().unwrap());
      headers.insert("authorization", authorization.parse().unwrap());
      let result = post(state.clone(), endpoint_token, headers).await;
      assert_eq!(result, Ok(StatusCode::CREATED));
      let (accepted, rejected) = state.vapid_counters.snapshot();
      assert_eq!((accepted, rejected.len()), (1, 0));
   }

   /// The kill file must suspend the whole keyed path, including the encoding
   /// requirement, so an operator can restore delivery without a redeploy.
   #[tokio::test]
   async fn kill_file_restores_delivery_for_keyed_registrations() {
      let (mut state, endpoint_token) = state_with_registration(Some(TEST_KEY)).await;
      state.vapid_kill_file = Some(Arc::from(existing_path().as_path()));
      let result = post(state, endpoint_token, HeaderMap::new()).await;
      assert_eq!(result, Ok(StatusCode::CREATED));
   }
}
