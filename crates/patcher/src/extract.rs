//! Firebase credential extraction
//!
//! Extracts Firebase configuration from APK resources.

use std::path::Path;

use anyhow::{
   Context as _,
   Result,
};
use regex_lite::Regex;
use serde::{
   Deserialize,
   Serialize,
};
use walkdir::WalkDir;

/// Firebase credentials extracted from an APK
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct FirebaseCredentials {
   pub project_id:     Option<String>,
   pub app_id:         Option<String>,
   pub api_key:        Option<String>,
   pub sender_id:      Option<String>,
   pub database_url:   Option<String>,
   pub storage_bucket: Option<String>,
}

/// Extract Firebase credentials from an APK
pub fn extract_firebase_credentials(apk_path: &Path) -> Result<FirebaseCredentials> {
   let temp_dir = std::env::temp_dir().join("pushcompat-extract");
   let _ = std::fs::remove_dir_all(&temp_dir);
   std::fs::create_dir_all(&temp_dir)?;

   // Decode APK
   crate::apk::decode_apk(apk_path, &temp_dir)?;

   let creds = extract_firebase_credentials_from_decoded(&temp_dir)?;

   // Cleanup
   let _ = std::fs::remove_dir_all(&temp_dir);

   Ok(creds)
}

/// Extract Firebase credentials from an already-decoded APK directory
pub fn extract_firebase_credentials_from_decoded(
   decoded_dir: &Path,
) -> Result<FirebaseCredentials> {
   let mut creds = FirebaseCredentials::default();

   // Try to find google-services.json in raw resources
   let raw_dir = decoded_dir.join("res/raw");
   if raw_dir.exists() {
      for entry in std::fs::read_dir(&raw_dir)? {
         let entry = entry?;
         let path = entry.path();
         if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains("google"))
            && let Ok(content) = std::fs::read_to_string(&path)
               && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                  extract_from_google_services_json(&json, &mut creds);
               }
      }
   }

   // Extract from strings.xml
   let strings_path = decoded_dir.join("res/values/strings.xml");
   if strings_path.exists() {
      let content = std::fs::read_to_string(&strings_path)?;
      extract_from_strings_xml(&content, &mut creds);
   }

   // Search all values files for Firebase strings
   let values_dir = decoded_dir.join("res/values");
   if values_dir.exists() {
      for entry in WalkDir::new(&values_dir)
         .into_iter()
         .filter_map(std::result::Result::ok)
         .filter(|e| e.path().extension().is_some_and(|ext| ext == "xml"))
      {
         let content = std::fs::read_to_string(entry.path())?;
         extract_from_strings_xml(&content, &mut creds);
      }
   }

   // Check AndroidManifest.xml for metadata
   let manifest_path = decoded_dir.join("AndroidManifest.xml");
   if manifest_path.exists() {
      let content = std::fs::read_to_string(&manifest_path)?;
      extract_from_manifest(&content, &mut creds);
   }

   Ok(creds)
}

fn extract_from_google_services_json(json: &serde_json::Value, creds: &mut FirebaseCredentials) {
   if let Some(project_id) = json["project_info"]["project_id"].as_str() {
      creds.project_id = Some(project_id.to_owned());
   }

   if let Some(project_number) = json["project_info"]["project_number"].as_str() {
      creds.sender_id = Some(project_number.to_owned());
   }

   if let Some(storage_bucket) = json["project_info"]["storage_bucket"].as_str() {
      creds.storage_bucket = Some(storage_bucket.to_owned());
   }

   if let Some(firebase_url) = json["project_info"]["firebase_url"].as_str() {
      creds.database_url = Some(firebase_url.to_owned());
   }

   // Get client info
   if let Some(clients) = json["client"].as_array() {
      for client in clients {
         if let Some(app_id) = client["client_info"]["mobilesdk_app_id"].as_str() {
            creds.app_id = Some(app_id.to_owned());
         }

         // Get API key
         if let Some(api_keys) = client["api_key"].as_array() {
            for key in api_keys {
               if let Some(current_key) = key["current_key"].as_str() {
                  creds.api_key = Some(current_key.to_owned());
                  break;
               }
            }
         }
      }
   }
}

fn extract_from_strings_xml(content: &str, creds: &mut FirebaseCredentials) {
   // Common Firebase string resource names
   let patterns = [
      ("google_app_id", &mut creds.app_id),
      ("gcm_defaultSenderId", &mut creds.sender_id),
      ("default_web_client_id", &mut None::<String>), // Not needed but common
      ("firebase_database_url", &mut creds.database_url),
      ("google_api_key", &mut creds.api_key),
      ("google_storage_bucket", &mut creds.storage_bucket),
      ("project_id", &mut creds.project_id),
   ];

   for (name, target) in patterns {
      if target.is_none()
         && let Some(value) = extract_string_resource(content, name) {
            *target = Some(value);
         }
   }
}

fn extract_string_resource(xml: &str, name: &str) -> Option<String> {
   let pattern = format!(
      r#"<string name="{}"[^>]*>([^<]+)</string>"#,
      regex_lite::escape(name)
   );
   let re = Regex::new(&pattern).ok()?;

   re.captures(xml)
      .and_then(|caps| caps.get(1))
      .map(|m| m.as_str().to_owned())
}

fn extract_from_manifest(content: &str, _creds: &mut FirebaseCredentials) {
   // Look for Firebase metadata in manifest
   let metadata_pattern =
      r#"<meta-data[^>]*android:name="([^"]+)"[^>]*android:value="([^"]+)"[^>]*/>"#;
   let re = Regex::new(metadata_pattern).unwrap();

   for caps in re.captures_iter(content) {
      let name = caps.get(1).map_or("", |m| m.as_str());
      let value = caps.get(2).map_or("", |m| m.as_str());

      match name {
         "com.google.firebase.messaging.default_notification_channel_id" => {},
         "firebase_messaging_auto_init_enabled" => {},
         name if name.contains("firebase") || name.contains("gcm") => {
            println!("  Found metadata: {name} = {value}");
         },
         _ => {},
      }
   }
}

/// Extract the package name from AndroidManifest.xml
pub fn extract_package_name(decoded_dir: &Path) -> Result<String> {
   let manifest_path = decoded_dir.join("AndroidManifest.xml");
   let content =
      std::fs::read_to_string(&manifest_path).context("Failed to read AndroidManifest.xml")?;

   let re = Regex::new(r#"package="([^"]+)""#)?;
   let caps = re
      .captures(&content)
      .context("Package name not found in manifest")?;

   Ok(caps.get(1).unwrap().as_str().to_owned())
}

/// Extract the signing certificate SHA1 fingerprint from an APK
/// Returns lowercase hex without colons, e.g.,
/// "38918a453d07199354f8b19af05ec6562ced5788"
pub fn extract_cert_sha1(apk_path: &Path) -> Result<String> {
   // Try apksigner first
   let output = std::process::Command::new("apksigner")
      .args(["verify", "--print-certs"])
      .arg(apk_path)
      .output()
      .context("Failed to run apksigner")?;

   if output.status.success() {
      let stdout = String::from_utf8_lossy(&output.stdout);
      // Look for "Signer #1 certificate SHA-1 digest: <hex>"
      let re = Regex::new(r"SHA-1 digest:\s*([0-9a-fA-F]+)")?;
      if let Some(caps) = re.captures(&stdout) {
         let sha1 = caps.get(1).unwrap().as_str().to_lowercase();
         return Ok(sha1);
      }
   }

   // Fallback: try keytool
   // First, extract the cert from the APK
   let temp_dir = std::env::temp_dir().join("pushcompat-cert");
   let _ = std::fs::remove_dir_all(&temp_dir);
   std::fs::create_dir_all(&temp_dir)?;

   // Open APK as zip and find cert file
   let file = std::fs::File::open(apk_path)?;
   let mut archive = zip::ZipArchive::new(file)?;

   let mut cert_file = None;
   for i in 0..archive.len() {
      let file = archive.by_index(i)?;
      let name = file.name().to_owned();
      if name.starts_with("META-INF/")
         && (name.ends_with(".RSA") || name.ends_with(".DSA") || name.ends_with(".EC"))
      {
         cert_file = Some(name);
         break;
      }
   }

   let cert_name = cert_file.context("No signing certificate found in APK")?;
   let cert_path = temp_dir.join("cert.rsa");

   {
      let mut file = archive.by_name(&cert_name)?;
      let mut cert_out = std::fs::File::create(&cert_path)?;
      std::io::copy(&mut file, &mut cert_out)?;
   }

   // Use keytool to get fingerprint
   let output = std::process::Command::new("keytool")
      .args(["-printcert", "-file"])
      .arg(&cert_path)
      .output()
      .context("Failed to run keytool")?;

   let _ = std::fs::remove_dir_all(&temp_dir);

   if output.status.success() {
      let stdout = String::from_utf8_lossy(&output.stdout);
      // Look for "SHA1: XX:XX:XX..."
      let re = Regex::new(r"SHA1:\s*([0-9A-Fa-f:]+)")?;
      if let Some(caps) = re.captures(&stdout) {
         let sha1 = caps
            .get(1)
            .unwrap()
            .as_str()
            .replace(':', "")
            .to_lowercase();
         return Ok(sha1);
      }
   }

   anyhow::bail!("Could not extract certificate SHA1 from APK")
}
