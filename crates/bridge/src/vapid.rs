//! RFC 8292 VAPID validation. Push endpoints must not be bearer capabilities,
//! so this checks the `vapid` Authorization header against the key pinned at
//! registration.

use data_encoding::BASE64URL_NOPAD;
use p256::ecdsa::{
   Signature,
   VerifyingKey,
   signature::Verifier,
};

pub const VAPID_KEY_B64_LEN: usize = 87;
const MAX_EXP_AHEAD_SECS: u64 = 24 * 60 * 60;
const CLOCK_SKEW_SECS: u64 = 300;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum VapidRejection {
   MissingHeader,
   MalformedHeader,
   KeyMismatch,
   BadToken,
   BadSignature,
   Expired,
   TooFarAhead,
   BadAudience,
}

impl VapidRejection {
   pub fn as_str(self) -> &'static str {
      match self {
         Self::MissingHeader => "missing_header",
         Self::MalformedHeader => "malformed_header",
         Self::KeyMismatch => "key_mismatch",
         Self::BadToken => "bad_token",
         Self::BadSignature => "bad_signature",
         Self::Expired => "expired",
         Self::TooFarAhead => "too_far_ahead",
         Self::BadAudience => "bad_audience",
      }
   }
}

pub fn decode_public_key(b64: &str) -> Option<[u8; 65]> {
   if b64.len() != VAPID_KEY_B64_LEN {
      return None;
   }
   let bytes = BASE64URL_NOPAD.decode(b64.as_bytes()).ok()?;
   let arr: [u8; 65] = bytes.try_into().ok()?;
   (arr[0] == 0x04).then_some(arr)
}

// Senders re-encode the key the page handed them; padded or standard-alphabet
// base64 is the same key, so k is normalized to bytes before comparison rather
// than string-matched against the stored form.
fn decode_key_param(b64: &str) -> Option<[u8; 65]> {
   let normalized: String = b64
      .trim_end_matches('=')
      .chars()
      .map(|c| {
         match c {
            '+' => '-',
            '/' => '_',
            other => other,
         }
      })
      .collect();
   let bytes = BASE64URL_NOPAD.decode(normalized.as_bytes()).ok()?;
   let arr: [u8; 65] = bytes.try_into().ok()?;
   (arr[0] == 0x04).then_some(arr)
}

pub fn validate(
   authorization: Option<&str>,
   stored_key_b64: &str,
   public_origin: &str,
   now: u64,
) -> Result<(), VapidRejection> {
   let header = authorization.ok_or(VapidRejection::MissingHeader)?;
   let (scheme, rest) = header
      .split_once(' ')
      .ok_or(VapidRejection::MalformedHeader)?;
   if !scheme.eq_ignore_ascii_case("vapid") {
      return Err(VapidRejection::MalformedHeader);
   }
   let mut token = None;
   let mut key = None;
   for part in rest.split(',') {
      let part = part.trim();
      if let Some(value) = part.strip_prefix("t=") {
         token = Some(value);
      } else if let Some(value) = part.strip_prefix("k=") {
         key = Some(value);
      }
   }
   let (token, key) = token.zip(key).ok_or(VapidRejection::MalformedHeader)?;
   let key_bytes = decode_public_key(stored_key_b64).ok_or_else(|| {
      tracing::warn!("Stored VAPID key failed to decode");
      VapidRejection::KeyMismatch
   })?;
   if decode_key_param(key) != Some(key_bytes) {
      return Err(VapidRejection::KeyMismatch);
   }

   let mut parts = token.splitn(3, '.');
   let (Some(header_b64), Some(payload_b64), Some(signature_b64)) =
      (parts.next(), parts.next(), parts.next())
   else {
      return Err(VapidRejection::BadToken);
   };
   let decode = |part: &str| {
      BASE64URL_NOPAD
         .decode(part.as_bytes())
         .map_err(|_| VapidRejection::BadToken)
   };
   let jwt_header: serde_json::Value =
      serde_json::from_slice(&decode(header_b64)?).map_err(|_| VapidRejection::BadToken)?;
   if jwt_header.get("alg").and_then(|alg| alg.as_str()) != Some("ES256") {
      return Err(VapidRejection::BadToken);
   }
   let claims: serde_json::Value =
      serde_json::from_slice(&decode(payload_b64)?).map_err(|_| VapidRejection::BadToken)?;

   let signature =
      Signature::from_slice(&decode(signature_b64)?).map_err(|_| VapidRejection::BadToken)?;
   let verifying_key =
      VerifyingKey::from_sec1_bytes(&key_bytes).map_err(|_| VapidRejection::KeyMismatch)?;
   verifying_key
      .verify(format!("{header_b64}.{payload_b64}").as_bytes(), &signature)
      .map_err(|_| VapidRejection::BadSignature)?;

   let exp = claims
      .get("exp")
      .and_then(|exp| exp.as_u64())
      .ok_or(VapidRejection::BadToken)?;
   if exp.saturating_add(CLOCK_SKEW_SECS) < now {
      return Err(VapidRejection::Expired);
   }
   if exp
      > now
         .saturating_add(MAX_EXP_AHEAD_SECS)
         .saturating_add(CLOCK_SKEW_SECS)
   {
      return Err(VapidRejection::TooFarAhead);
   }
   let aud = claims
      .get("aud")
      .and_then(|aud| aud.as_str())
      .ok_or(VapidRejection::BadToken)?;
   if aud.trim_end_matches('/') != public_origin.trim_end_matches('/') {
      return Err(VapidRejection::BadAudience);
   }
   // sub is deliberately unvalidated: RFC 8292 provides it as operator contact
   // info, not authentication.
   Ok(())
}

#[cfg(test)]
pub(crate) mod test_support {
   use p256::ecdsa::{
      Signature,
      SigningKey,
      signature::Signer,
   };

   use super::BASE64URL_NOPAD;

   /// A signed `vapid` Authorization header and the base64url public key it
   /// authenticates against, for exercising callers of [`super::validate`].
   pub(crate) fn signed_header(aud: &str, exp: u64) -> (String, String) {
      let signing = SigningKey::from_slice(&[11u8; 32]).unwrap();
      let key = BASE64URL_NOPAD.encode(signing.verifying_key().to_sec1_point(false).as_bytes());
      let header = BASE64URL_NOPAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
      let payload = BASE64URL_NOPAD
         .encode(format!(r#"{{"aud":"{aud}","exp":{exp},"sub":"mailto:t@t"}}"#).as_bytes());
      let signed = format!("{header}.{payload}");
      let signature: Signature = signing.sign(signed.as_bytes());
      let token = format!("{signed}.{}", BASE64URL_NOPAD.encode(&signature.to_bytes()));
      (format!("vapid t={token}, k={key}"), key)
   }
}

#[cfg(test)]
mod tests {
   use p256::ecdsa::{
      SigningKey,
      signature::Signer,
   };

   use super::*;

   fn keypair() -> (SigningKey, String) {
      let signing = SigningKey::from_slice(&[7u8; 32]).unwrap();
      let public = signing.verifying_key().to_sec1_point(false);
      (signing, BASE64URL_NOPAD.encode(public.as_bytes()))
   }

   fn jwt(signing: &SigningKey, aud: &str, exp: u64) -> String {
      let header = BASE64URL_NOPAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
      let payload = BASE64URL_NOPAD
         .encode(format!(r#"{{"aud":"{aud}","exp":{exp},"sub":"mailto:t@t"}}"#).as_bytes());
      let signed = format!("{header}.{payload}");
      let signature: Signature = signing.sign(signed.as_bytes());
      format!("{signed}.{}", BASE64URL_NOPAD.encode(&signature.to_bytes()))
   }

   fn auth(signing: &SigningKey, key_b64: &str, aud: &str, exp: u64) -> String {
      format!("vapid t={}, k={key_b64}", jwt(signing, aud, exp))
   }

   const ORIGIN: &str = "https://push.benzeneos.org";
   const NOW: u64 = 1_800_000_000;

   #[test]
   fn accepts_valid_token() {
      let (signing, key) = keypair();
      let header = auth(&signing, &key, ORIGIN, NOW + 3600);
      assert_eq!(validate(Some(&header), &key, ORIGIN, NOW), Ok(()));
   }

   #[test]
   fn rejects_missing_header() {
      let (_, key) = keypair();
      assert_eq!(
         validate(None, &key, ORIGIN, NOW),
         Err(VapidRejection::MissingHeader)
      );
   }

   #[test]
   fn rejects_key_mismatch() {
      let (signing, key) = keypair();
      let other = SigningKey::from_slice(&[9u8; 32]).unwrap();
      let other_b64 = BASE64URL_NOPAD.encode(other.verifying_key().to_sec1_point(false).as_bytes());
      let header = auth(&signing, &key, ORIGIN, NOW + 3600);
      assert_eq!(
         validate(Some(&header), &other_b64, ORIGIN, NOW),
         Err(VapidRejection::KeyMismatch)
      );
   }

   #[test]
   fn rejects_wrong_signature() {
      let (_, key) = keypair();
      let other = SigningKey::from_slice(&[9u8; 32]).unwrap();
      let header = format!("vapid t={}, k={key}", jwt(&other, ORIGIN, NOW + 3600));
      assert_eq!(
         validate(Some(&header), &key, ORIGIN, NOW),
         Err(VapidRejection::BadSignature)
      );
   }

   #[test]
   fn rejects_alg_none_signed() {
      let (signing, key) = keypair();
      let header_b64 = BASE64URL_NOPAD.encode(br#"{"typ":"JWT","alg":"none"}"#);
      let payload_b64 = BASE64URL_NOPAD.encode(
         format!(
            r#"{{"aud":"{ORIGIN}","exp":{},"sub":"mailto:t@t"}}"#,
            NOW + 3600
         )
         .as_bytes(),
      );
      let signed = format!("{header_b64}.{payload_b64}");
      let signature: Signature = signing.sign(signed.as_bytes());
      let token = format!("{signed}.{}", BASE64URL_NOPAD.encode(&signature.to_bytes()));
      let header = format!("vapid t={token}, k={key}");
      assert_eq!(
         validate(Some(&header), &key, ORIGIN, NOW),
         Err(VapidRejection::BadToken)
      );
   }

   #[test]
   fn rejects_two_segment_token() {
      let (_, key) = keypair();
      let header = format!("vapid t=aaaa.bbbb, k={key}");
      assert_eq!(
         validate(Some(&header), &key, ORIGIN, NOW),
         Err(VapidRejection::BadToken)
      );
   }

   #[test]
   fn rejects_expired_beyond_skew() {
      let (signing, key) = keypair();
      let header = auth(&signing, &key, ORIGIN, NOW - CLOCK_SKEW_SECS - 1);
      assert_eq!(
         validate(Some(&header), &key, ORIGIN, NOW),
         Err(VapidRejection::Expired)
      );
   }

   #[test]
   fn accepts_expired_within_skew() {
      let (signing, key) = keypair();
      let header = auth(&signing, &key, ORIGIN, NOW - CLOCK_SKEW_SECS + 10);
      assert_eq!(validate(Some(&header), &key, ORIGIN, NOW), Ok(()));
   }

   #[test]
   fn rejects_exp_too_far_ahead() {
      let (signing, key) = keypair();
      let header = auth(
         &signing,
         &key,
         ORIGIN,
         NOW + MAX_EXP_AHEAD_SECS + CLOCK_SKEW_SECS + 10,
      );
      assert_eq!(
         validate(Some(&header), &key, ORIGIN, NOW),
         Err(VapidRejection::TooFarAhead)
      );
   }

   #[test]
   fn rejects_u64_max_exp() {
      let (signing, key) = keypair();
      let header = auth(&signing, &key, ORIGIN, u64::MAX);
      assert_eq!(
         validate(Some(&header), &key, ORIGIN, NOW),
         Err(VapidRejection::TooFarAhead)
      );
   }

   #[test]
   fn rejects_wrong_audience() {
      let (signing, key) = keypair();
      let header = auth(&signing, &key, "https://evil.example", NOW + 3600);
      assert_eq!(
         validate(Some(&header), &key, ORIGIN, NOW),
         Err(VapidRejection::BadAudience)
      );
   }

   #[test]
   fn accepts_trailing_slash_audience() {
      let (signing, key) = keypair();
      let header = auth(&signing, &key, "https://push.benzeneos.org/", NOW + 3600);
      assert_eq!(validate(Some(&header), &key, ORIGIN, NOW), Ok(()));
   }

   #[test]
   fn rejects_webpush_legacy_scheme() {
      let (signing, key) = keypair();
      let header = format!("WebPush {}", jwt(&signing, ORIGIN, NOW + 3600));
      assert_eq!(
         validate(Some(&header), &key, ORIGIN, NOW),
         Err(VapidRejection::MalformedHeader)
      );
   }

   #[test]
   fn accepts_mixed_case_scheme() {
      let (signing, key) = keypair();
      let token = jwt(&signing, ORIGIN, NOW + 3600);
      for scheme in ["Vapid", "VAPID", "vApId"] {
         let header = format!("{scheme} t={token}, k={key}");
         assert_eq!(
            validate(Some(&header), &key, ORIGIN, NOW),
            Ok(()),
            "{scheme}"
         );
      }
      let nospace = format!("vapidt={token},k={key}");
      assert_eq!(
         validate(Some(&nospace), &key, ORIGIN, NOW),
         Err(VapidRejection::MalformedHeader)
      );
   }

   #[test]
   fn rejects_missing_key_param() {
      let (signing, key) = keypair();
      let token = jwt(&signing, ORIGIN, NOW + 3600);
      let header = format!("vapid t={token}");
      assert_eq!(
         validate(Some(&header), &key, ORIGIN, NOW),
         Err(VapidRejection::MalformedHeader)
      );
   }

   #[test]
   fn accepts_padded_or_standard_alphabet_k() {
      let (signing, key) = keypair();
      let token = jwt(&signing, ORIGIN, NOW + 3600);
      let padded = format!("{key}=");
      let standard = key.replace('-', "+").replace('_', "/");
      for variant in [padded, standard] {
         let header = format!("vapid t={token}, k={variant}");
         assert_eq!(validate(Some(&header), &key, ORIGIN, NOW), Ok(()));
      }
   }

   #[test]
   fn decode_public_key_enforces_format() {
      let (_, key) = keypair();
      assert!(decode_public_key(&key).is_some());
      assert!(decode_public_key("short").is_none());
      assert!(decode_public_key(&"A".repeat(87)).is_none());
   }
}
