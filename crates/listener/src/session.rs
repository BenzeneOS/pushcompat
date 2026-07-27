use serde::{
   Deserialize,
   Serialize,
};

use crate::{
   Error,
   FcmCredentials,
   MessageStream,
   gcm::{
      DeviceSessionState,
      FirebaseConfig,
   },
};

#[derive(Clone, Debug)]
pub struct DeviceSession {
   state: DeviceSessionState,
}

impl DeviceSession {
   pub async fn fresh(http: &reqwest::Client) -> Result<Self, Error> {
      Ok(Self {
         state: DeviceSessionState::checkin(http).await?,
      })
   }

   #[must_use]
   pub const fn restore(state: DeviceSessionState) -> Self {
      Self { state }
   }

   #[must_use]
   pub fn state(&self) -> DeviceSessionState {
      self.state.clone()
   }

   #[must_use]
   pub fn into_state(self) -> DeviceSessionState {
      self.state
   }

   pub async fn refresh(&mut self, http: &reqwest::Client) -> Result<(), Error> {
      self.state = self.state.refresh(http).await?;
      Ok(())
   }

   pub async fn connect(
      &self,
      persistent_ids: Vec<String>,
   ) -> Result<MessageStream<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>, Error> {
      let connection = self.state.connect(persistent_ids).await?;
      Ok(MessageStream::new(connection.0))
   }

   #[must_use]
   pub const fn android_id(&self) -> i64 {
      self.state.android_id
   }

   pub fn decrypt(&self, encrypted_base64: &str) -> Result<Vec<u8>, Error> {
      self.state.decrypt(encrypted_base64)
   }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRegistrationState {
   pub fcm_token:   String,
   pub credentials: FcmCredentials,
}

#[derive(Clone, Debug)]
pub struct AppRegistration {
   state: AppRegistrationState,
}

impl AppRegistration {
   pub async fn register(
      http: &reqwest::Client,
      device: &DeviceSession,
      credentials: FcmCredentials,
   ) -> Result<Self, Error> {
      tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

      let firebase_config = FirebaseConfig {
         project_id: credentials.project_id.clone(),
         api_key:    credentials.api_key.clone(),
         app_id:     credentials.app_id.clone(),
      };
      let firebase_installation = DeviceSessionState::register_firebase_installation(
         http,
         &firebase_config,
         &credentials.package_name,
         credentials.cert_sha1.as_deref().unwrap_or(""),
      )
      .await?;
      let gcm_token = device
         .state
         .register(
            http,
            &credentials.sender_id,
            &credentials.package_name,
            credentials.cert_sha1.as_deref(),
            credentials.app_version,
            credentials.app_version_name.as_deref(),
            credentials.target_sdk,
            Some(&firebase_config),
            Some(&firebase_installation),
         )
         .await?;

      Ok(Self {
         state: AppRegistrationState {
            fcm_token: gcm_token.token,
            credentials,
         },
      })
   }

   #[must_use]
   pub const fn restore(state: AppRegistrationState) -> Self {
      Self { state }
   }

   #[must_use]
   pub fn state(&self) -> AppRegistrationState {
      self.state.clone()
   }

   #[must_use]
   pub fn into_state(self) -> AppRegistrationState {
      self.state
   }

   #[must_use]
   pub fn fcm_token(&self) -> &str {
      &self.state.fcm_token
   }

   #[must_use]
   pub const fn credentials(&self) -> &FcmCredentials {
      &self.state.credentials
   }
}
