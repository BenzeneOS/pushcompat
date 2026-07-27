//! `PushCompat` FCM listener
//!
//! This crate allows a server to register with Firebase Cloud Messaging
//! and receive push messages as if it were an Android device.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use pushcompat_listener::{
//!    AppRegistration,
//!    DeviceSession,
//!    FcmCredentials,
//! };
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!    let http = pushcompat_listener::http_client_builder().build()?;
//!    let creds = FcmCredentials {
//!       sender_id:        "123456789".into(),
//!       api_key:          "AIza...".into(),
//!       app_id:           "1:123456789:android:abc123".into(),
//!       project_id:       "my-project".into(),
//!       package_name:     "com.example.app".into(),
//!       cert_sha1:        None,
//!       app_version:      None,
//!       app_version_name: None,
//!       target_sdk:       None,
//!    };
//!
//!    let device = DeviceSession::fresh(&http).await?;
//!    let registration = AppRegistration::register(&http, &device, creds).await?;
//!    println!("FCM Token: {}", registration.fcm_token());
//!
//!    let mut stream = device.connect(vec![]).await?;
//!    // Use tokio_stream::StreamExt to receive messages
//!
//!    Ok(())
//! }
//! ```

#[path = "proto/android_checkin.rs"] mod android_checkin;
#[path = "proto/checkin.rs"] mod checkin;
#[path = "proto/mcs.rs"] mod mcs;

mod error;
mod gcm;
mod push;
mod session;

pub use error::Error;
pub use gcm::DeviceSessionState;
pub use push::{
   DataMessage,
   LoginResponseInfo,
   Message,
   MessageStream,
   MessageTag,
   decode_login_response,
   new_heartbeat_ack,
   new_stream_ack,
};
use serde::{
   Deserialize,
   Serialize,
};
pub use session::{
   AppRegistration,
   AppRegistrationState,
   DeviceSession,
};

pub fn install_crypto_provider() {
   let _ = rustls::crypto::ring::default_provider().install_default();
}

pub fn http_client_builder() -> reqwest::ClientBuilder {
   install_crypto_provider();
   reqwest::Client::builder().use_preconfigured_tls(rustls_client_config())
}

fn rustls_client_config() -> rustls::ClientConfig {
   let root_store = rustls::RootCertStore {
      roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
   };
   rustls::ClientConfig::builder()
      .with_root_certificates(root_store)
      .with_no_client_auth()
}

/// Firebase/FCM credentials extracted from an Android app
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FcmCredentials {
   /// Firebase sender ID (project number), e.g., "890224420307"
   pub sender_id:        String,
   /// Firebase API key
   pub api_key:          String,
   /// Firebase app ID, e.g., "1:890224420307:android:835ea94c9a536bb0"
   pub app_id:           String,
   /// Firebase project ID, e.g., "github-mobile-cc45e"
   pub project_id:       String,
   /// Android package name, e.g., "com.github.android"
   pub package_name:     String,
   /// SHA1 of the app's signing certificate (lowercase hex, no colons),
   /// optional
   #[serde(default)]
   pub cert_sha1:        Option<String>,
   /// App version code (versionCode from APK), optional
   #[serde(default)]
   pub app_version:      Option<i32>,
   /// App version name (versionName from APK), sent as X-app_ver_name, optional
   #[serde(default)]
   pub app_version_name: Option<String>,
   /// Target SDK version from APK, optional
   #[serde(default)]
   pub target_sdk:       Option<i32>,
}
