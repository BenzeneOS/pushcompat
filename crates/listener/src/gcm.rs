mod contract {
   pub use crate::{
      android_checkin::*,
      checkin::*,
   };
}

use std::io::{
   Read as _,
   Write as _,
};

use aes_gcm::{
   Aes128Gcm,
   Nonce,
   aead::{
      Aead as _,
      KeyInit as _,
   },
};
use data_encoding::{
   BASE64,
   BASE64URL_NOPAD,
};
use flate2::{
   Compression,
   read::GzDecoder,
   write::GzEncoder,
};
use hkdf::Hkdf;
use p256::{
   PublicKey,
   SecretKey,
   elliptic_curve::sec1::ToSec1Point as _,
};
use quick_protobuf::{
   BytesReader,
   MessageRead as _,
   MessageWrite as _,
   Writer,
};
use rand::RngCore as _;
use serde::{
   Deserialize,
   Serialize,
};
use serde_json::Value;
use sha2::Sha256;
use tokio_rustls::rustls::pki_types::ServerName;

use crate::Error;

fn require_some<T>(value: Option<T>, reason: &'static str) -> Result<T, Error> {
   match value {
      Some(value) => Ok(value),
      None => Err(Error::DependencyFailure("Android device check-in", reason)),
   }
}

fn decrypt_web_push(
   private_key_bytes: &[u8],
   public_key_bytes: &[u8],
   auth_secret: &[u8],
   encrypted: &[u8],
) -> Result<Vec<u8>, Error> {
   const API_NAME: &str = "FCM decryption";
   const HEADER_PREFIX_LEN: usize = 21;
   const PUBLIC_KEY_LEN: usize = 65;
   const AUTH_TAG_LEN: usize = 16;

   if private_key_bytes.len() != 32
      || public_key_bytes.len() != PUBLIC_KEY_LEN
      || auth_secret.len() != 16
   {
      return Err(Error::DependencyFailure(
         API_NAME,
         "session contains invalid key material",
      ));
   }

   if encrypted.len() < HEADER_PREFIX_LEN {
      return Err(Error::DependencyFailure(
         API_NAME,
         "encrypted payload header is truncated",
      ));
   }

   let salt = encrypted.get(..16).ok_or(Error::DependencyFailure(
      API_NAME,
      "encrypted payload salt is truncated",
   ))?;
   let record_size_bytes = encrypted
      .get(16..20)
      .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
      .ok_or(Error::DependencyFailure(
         API_NAME,
         "encrypted payload record size is truncated",
      ))?;
   let record_size = u32::from_be_bytes(record_size_bytes) as usize;
   let key_id_len = usize::from(*encrypted.get(20).ok_or(Error::DependencyFailure(
      API_NAME,
      "encrypted payload key length is truncated",
   ))?);

   if record_size < 18 || key_id_len != PUBLIC_KEY_LEN {
      return Err(Error::DependencyFailure(
         API_NAME,
         "encrypted payload header is invalid",
      ));
   }

   let header_len = HEADER_PREFIX_LEN
      .checked_add(key_id_len)
      .ok_or(Error::DependencyFailure(
         API_NAME,
         "encrypted payload header size overflowed",
      ))?;
   let sender_public_key_bytes =
      encrypted
         .get(HEADER_PREFIX_LEN..header_len)
         .ok_or(Error::DependencyFailure(
            API_NAME,
            "encrypted payload key is truncated",
         ))?;
   let Some(ciphertext) = encrypted.get(header_len..) else {
      return Err(Error::DependencyFailure(
         API_NAME,
         "encrypted payload key is truncated",
      ));
   };
   if ciphertext.len() <= AUTH_TAG_LEN || ciphertext.len() >= record_size {
      return Err(Error::DependencyFailure(
         API_NAME,
         "encrypted payload record size is invalid",
      ));
   }

   let sender_public_key = PublicKey::from_sec1_bytes(sender_public_key_bytes)
      .map_err(|_| Error::DependencyFailure(API_NAME, "sender public key is invalid"))?;
   let private_key = SecretKey::from_slice(private_key_bytes)
      .map_err(|_| Error::DependencyFailure(API_NAME, "private key is invalid"))?;
   let receiver_public_key = private_key.public_key().to_sec1_point(false);
   if receiver_public_key.as_bytes() != public_key_bytes {
      return Err(Error::DependencyFailure(
         API_NAME,
         "session public and private keys do not match",
      ));
   }

   let shared_secret = p256::ecdh::diffie_hellman(
      private_key.to_nonzero_scalar(),
      sender_public_key.as_affine(),
   );
   let mut key_info = Vec::with_capacity(14 + 1 + PUBLIC_KEY_LEN * 2);
   key_info.extend_from_slice(b"WebPush: info\0");
   key_info.extend_from_slice(public_key_bytes);
   key_info.extend_from_slice(sender_public_key_bytes);

   let mut input_key_material = [0; 32];
   Hkdf::<Sha256>::new(Some(auth_secret), shared_secret.raw_secret_bytes())
      .expand(&key_info, &mut input_key_material)
      .map_err(|_| Error::DependencyFailure(API_NAME, "key derivation failed"))?;

   let content_hkdf = Hkdf::<Sha256>::new(Some(salt), &input_key_material);
   let mut content_key = [0; 16];
   content_hkdf
      .expand(b"Content-Encoding: aes128gcm\0", &mut content_key)
      .map_err(|_| Error::DependencyFailure(API_NAME, "content key derivation failed"))?;
   let mut nonce = [0; 12];
   content_hkdf
      .expand(b"Content-Encoding: nonce\0", &mut nonce)
      .map_err(|_| Error::DependencyFailure(API_NAME, "nonce derivation failed"))?;

   let cipher = Aes128Gcm::new_from_slice(&content_key)
      .map_err(|_| Error::DependencyFailure(API_NAME, "content key is invalid"))?;
   let nonce = Nonce::try_from(nonce.as_slice())
      .map_err(|_| Error::DependencyFailure(API_NAME, "nonce is invalid"))?;
   let mut plaintext = cipher
      .decrypt(&nonce, ciphertext)
      .map_err(|_| Error::DependencyFailure(API_NAME, "payload authentication failed"))?;

   let Some(delimiter_index) = plaintext.iter().rposition(|byte| *byte != 0) else {
      return Err(Error::DependencyFailure(
         API_NAME,
         "payload delimiter is missing",
      ));
   };
   if plaintext.get(delimiter_index).copied() != Some(2) {
      return Err(Error::DependencyFailure(
         API_NAME,
         "payload delimiter is invalid",
      ));
   }
   plaintext.truncate(delimiter_index);

   Ok(plaintext)
}

const CHECKIN_URL: &str = "https://android.clients.google.com/checkin";
// microG uses android.clients.google.com
const REGISTER_URL: &str = "https://android.clients.google.com/c2dm/register3";
const MAX_HTTP_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_DECOMPRESSED_READ_BYTES: u64 = 1_048_577;
const MAX_PERSISTENT_IDS: usize = 500;
const MAX_PERSISTENT_ID_BYTES: usize = 4096;

async fn read_response_bytes(
   mut response: reqwest::Response,
   api: &'static str,
) -> Result<Vec<u8>, Error> {
   let mut body = Vec::new();
   while let Some(chunk) = response
      .chunk()
      .await
      .map_err(|error| Error::Response(api, error))?
   {
      let body_len = body
         .len()
         .checked_add(chunk.len())
         .ok_or(Error::DependencyFailure(
            api,
            "response body size overflowed",
         ))?;
      if body_len > MAX_HTTP_RESPONSE_BYTES {
         return Err(Error::DependencyFailure(api, "response body is too large"));
      }
      body
         .try_reserve(chunk.len())
         .map_err(|_| Error::DependencyFailure(api, "response body allocation failed"))?;
      body.extend_from_slice(&chunk);
   }
   Ok(body)
}

async fn read_response_text(
   response: reqwest::Response,
   api: &'static str,
) -> Result<String, Error> {
   String::from_utf8(read_response_bytes(response, api).await?)
      .map_err(|_| Error::DependencyFailure(api, "response body is not UTF-8"))
}

// Normal JSON serialization will lose precision and change the number, so we
// must force the i64/u64 to serialize to string.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceSessionState {
   #[serde(with = "decimal_string")]
   pub android_id: i64,

   #[serde(with = "decimal_string")]
   pub security_token: u64,

   /// EC P-256 private key for decryption (base64 URL-safe, 32 bytes)
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub private_key: Option<String>,

   /// EC P-256 public key for registration (base64 URL-safe, 65 bytes
   /// uncompressed)
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub public_key: Option<String>,

   /// Auth secret for decryption (base64 URL-safe, 16 bytes)
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub auth_secret: Option<String>,
}

mod decimal_string {
   use std::{
      fmt::Display,
      str::FromStr,
   };

   use serde::{
      Deserialize as _,
      Deserializer,
      Serializer,
      de::Error as _,
   };

   pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
   where
      T: Display,
      S: Serializer,
   {
      serializer.collect_str(value)
   }

   pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
   where
      T: FromStr,
      T::Err: Display,
      D: Deserializer<'de>,
   {
      String::deserialize(deserializer)?
         .parse()
         .map_err(D::Error::custom)
   }
}

impl DeviceSessionState {
   /// Decrypt an encrypted FCM message payload
   pub(crate) fn decrypt(&self, encrypted_base64: &str) -> Result<Vec<u8>, Error> {
      let private_key_b64 = self
         .private_key
         .as_ref()
         .ok_or_else(|| Error::DependencyFailure("FCM decryption", "no private key in session"))?;
      let public_key_b64 = self
         .public_key
         .as_ref()
         .ok_or_else(|| Error::DependencyFailure("FCM decryption", "no public key in session"))?;
      let auth_secret_b64 = self
         .auth_secret
         .as_ref()
         .ok_or_else(|| Error::DependencyFailure("FCM decryption", "no auth secret in session"))?;

      // Decode the encrypted payload (standard base64, may have / and +)
      let encrypted = BASE64
         .decode(encrypted_base64.as_bytes())
         .map_err(|_| Error::DependencyFailure("FCM decryption", "invalid base64 payload"))?;

      // Decode private key (URL-safe base64)
      let private_key_bytes = BASE64URL_NOPAD
         .decode(private_key_b64.as_bytes())
         .map_err(|_| Error::DependencyFailure("FCM decryption", "invalid private key base64"))?;

      // Decode public key (URL-safe base64)
      let public_key_bytes = BASE64URL_NOPAD
         .decode(public_key_b64.as_bytes())
         .map_err(|_| Error::DependencyFailure("FCM decryption", "invalid public key base64"))?;

      // Decode auth secret (URL-safe base64)
      let auth_secret_bytes = BASE64URL_NOPAD
         .decode(auth_secret_b64.as_bytes())
         .map_err(|_| Error::DependencyFailure("FCM decryption", "invalid auth secret base64"))?;

      decrypt_web_push(
         &private_key_bytes,
         &public_key_bytes,
         &auth_secret_bytes,
         &encrypted,
      )
   }
}

/// Token received from GCM registration
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcmToken {
   pub token: String,
}

/// Firebase Installations credentials
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FirebaseInstallation {
   /// Firebase Installation ID (FID)
   pub fid:           String,
   /// Auth token (JWT) for FCM registration
   pub auth_token:    String,
   /// Refresh token for obtaining new auth tokens
   pub refresh_token: String,
}

/// Firebase app configuration needed for registration
#[derive(Clone, Debug)]
pub struct FirebaseConfig {
   /// Firebase project ID (e.g., "github-mobile-cc45e")
   pub project_id: String,
   /// Firebase API key (from google-services.json)
   pub api_key:    String,
   /// Firebase App ID (e.g., "1:890224420307:android:835ea94c9a536bb0")
   pub app_id:     String,
}

impl DeviceSessionState {
   async fn request(
      http: &reqwest::Client,
      android_id: Option<i64>,
      security_token: Option<u64>,
   ) -> Result<Self, Error> {
      // Current timestamp for event
      let now_ms = std::time::SystemTime::now()
         .duration_since(std::time::UNIX_EPOCH)
         .map_or(0, |d| d.as_millis() as i64);
      let mut logging_id_bytes = [0_u8; 8];
      rand::rngs::OsRng
         .try_fill_bytes(&mut logging_id_bytes)
         .map_err(|_| Error::DependencyFailure("system randomness", "unavailable"))?;
      let logging_id = i64::from_ne_bytes(logging_id_bytes) & i64::MAX;

      // Build event list - microG sends "event_log_start" on first checkin,
      // "system_update" on re-checkin GMS has this structure (axdz class) but
      // may not always populate it
      let event = if android_id.is_none() {
         // First checkin - send "event_log_start"
         vec![contract::AndroidCheckinEvent {
            tag:       Some("event_log_start".into()),
            value:     None,
            time_msec: Some(now_ms),
         }]
      } else {
         // Re-checkin - send "system_update"
         vec![contract::AndroidCheckinEvent {
            tag:       Some("system_update".into()),
            value:     Some("1536,0,-1,NULL".into()),
            time_msec: Some(now_ms),
         }]
      };

      // Use Android device type with proper Android build info
      // This mimics what a real Android device (Pixel 5) would send
      let request = contract::AndroidCheckinRequest {
         version: Some(3),
         id: android_id,
         security_token,
         user_serial_number: Some(0),
         fragment: Some(i32::from(android_id.is_some())),
         locale: Some("en_US".into()),
         time_zone: Some("America/Los_Angeles".into()),
         logging_id: Some(logging_id),
         // microG uses this specific initial digest value
         digest: Some("1-929a0dca0eee55513280171a8585da7dcd3700f8".into()),
         ota_cert: vec!["71Q6Rn2DDZl1zPDVaaeEHItd".into()],
         account_cookie: vec![String::new()],
         serial_number: Some("RF8M33YQXMR".into()),
         mac_addr: vec!["aabbccddeeff".into()],
         mac_addr_type: vec!["wifi".into()],
         checkin: contract::AndroidCheckinProto {
            type_pb: contract::DeviceType::DEVICE_ANDROID_OS,
            build: Some(contract::AndroidBuildProto {
               fingerprint: Some(
                  "google/redfin/redfin:14/AP2A.240805.005/12025142:user/release-keys".into(),
               ),
               hardware: Some("redfin".into()),
               brand: Some("google".into()),
               radio: Some("g7250-00217-231219-B-11446880".into()),
               bootloader: Some("slider-1.2-10323765".into()),
               client_id: Some("android-google".into()),
               time: Some(1722859200), // Aug 2024
               device: Some("redfin".into()),
               sdk_version: Some(34),
               model: Some("Pixel 5".into()),
               manufacturer: Some("Google".into()),
               product: Some("redfin".into()),
               ota_installed: Some(false),
               ..Default::default()
            }),
            last_checkin_msec: Some(0),
            event, // Add the event list (microG CheckinClient.java:108-112)
            roaming: Some("WIFI::".into()),
            user_number: Some(0),
            ..Default::default()
         },
         ..Default::default()
      };

      const API_NAME: &str = "GCM checkin";

      // User-Agent matching microG's CheckinClient.java
      let user_agent = "Android-Checkin/2.0 (redfin AP2A.240805.005); gzip";

      // Gzip compress the request body (both GMS and microG do this)
      let mut proto_bytes = Vec::with_capacity(request.get_size());
      request
         .write_message(&mut Writer::new(&mut proto_bytes))
         .map_err(|_| Error::DependencyFailure(API_NAME, "failed to serialize checkin request"))?;
      let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
      encoder
         .write_all(&proto_bytes)
         .map_err(|_| Error::DependencyFailure(API_NAME, "failed to gzip compress request"))?;
      let compressed_body = encoder
         .finish()
         .map_err(|_| Error::DependencyFailure(API_NAME, "failed to finish gzip compression"))?;

      tracing::debug!(
         "GCM checkin: compressed {} bytes -> {} bytes",
         proto_bytes.len(),
         compressed_body.len()
      );

      let response = http
            .post(CHECKIN_URL)
            .body(compressed_body)
            // Content-Type must be "application/x-protobuffer" (with 'buffer' suffix)
            // Both GMS (awzn.java:92) and microG use this exact value
            .header(reqwest::header::CONTENT_TYPE, "application/x-protobuffer")
            // GMS and microG both send gzip-compressed bodies
            .header(reqwest::header::CONTENT_ENCODING, "gzip")
            .header(reqwest::header::ACCEPT_ENCODING, "gzip")
            .header(reqwest::header::USER_AGENT, user_agent)
            .send()
            .await
            .map_err(|e| Error::Request(API_NAME, e))?;

      // Check if response is gzip-encoded and decompress if needed
      let is_gzip = response
         .headers()
         .get(reqwest::header::CONTENT_ENCODING)
         .is_some_and(|v| v.to_str().unwrap_or("").contains("gzip"));

      let response_bytes = read_response_bytes(response, API_NAME).await?;

      let decoded_bytes = if is_gzip {
         let decoder = GzDecoder::new(response_bytes.as_slice());
         let mut decompressed = Vec::new();
         decoder
            .take(MAX_DECOMPRESSED_READ_BYTES)
            .read_to_end(&mut decompressed)
            .map_err(|_| {
               Error::DependencyFailure(API_NAME, "failed to decompress gzip response")
            })?;
         if decompressed.len() > MAX_HTTP_RESPONSE_BYTES {
            return Err(Error::DependencyFailure(
               API_NAME,
               "decompressed response body is too large",
            ));
         }
         tracing::debug!(
            "GCM checkin: decompressed {} bytes -> {} bytes",
            response_bytes.len(),
            decompressed.len()
         );
         decompressed
      } else {
         response_bytes.to_vec()
      };

      let mut reader = BytesReader::from_bytes(&decoded_bytes);
      let response = contract::AndroidCheckinResponse::from_reader(&mut reader, &decoded_bytes)
         .map_err(|e| Error::ProtobufDecode("android checkin response", e))?;

      let android_id = require_some(response.android_id, "response is missing android id")?;

      const BAD_ID: Result<i64, Error> = Err(Error::DependencyFailure(
         API_NAME,
         "responded with non-numeric android id",
      ));
      let android_id = i64::try_from(android_id).or(BAD_ID)?;
      let security_token = require_some(
         response.security_token,
         "response is missing security token",
      )?;

      Ok(Self {
         android_id,
         security_token,
         private_key: None,
         public_key: None,
         auth_secret: None,
      })
   }

   /// Perform initial GCM checkin to get `android_id` and `security_token`
   pub(crate) async fn checkin(http: &reqwest::Client) -> Result<Self, Error> {
      let mut session = Self::request(http, None, None).await?;
      // Generate encryption keys for this session
      session.generate_keys()?;
      Ok(session)
   }

   /// Refresh the session (re-checkin with existing credentials)
   pub(crate) async fn refresh(&self, http: &reqwest::Client) -> Result<Self, Error> {
      let mut session =
         Self::request(http, Some(self.android_id), Some(self.security_token)).await?;
      // Keep existing keys if we have them, otherwise generate new ones
      if self.private_key.is_some() {
         session.private_key = self.private_key.clone();
         session.public_key = self.public_key.clone();
         session.auth_secret = self.auth_secret.clone();
      } else {
         session.generate_keys()?;
      }
      Ok(session)
   }

   /// Generate EC P-256 key pair and auth secret for push encryption
   fn generate_keys(&mut self) -> Result<(), Error> {
      let mut private_key_bytes = [0; 32];
      let private_key = loop {
         rand::rngs::OsRng
            .try_fill_bytes(&mut private_key_bytes)
            .map_err(|_| Error::DependencyFailure("system randomness", "unavailable"))?;
         if let Ok(private_key) = SecretKey::from_slice(&private_key_bytes) {
            break private_key;
         }
      };
      let public_key = private_key.public_key().to_sec1_point(false);
      let mut auth_secret = [0; 16];
      rand::rngs::OsRng
         .try_fill_bytes(&mut auth_secret)
         .map_err(|_| Error::DependencyFailure("system randomness", "unavailable"))?;

      // Store private key, public key, and auth secret as base64
      self.private_key = Some(BASE64URL_NOPAD.encode(&private_key.to_bytes()));
      self.public_key = Some(BASE64URL_NOPAD.encode(public_key.as_bytes()));
      self.auth_secret = Some(BASE64URL_NOPAD.encode(&auth_secret));

      tracing::debug!("Generated FCM encryption keys");
      Ok(())
   }

   /// Get the public key for registration (base64 URL-safe)
   pub(crate) fn get_public_key(&self) -> Result<String, Error> {
      self
         .public_key
         .clone()
         .ok_or_else(|| Error::DependencyFailure("public key", "no public key in session"))
   }

   /// Register with Firebase Installations to get FID and auth token
   ///
   /// This is required for FCM registration with modern Firebase SDK (>=
   /// 20.1.1)
   pub(crate) async fn register_firebase_installation(
      http: &reqwest::Client,
      firebase_config: &FirebaseConfig,
      package_name: &str,
      cert_sha1: &str,
   ) -> Result<FirebaseInstallation, Error> {
      const API_NAME: &str = "Firebase Installations";

      // Generate a random FID (Firebase Installation ID)
      // FID is a 22-character base64url string starting with 'c' or similar
      // Use OsRng instead of thread_rng() because thread_rng() is not Send
      let fid = {
         let mut rng = rand::rngs::OsRng;
         let mut fid_bytes = [0_u8; 17];
         rng.try_fill_bytes(&mut fid_bytes)
            .map_err(|_| Error::DependencyFailure("system randomness", "unavailable"))?;
         let mut fid = BASE64URL_NOPAD.encode(&fid_bytes);
         fid.truncate(22);
         // FID should start with a valid char (c, d, e, f)
         let first_byte = fid_bytes
            .first()
            .copied()
            .ok_or(Error::DependencyFailure(API_NAME, "failed to generate FID"))?
            & 0x0F;
         let first_char = match first_byte % 4 {
            0 => 'c',
            1 => 'd',
            2 => 'e',
            _ => 'f',
         };
         let suffix = fid
            .get(1..)
            .ok_or(Error::DependencyFailure(API_NAME, "failed to generate FID"))?;
         format!("{first_char}{suffix}")
      };

      let url = format!(
         "https://firebaseinstallations.googleapis.com/v1/projects/{}/installations",
         firebase_config.project_id
      );

      let payload = serde_json::json!({
          "fid": fid,
          "appId": firebase_config.app_id,
          "authVersion": "FIS_v2",
          "sdkVersion": "a:17.0.0",
      });

      tracing::info!("Firebase Installations URL: {url}");
      tracing::debug!("Firebase Installations payload: {payload:?}");

      let response = http
         .post(&url)
         .header("Content-Type", "application/json")
         .header("x-goog-api-key", &firebase_config.api_key)
         .header("x-android-package", package_name)
         .header("x-android-cert", cert_sha1.to_uppercase())
         .json(&payload)
         .send()
         .await
         .map_err(|e| Error::Request(API_NAME, e))?;

      let status = response.status();
      let response_text = read_response_text(response, API_NAME).await?;

      if !status.is_success() {
         tracing::error!("Firebase Installations failed: {status} - {response_text}");
         let response_summary = response_text.chars().take(200).collect::<String>();
         return Err(Error::DependencyRejection(
            API_NAME,
            format!("HTTP {status}: {response_summary}"),
         ));
      }

      let response_json = serde_json::from_str::<Value>(&response_text)
         .map_err(|_| Error::DependencyFailure(API_NAME, "invalid JSON response"))?;

      let fid = response_json
         .get("fid")
         .and_then(Value::as_str)
         .ok_or(Error::DependencyFailure(
            API_NAME,
            "missing fid in response",
         ))?
         .to_owned();

      let auth_token = response_json
         .get("authToken")
         .and_then(|auth| auth.get("token"))
         .and_then(Value::as_str)
         .ok_or(Error::DependencyFailure(
            API_NAME,
            "missing authToken in response",
         ))?
         .to_owned();

      let refresh_token = response_json
         .get("refreshToken")
         .and_then(Value::as_str)
         .ok_or(Error::DependencyFailure(
            API_NAME,
            "missing refreshToken in response",
         ))?
         .to_owned();

      tracing::info!("Firebase Installations succeeded, FID: {fid}");

      Ok(FirebaseInstallation {
         fid,
         auth_token,
         refresh_token,
      })
   }

   /// Register with GCM to get a token for receiving messages
   ///
   /// # Arguments
   /// * `http` - HTTP client
   /// * `sender_id` - Firebase sender ID (project number), e.g., "890224420307"
   /// * `package_name` - Android package name, e.g., "com.github.android"
   /// * `cert_sha1` - SHA1 of signing certificate (lowercase hex, no colons),
   ///   or None
   /// * `app_version` - App version code (versionCode from APK)
   /// * `app_version_name` - App version name (versionName from APK), sent as
   ///   X-app_ver_name
   /// * `target_sdk` - Target SDK version from APK
   /// * `firebase_config` - Firebase configuration for Installations API
   /// * `firebase_installation` - Pre-registered Firebase Installation
   pub(crate) async fn register(
      &self,
      http: &reqwest::Client,
      sender_id: &str,
      package_name: &str,
      cert_sha1: Option<&str>,
      app_version: Option<i32>,
      app_version_name: Option<&str>,
      target_sdk: Option<i32>,
      firebase_config: Option<&FirebaseConfig>,
      firebase_installation: Option<&FirebaseInstallation>,
   ) -> Result<GcmToken, Error> {
      let android_id = self.android_id.to_string();
      let auth_header = format!("AidLogin {android_id}:{}", self.security_token);
      let user_agent = "Android-GCM/1.5 (redfin AP2A.240805.005)";

      let app_ver_str = app_version.unwrap_or(1).to_string();
      let target_ver_str = target_sdk.unwrap_or(34).to_string();
      let cert_str = cert_sha1.map(str::to_lowercase).unwrap_or_default();
      let ver_name_str = app_version_name.unwrap_or("1.0.0");

      // Get encryption keys for push encryption
      let public_key = self.get_public_key().unwrap_or_default();
      let auth_secret = self.auth_secret.clone().unwrap_or_default();

      // Build form body with exact ordering matching Java's LinkedHashMap
      let form_body = if let (Some(fis), Some(config)) = (firebase_installation, firebase_config) {
         format!(
            "app={}&device={}&sender={}&cert={}&app_ver={}&target_ver={}&X-appid={}&\
             X-Goog-Firebase-Installations-Auth={}&X-cliv={}&X-scope={}&X-subtype={}&\
             X-gmp_app_id={}&X-Firebase-Client={}&X-app_ver_name={}&encryption_key={}&\
             encryption_auth={}",
            urlencoding::encode(package_name),
            urlencoding::encode(&android_id),
            urlencoding::encode(sender_id),
            urlencoding::encode(&cert_str),
            urlencoding::encode(&app_ver_str),
            urlencoding::encode(&target_ver_str),
            urlencoding::encode(&fis.fid),
            urlencoding::encode(&fis.auth_token),
            urlencoding::encode("fiid-21.0.0"),
            urlencoding::encode("*"),
            urlencoding::encode(sender_id),
            urlencoding::encode(&config.app_id),
            urlencoding::encode("fire-installations/17.0.0"),
            urlencoding::encode(ver_name_str),
            urlencoding::encode(&public_key),
            urlencoding::encode(&auth_secret),
         )
      } else {
         format!(
            "app={}&device={}&sender={}&cert={}&app_ver={}&target_ver={}&encryption_key={}&\
             encryption_auth={}",
            urlencoding::encode(package_name),
            urlencoding::encode(&android_id),
            urlencoding::encode(sender_id),
            urlencoding::encode(&cert_str),
            urlencoding::encode(&app_ver_str),
            urlencoding::encode(&target_ver_str),
            urlencoding::encode(&public_key),
            urlencoding::encode(&auth_secret),
         )
      };

      const API_NAME: &str = "GCM registration";
      // Aggressive retry: 15 attempts × 250ms = ~4s total window
      // PHONE_REGISTRATION_ERROR is typically due to FIS token propagation delay
      const MAX_RETRIES: u32 = 15;
      const RETRY_DELAY_MS: u64 = 250;

      tracing::debug!("GCM register: {}", form_body);

      let mut last_error = None;

      for attempt in 1..=MAX_RETRIES {
         let result = http
            .post(REGISTER_URL)
            .header(
               reqwest::header::CONTENT_TYPE,
               "application/x-www-form-urlencoded",
            )
            .header(reqwest::header::AUTHORIZATION, &auth_header)
            .header(reqwest::header::USER_AGENT, user_agent)
            .header("app", package_name)
            .body(form_body.clone())
            .send()
            .await
            .map_err(|e| Error::Request(API_NAME, e))?;

         let response_text = read_response_text(result, API_NAME).await?;

         // Response format is "token=<token>" or "Error=<reason>"
         if let Some(token) = response_text.strip_prefix("token=") {
            if attempt > 1 {
               tracing::info!("GCM registration succeeded on attempt {}", attempt);
            }
            return Ok(GcmToken {
               token: token.to_owned(),
            });
         }

         if let Some(error) = response_text.strip_prefix("Error=") {
            // Retry on PHONE_REGISTRATION_ERROR (transient timing issue)
            if error == "PHONE_REGISTRATION_ERROR" && attempt < MAX_RETRIES {
               tracing::debug!(
                  "GCM registration attempt {}/{} failed, retrying...",
                  attempt,
                  MAX_RETRIES
               );
               last_error = Some(error.to_owned());
               tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_DELAY_MS)).await;
               continue;
            }
            return Err(Error::DependencyRejection(API_NAME, error.into()));
         }

         tracing::warn!("Unexpected GCM response: {}", response_text);
         return Err(Error::DependencyFailure(API_NAME, "malformed response"));
      }

      Err(Error::DependencyRejection(
         API_NAME,
         last_error.unwrap_or_else(|| "max retries exceeded".into()),
      ))
   }

   /// Connect to mtalk.google.com MCS server
   pub(crate) async fn connect(
      &self,
      received_persistent_id: Vec<String>,
   ) -> Result<Connection, Error> {
      const ERR_RESOLVE: Error =
         Error::DependencyFailure("name resolution", "unable to resolve google talk host name");

      crate::install_crypto_provider();

      if received_persistent_id.len() > MAX_PERSISTENT_IDS
         || received_persistent_id
            .iter()
            .any(|id| id.len() > MAX_PERSISTENT_ID_BYTES)
      {
         return Err(Error::DependencyFailure(
            "MCS login",
            "persistent id history is invalid",
         ));
      }

      let domain = ServerName::try_from("mtalk.google.com").or(Err(ERR_RESOLVE))?;

      let login_request = self.new_mcs_login_request(received_persistent_id);

      let login_capacity =
         login_request
            .get_size()
            .checked_add(6)
            .ok_or(Error::DependencyFailure(
               "MCS login",
               "request size overflowed",
            ))?;
      let mut login_bytes = Vec::new();
      login_bytes
         .try_reserve(login_capacity)
         .map_err(|_| Error::DependencyFailure("MCS login", "request allocation failed"))?;
      let mut writer = Writer::new(&mut login_bytes);
      writer
         .write_u8(Self::MCS_VERSION)
         .and_then(|()| writer.write_u8(Self::LOGIN_REQUEST_TAG))
         .and_then(|()| writer.write_message(&login_request))
         .map_err(|_| Error::DependencyFailure("MCS login", "failed to serialize request"))?;

      Self::try_connect(domain, &login_bytes)
         .await
         .map_err(Error::Socket)
   }

   const MCS_VERSION: u8 = 41;
   const LOGIN_REQUEST_TAG: u8 = 2;

   fn new_mcs_login_request(
      &self,
      received_persistent_id: Vec<String>,
   ) -> crate::mcs::LoginRequest {
      let android_id = self.android_id.to_string();
      crate::mcs::LoginRequest {
         adaptive_heartbeat: Some(false),
         auth_service: Some(crate::mcs::mod_LoginRequest::AuthService::ANDROID_ID),
         auth_token: self.security_token.to_string(),
         id: "chrome-63.0.3234.0".into(),
         domain: "mcs.android.com".into(),
         device_id: Some(format!("android-{:x}", self.android_id)),
         network_type: Some(1),
         resource: android_id.clone(),
         user: android_id,
         use_rmq2: Some(true),
         setting: vec![crate::mcs::Setting {
            name:  "new_vc".into(),
            value: "1".into(),
         }],
         received_persistent_id,
         ..Default::default()
      }
   }

   async fn try_connect(
      domain: ServerName<'static>,
      login_bytes: &[u8],
   ) -> Result<Connection, tokio::io::Error> {
      use tokio::io::{
         AsyncReadExt as _,
         AsyncWriteExt as _,
      };

      let stream = tokio::net::TcpStream::connect("mtalk.google.com:5228").await?;
      let tls = new_tls_initiator();
      let mut stream = tls.connect(domain, stream).await?;

      stream.write_all(login_bytes).await?;

      // Read the version byte from server
      stream.read_i8().await?;

      Ok(Connection(stream))
   }
}

fn new_tls_initiator() -> tokio_rustls::TlsConnector {
   tokio_rustls::TlsConnector::from(std::sync::Arc::new(crate::rustls_client_config()))
}

pub struct Connection(pub tokio_rustls::client::TlsStream<tokio::net::TcpStream>);

impl std::ops::Deref for Connection {
   type Target = tokio_rustls::client::TlsStream<tokio::net::TcpStream>;

   fn deref(&self) -> &Self::Target {
      &self.0
   }
}

impl std::ops::DerefMut for Connection {
   fn deref_mut(&mut self) -> &mut Self::Target {
      &mut self.0
   }
}

#[cfg(test)]
mod tests {
   use super::*;

   const RFC8291_BODY: &str = concat!(
      "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27ml",
      "mlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPT",
      "pK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN",
   );
   const RFC8291_PRIVATE_KEY: &str = "q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94";
   const RFC8291_PUBLIC_KEY: &str = concat!(
      "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcx",
      "aOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
   );
   const RFC8291_AUTH_SECRET: &str = "BTBZMqHH6r4Tts7J_aSIgg";

   fn decode_url(value: &str) -> Vec<u8> {
      BASE64URL_NOPAD.decode(value.as_bytes()).unwrap()
   }

   fn rfc8291_state() -> DeviceSessionState {
      DeviceSessionState {
         android_id:     1,
         security_token: 2,
         private_key:    Some(RFC8291_PRIVATE_KEY.into()),
         public_key:     Some(RFC8291_PUBLIC_KEY.into()),
         auth_secret:    Some(RFC8291_AUTH_SECRET.into()),
      }
   }

   #[test]
   fn decrypts_rfc8291_example() {
      let encoded = BASE64.encode(&decode_url(RFC8291_BODY));
      let plaintext = rfc8291_state().decrypt(&encoded).unwrap();

      assert_eq!(plaintext, b"When I grow up, I want to be a watermelon");
   }
}
