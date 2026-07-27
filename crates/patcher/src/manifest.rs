//! AndroidManifest.xml manipulation
//!
//! Adds `UnifiedPush` receiver and required permissions.

use std::path::Path;

use anyhow::{
   Context as _,
   Result,
};
use regex_lite::Regex;

/// Permit cleartext traffic, needed only when the bridge is reached over plain
/// HTTP. Apps targeting API 28+ block it by default, and the shim's
/// registration POST fails silently without this.
pub fn allow_cleartext(manifest_path: &Path) -> Result<()> {
   let content = std::fs::read_to_string(manifest_path)?;

   // The attribute is usually already present and set to false, so flip it rather
   // than assuming its absence.
   let patched = if content.contains("android:usesCleartextTraffic") {
      let re = Regex::new(r#"android:usesCleartextTraffic="[^"]*""#)?;
      re.replace(&content, r#"android:usesCleartextTraffic="true""#)
   } else {
      let re = Regex::new(r"(<application\b)")?;
      re.replace(&content, r#"$1 android:usesCleartextTraffic="true""#)
   };
   std::fs::write(manifest_path, patched.as_ref())?;
   println!("  Enabled cleartext traffic (bridge URL is not HTTPS)");
   Ok(())
}

/// Add the `UnifiedPush` receiver to AndroidManifest.xml
pub fn add_unifiedpush_receiver(manifest_path: &Path, _package_name: &str) -> Result<()> {
   let content =
      std::fs::read_to_string(manifest_path).context("Failed to read AndroidManifest.xml")?;

   // Check if already patched
   if content.contains("com.benzeneos.pushcompat.shim.PushCompatReceiver") {
      println!("  Manifest already contains PushCompat receiver, skipping");
      return Ok(());
   }

   let mut new_content = content;

   // Add INTERNET permission if not present
   if !new_content.contains("android.permission.INTERNET") {
      new_content = add_permission(&new_content, "android.permission.INTERNET");
   }

   // Add receiver declaration before </application>
   let receiver_declaration = r#"
        <receiver
            android:name="com.benzeneos.pushcompat.shim.PushCompatReceiver"
            android:exported="true">
            <intent-filter>
                <action android:name="org.unifiedpush.android.connector.MESSAGE"/>
                <action android:name="org.unifiedpush.android.connector.NEW_ENDPOINT"/>
                <action android:name="org.unifiedpush.android.connector.REGISTRATION_FAILED"/>
                <action android:name="org.unifiedpush.android.connector.UNREGISTERED"/>
            </intent-filter>
        </receiver>
    "#.to_owned();

   // Find </application> and insert before it
   let app_end = new_content
      .find("</application>")
      .context("</application> not found in manifest")?;

   new_content.insert_str(app_end, &receiver_declaration);

   // Add queries for ntfy package (required for Android 11+)
   if !new_content.contains("<queries>") {
      let queries_section = r#"
    <queries>
        <package android:name="io.heckel.ntfy"/>
    </queries>
"#;
      // Insert before <application
      if let Some(app_start) = new_content.find("<application") {
         new_content.insert_str(app_start, queries_section);
      }
   } else if !new_content.contains("io.heckel.ntfy") {
      // Add ntfy to existing queries
      let queries_end = new_content
         .find("</queries>")
         .context("</queries> not found")?;
      new_content.insert_str(
         queries_end,
         r#"        <package android:name="io.heckel.ntfy"/>
    "#,
      );
   }

   std::fs::write(manifest_path, new_content)?;
   Ok(())
}

fn add_permission(manifest: &str, permission: &str) -> String {
   let perm_line = format!(
      r#"    <uses-permission android:name="{permission}"/>
"#
   );

   // Find first <uses-permission or <application to insert before
   if let Some(pos) = manifest.find("<uses-permission") {
      let mut result = manifest.to_owned();
      result.insert_str(pos, &perm_line);
      result
   } else if let Some(pos) = manifest.find("<application") {
      let mut result = manifest.to_owned();
      result.insert_str(pos, &perm_line);
      result
   } else {
      manifest.to_owned()
   }
}

/// Remove split APK requirements from manifest (for base APK patching)
pub fn remove_split_requirements(manifest_path: &Path) -> Result<()> {
   let content = std::fs::read_to_string(manifest_path)?;

   // Remove android:requiredSplitTypes
   let re1 = Regex::new(r#"\s*android:requiredSplitTypes="[^"]*""#)?;
   let content = re1.replace_all(&content, "");

   // Remove android:splitTypes
   let re2 = Regex::new(r#"\s*android:splitTypes="[^"]*""#)?;
   let content = re2.replace_all(&content, "");

   // Remove split configuration metadata
   let re3 =
      Regex::new(r#"<meta-data[^>]*android:name="com\.android\.vending\.splits[^"]*"[^>]*/>\s*"#)?;
   let content = re3.replace_all(&content, "");

   // Remove android:isSplitRequired
   let re4 = Regex::new(r#"\s*android:isSplitRequired="[^"]*""#)?;
   let content = re4.replace_all(&content, "");

   std::fs::write(manifest_path, content.as_ref())?;
   Ok(())
}

/// Find the receiver that handles `com.google.android.c2dm.intent.RECEIVE`.
///
/// This is the only durable seam into an app's Firebase stack. Delivering by
/// starting `FirebaseMessagingService` directly does not work: it overrides
/// `getStartCommandIntent()` to poll `ServiceStarter`'s queue and discards whatever
/// intent it was started with. Broadcasting here instead lets the SDK's own
/// queue-and-bind path run.
///
/// Discovered from the manifest rather than hardcoded because R8 may rename the
/// class, but it must remain addressable in the merged manifest.
pub fn find_c2dm_receiver(manifest_path: &Path) -> Result<Option<String>> {
   let content = std::fs::read_to_string(manifest_path)?;
   let name_re = Regex::new(r#"android:name="([^"]+)""#)?;

   // Scanned rather than regex-paired: a self-closing `<receiver .../>` has no
   // body, and pairing its opening tag with the next `</receiver>` silently
   // attributes the *following* receiver's intent-filter to it. That mismatch
   // picks Play Analytics over Firebase.
   let mut candidates = Vec::new();
   let mut cursor = 0_usize;
   while let Some(offset) = content[cursor..].find("<receiver") {
      let open = cursor + offset;
      let Some(rel_close) = content[open..].find('>') else {
         break;
      };
      let tag_end = open + rel_close;
      let tag = &content[open..tag_end];
      cursor = tag_end + 1;

      // Self-closing: no intent-filter, so it cannot be the messaging receiver.
      if content[..tag_end].ends_with('/') {
         continue;
      }

      let Some(rel_end) = content[cursor..].find("</receiver>") else {
         break;
      };
      let body = &content[cursor..cursor + rel_end];

      if !body.contains("com.google.android.c2dm.intent.RECEIVE") {
         continue;
      }
      if let Some(name) = name_re.captures(tag).and_then(|c| c.get(1)) {
         candidates.push(name.as_str().to_owned());
      }
   }

   // Analytics and Measurement can also register for this action without
   // dispatching to FirebaseMessagingService. Firebase's own receiver keeps its
   // name through R8 because the manifest references it by name.
   let pick = candidates
      .iter()
      .find(|name| name.ends_with("FirebaseInstanceIdReceiver"))
      .or_else(|| {
         candidates.iter().find(|name| {
            let lower = name.to_ascii_lowercase();
            !lower.contains("analytics") && !lower.contains("measurement")
         })
      })
      .or_else(|| candidates.first());

   if candidates.len() > 1 {
      println!(
         "  ({} receivers matched c2dm RECEIVE, chose by preference)",
         candidates.len()
      );
   }

   Ok(pick.cloned())
}

/// Get application class name from manifest
pub fn get_application_class(manifest_path: &Path) -> Result<Option<String>> {
   let content = std::fs::read_to_string(manifest_path)?;

   let re = Regex::new(r#"<application[^>]*android:name="([^"]+)""#)?;

   Ok(re
      .captures(&content)
      .and_then(|caps| caps.get(1))
      .map(|m| m.as_str().to_owned()))
}
