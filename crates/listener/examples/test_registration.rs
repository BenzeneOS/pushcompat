//! Test the full FCM registration flow with Firebase Installations

use pushcompat_listener::{
   AppRegistration,
   DeviceSession,
   FcmCredentials,
};
fn env(key: &str) -> Result<String, String> {
   std::env::var(key).map_err(|_| format!("missing env var {key}"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
   let log_level = std::env::var("RUST_LOG")
      .ok()
      .and_then(|value| value.parse().ok())
      .unwrap_or(tracing::level_filters::LevelFilter::INFO);
   tracing_subscriber::fmt().with_max_level(log_level).init();

   let http = pushcompat_listener::http_client_builder()
        .http1_only()
        .local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
        .pool_max_idle_per_host(0) // Disable connection reuse - force new connections like real Android
        .build()?;

   let creds = FcmCredentials {
      sender_id:        env("PUSHCOMPAT_SENDER_ID")?,
      api_key:          env("PUSHCOMPAT_API_KEY")?,
      app_id:           env("PUSHCOMPAT_APP_ID")?,
      project_id:       env("PUSHCOMPAT_PROJECT_ID")?,
      package_name:     env("PUSHCOMPAT_PACKAGE")?,
      cert_sha1:        std::env::var("PUSHCOMPAT_CERT_SHA1").ok(),
      app_version:      std::env::var("PUSHCOMPAT_APP_VERSION")
         .ok()
         .and_then(|v| v.parse().ok()),
      app_version_name: std::env::var("PUSHCOMPAT_APP_VERSION_NAME").ok(),
      target_sdk:       std::env::var("PUSHCOMPAT_TARGET_SDK")
         .ok()
         .and_then(|v| v.parse().ok()),
   };

   println!("=== Testing FCM Registration ===");
   println!("Package: {}", creds.package_name);
   println!("Sender ID: {}", creds.sender_id);
   println!();

   let device = DeviceSession::fresh(&http).await?;
   match AppRegistration::register(&http, &device, creds).await {
      Ok(registration) => {
         println!("✅ SUCCESS!");
         println!();
         println!("android_id: {}", device.android_id());
         let token = registration.fcm_token();
         println!("FCM Token: {}...", &token[..24.min(token.len())]);
         println!("Token length: {}", token.len());
      },
      Err(e) => {
         println!("❌ FAILED: {e:?}");
         return Err(e.into());
      },
   }

   Ok(())
}
