//! Per-install registration authentication.

use axum::http::{
   HeaderMap,
   StatusCode,
};
use data_encoding::HEXLOWER;
use sha2::{
   Digest,
   Sha256,
};
use subtle::ConstantTimeEq;

use crate::{
   db::Database,
   types::{
      AppId,
      InstallId,
      InstallSecret,
   },
};

pub fn hash_secret(secret: &InstallSecret) -> String {
   HEXLOWER.encode(&Sha256::digest(secret.expose().as_bytes()))
}

pub fn verify_secret(secret: &InstallSecret, stored_hash: &str) -> bool {
   let Ok(stored) = HEXLOWER.decode(stored_hash.as_bytes()) else {
      return false;
   };
   bool::from(
      Sha256::digest(secret.expose().as_bytes())
         .as_slice()
         .ct_eq(&stored),
   )
}

const MIN_SECRET_LEN: usize = 32;
const MAX_SECRET_LEN: usize = 128;

pub fn bearer_secret(headers: &HeaderMap) -> Option<InstallSecret> {
   let value = headers
      .get(axum::http::header::AUTHORIZATION)?
      .to_str()
      .ok()?;
   let (scheme, secret) = value.split_once(' ')?;
   if !scheme.eq_ignore_ascii_case("bearer") {
      return None;
   }
   let secret = secret.trim();
   if secret.len() < MIN_SECRET_LEN || secret.len() > MAX_SECRET_LEN {
      return None;
   }
   Some(InstallSecret::from(secret))
}

#[derive(Debug)]
pub struct RegisterIdentity {
   pub install_id:  InstallId,
   pub secret_hash: String,
}

/// Credentials are mandatory.
pub async fn authorize_register(
   db: &Database,
   _app_id: &AppId,
   install_id: Option<&str>,
   secret: Option<&InstallSecret>,
) -> Result<RegisterIdentity, (StatusCode, String)> {
   let (Some(raw_install_id), Some(secret)) = (install_id, secret) else {
      return Err((
         StatusCode::UNAUTHORIZED,
         "install_id and secret required".to_string(),
      ));
   };
   let install_id = InstallId::try_from(raw_install_id)
      .map_err(|_| (StatusCode::BAD_REQUEST, "invalid install_id".to_string()))?;

   let secret_hash = hash_secret(secret);
   if !db
      .claim_installation(&install_id, &secret_hash, secret)
      .await
      .map_err(internal)?
   {
      return Err((StatusCode::UNAUTHORIZED, "invalid credentials".to_string()));
   }

   Ok(RegisterIdentity {
      install_id,
      secret_hash,
   })
}

pub async fn authorize_unregister(
   db: &Database,
   _app_id: &AppId,
   install_id: Option<&str>,
   secret: Option<&InstallSecret>,
) -> Result<InstallId, (StatusCode, String)> {
   let (Some(raw_install_id), Some(secret)) = (install_id, secret) else {
      return Err((
         StatusCode::UNAUTHORIZED,
         "install_id and secret required".to_string(),
      ));
   };
   let Ok(install_id) = InstallId::try_from(raw_install_id) else {
      return Err((StatusCode::UNAUTHORIZED, "invalid credentials".to_string()));
   };
   if db
      .verify_installation(install_id.as_ref(), secret)
      .await
      .map_err(internal)?
   {
      Ok(install_id)
   } else {
      Err((StatusCode::UNAUTHORIZED, "invalid credentials".to_string()))
   }
}

fn internal(e: anyhow::Error) -> (StatusCode, String) {
   tracing::error!("database error: {e}");
   (
      StatusCode::INTERNAL_SERVER_ERROR,
      "database error".to_string(),
   )
}

#[cfg(test)]
mod tests {
   use axum::http::StatusCode;

   use super::*;
   use crate::{
      db::{
         Database,
         Registration,
      },
      types::Transport,
   };

   const INSTALL_A: &str = "0123456789abcdef0123456789abcdef";

   async fn test_db() -> Database {
      Database::new(":memory:").await.unwrap()
   }

   fn reg(install_id: &str, app_id: &str, endpoint: &str, secret_hash: &str) -> Registration {
      Registration {
         install_id:          InstallId::try_from(install_id).unwrap(),
         app_id:              AppId::from(app_id),
         secret_hash:         secret_hash.to_string(),
         endpoint:            endpoint.to_string(),
         fcm_token:           None,
         firebase_app_id:     "1:123:android:abc".to_string(),
         firebase_project_id: "proj".to_string(),
         firebase_api_key:    "key".to_string(),
         cert_sha1:           None,
         app_version:         None,
         app_version_name:    None,
         target_sdk:          None,
         transport:           Transport::UnifiedPush,
      }
   }

   #[tokio::test]
   async fn first_registration_claims_then_secret_is_enforced() {
      let db = test_db().await;
      let app_id = AppId::from("com.app");
      let secret_a = InstallSecret::from("s1");

      let identity = authorize_register(&db, &app_id, Some(INSTALL_A), Some(&secret_a))
         .await
         .unwrap();
      assert_eq!(identity.install_id.as_ref(), INSTALL_A);
      let mut r = reg(INSTALL_A, "com.app", "https://n.example/t", "");
      r.secret_hash = identity.secret_hash;
      db.save_registration(&r).await.unwrap();

      // Same secret: allowed
      assert!(
         authorize_register(&db, &app_id, Some(INSTALL_A), Some(&secret_a))
            .await
            .is_ok()
      );
      // Wrong secret: 401
      let secret_b = InstallSecret::from("s2");
      let err = authorize_register(&db, &app_id, Some(INSTALL_A), Some(&secret_b))
         .await
         .unwrap_err();
      assert_eq!(err.0, StatusCode::UNAUTHORIZED);

      // Malformed install_id is a 400, distinct from a credential failure.
      let err = authorize_register(&db, &app_id, Some("i1"), Some(&secret_a))
         .await
         .unwrap_err();
      assert_eq!(err.0, StatusCode::BAD_REQUEST);
   }
}
