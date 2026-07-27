//! APK patching logic
//!
//! Orchestrates the complete patching process:
//! 1. Decode APK
//! 2. Inject shim DEX
//! 3. Patch smali hooks
//! 4. Update manifest
//! 5. Build and sign

use std::{
   fs,
   path::{
      Path,
      PathBuf,
   },
};

use anyhow::{
   Context as _,
   Result,
   bail,
};
use regex_lite::Regex;
use walkdir::WalkDir;

use crate::{
   apk,
   extract,
   manifest,
};

/// Configuration for patching an APK
pub struct PatchConfig {
   pub input:         PathBuf,
   pub output:        PathBuf,
   pub bridge_url:    String,
   pub distributor:   String,
   pub shim_dex:      Option<PathBuf>,
   pub keystore:      Option<PathBuf>,
   pub keystore_pass: Option<String>,
   pub key_alias:     Option<String>,
}

/// Patch an APK for `UnifiedPush` support
pub fn patch_apk(config: PatchConfig) -> Result<()> {
   // Create temp directory
   let temp_dir = std::env::temp_dir().join("pushcompat-patch");
   let _ = fs::remove_dir_all(&temp_dir);
   fs::create_dir_all(&temp_dir)?;

   let decoded_dir = temp_dir.join("decoded");

   // Step 0: Extract original cert SHA1 BEFORE modifying the APK
   // This is critical because re-signing changes the cert, but Firebase validates
   // against the original
   println!("\n[0/8] Extracting original signing certificate...");
   let cert_sha1 = match extract::extract_cert_sha1(&config.input) {
      Ok(sha1) => {
         println!("  Cert SHA1: {sha1}");
         Some(sha1)
      },
      Err(e) => {
         println!("  Warning: Could not extract cert SHA1: {e}");
         println!("  FCM registration may fail without valid certificate");
         None
      },
   };

   // Step 1: Decode APK
   println!("\n[1/8] Decoding APK...");
   apk::decode_apk(&config.input, &decoded_dir)?;

   // Get package name
   let package_name = extract::extract_package_name(&decoded_dir)?;
   println!("  Package: {package_name}");

   // Step 2: Extract Firebase credentials
   println!("\n[2/8] Extracting Firebase credentials...");
   let firebase_creds = extract::extract_firebase_credentials_from_decoded(&decoded_dir)?;
   if firebase_creds.app_id.is_some() {
      println!("  App ID: {}", firebase_creds.app_id.as_ref().unwrap());
      println!(
         "  Project: {}",
         firebase_creds.project_id.as_deref().unwrap_or("unknown")
      );
      println!(
         "  API Key: {}...",
         &firebase_creds.api_key.as_deref().unwrap_or("none")
            [..20.min(firebase_creds.api_key.as_deref().unwrap_or("").len())]
      );
   } else {
      println!("  Warning: Could not extract Firebase credentials");
      println!("  The bridge may not be able to receive FCM messages");
   }

   // Step 3: Find the app's own Firebase broadcast receiver
   println!("\n[3/8] Analyzing FCM integration...");
   let manifest_path = decoded_dir.join("AndroidManifest.xml");
   let c2dm_receiver = manifest::find_c2dm_receiver(&manifest_path)?;

   if let Some(name) = &c2dm_receiver { println!("  c2dm receiver: {name}") } else {
      println!("  Warning: no receiver handles com.google.android.c2dm.intent.RECEIVE");
      println!("  Nothing to deliver into; the app likely does not use FCM");
   }

   // Step 4: Inject shim DEX
   println!("\n[4/8] Injecting shim...");
   inject_shim_dex(&decoded_dir, config.shim_dex.as_deref())?;

   // Step 5: Patch smali hooks
   println!("\n[5/8] Patching hooks...");
   patch_firebase_get_token(&decoded_dir)?;
   patch_application_class(
      &decoded_dir,
      &config.bridge_url,
      &config.distributor,
      &firebase_creds,
      c2dm_receiver.as_deref(),
      cert_sha1.as_deref(),
   )?;

   // Step 6: Update manifest
   println!("\n[6/8] Updating manifest...");
   manifest::remove_split_requirements(&manifest_path)?;
   manifest::add_unifiedpush_receiver(&manifest_path, &package_name)?;
   if !config.bridge_url.starts_with("https://") {
      manifest::allow_cleartext(&manifest_path)?;
   }

   // Step 7: Build and sign
   println!("\n[7/8] Building APK...");
   let rebuilt = temp_dir.join("rebuilt.apk");
   apk::build_apk(&decoded_dir, &rebuilt)?;
   apk::graft_onto_original(&config.input, &rebuilt, &config.output)?;
   apk::zipalign_apk(&config.output)?;
   apk::sign_apk(
      &config.output,
      config.keystore.as_deref(),
      config.keystore_pass.as_deref(),
      config.key_alias.as_deref(),
   )?;

   // Cleanup
   let _ = fs::remove_dir_all(&temp_dir);

   println!("\nDone! Patched APK: {}", config.output.display());
   println!("\nNext steps:");
   println!("  1. Install the patched APK on your device");
   println!("  2. Ensure ntfy (or your distributor) is installed");
   println!(
      "  3. Configure your bridge server at: {}",
      config.bridge_url
   );

   Ok(())
}

/// Inject the shim DEX into the decoded APK
fn inject_shim_dex(decoded_dir: &Path, shim_dex_path: Option<&Path>) -> Result<()> {
   let next_dex_num = apk::get_next_dex_number(decoded_dir);
   let target_smali_dir = decoded_dir.join(format!("smali_classes{next_dex_num}"));

   fs::create_dir_all(&target_smali_dir)?;

   let shim_dex = if let Some(path) = shim_dex_path {
      path.to_path_buf()
   } else {
      std::env::var_os("PUSHCOMPAT_SHIM_DEX")
         .map(PathBuf::from)
         .context("Shim DEX not found. Specify --shim-dex or use the Nix-built patcher.")?
   };

   println!("  Using shim: {}", shim_dex.display());

   // Look for pre-generated smali files next to the DEX
   let shim_smali_dir = shim_dex.parent().map(|p| p.join("smali"));

   if let Some(ref smali_dir) = shim_smali_dir {
      if smali_dir.exists() && smali_dir.is_dir() {
         // Copy pre-generated smali files
         println!("  Using pre-generated smali from: {}", smali_dir.display());
         copy_dir_recursive(smali_dir, &target_smali_dir)?;
      } else {
         // Fall back to baksmali - try BAKSMALI_JAR env var first (for nix develop)
         let baksmali_jar = std::env::var("BAKSMALI_JAR");

         let status = if let Ok(jar_path) = baksmali_jar {
            std::process::Command::new("java")
               .args(["-jar", &jar_path, "d", "-o"])
               .arg(&target_smali_dir)
               .arg(&shim_dex)
               .status()
               .context("Failed to run baksmali via java -jar. Check BAKSMALI_JAR.")?
         } else {
            std::process::Command::new("baksmali")
               .args(["d", "-o"])
               .arg(&target_smali_dir)
               .arg(&shim_dex)
               .status()
               .context(
                  "Failed to run baksmali. Is it installed? Or provide pre-generated smali files.",
               )?
         };

         if !status.success() {
            bail!("baksmali failed to disassemble shim DEX");
         }
      }
   } else {
      bail!("Invalid shim DEX path");
   }

   // Count injected classes
   let class_count = WalkDir::new(&target_smali_dir)
      .into_iter()
      .filter_map(std::result::Result::ok)
      .filter(|e| e.path().extension().is_some_and(|ext| ext == "smali"))
      .count();

   println!(
      "  Injected {class_count} classes into smali_classes{next_dex_num}"
   );

   Ok(())
}

/// Make `FirebaseMessaging.getToken()` return an already-completed Task.
///
/// Apps chain real setup onto that Task. GitHub registers every one of its
/// notification channels inside the completion callback, so without GMS the
/// callback never runs, the channels never exist, and Android rejects each
/// notification with "No Channel found" — after the payload has been delivered
/// and decrypted perfectly. Nothing about that failure points at push delivery,
/// which is what made it expensive to find.
///
/// The class name survives R8 (it is referenced reflectively by the SDK), and
/// getToken is identifiable as the only no-argument method returning the Task
/// type.
fn patch_firebase_get_token(decoded_dir: &Path) -> Result<()> {
   let Some(path) = find_smali(
      decoded_dir,
      "com/google/firebase/messaging/FirebaseMessaging.smali",
   ) else {
      println!("  FirebaseMessaging not found, skipping getToken hook");
      return Ok(());
   };

   let content = fs::read_to_string(&path)?;

   let signature = Regex::new(r"(?m)^\.method public final (\w+)\(\)(L[\w/$]+;)\s*$")?;
   // Several no-arg methods return objects; the Task is the one whose class
   // exposes both an isSuccessful-style predicate and a setResult taking
   // Object.
   let is_task = |ret: &str| {
      let relative = format!(
         "{}.smali",
         ret.trim_start_matches('L').trim_end_matches(';')
      );
      find_smali(decoded_dir, &relative)
         .and_then(|path| fs::read_to_string(path).ok())
         .is_some_and(|body| body.contains("(Ljava/lang/Object;)V") && body.contains("()Z"))
   };

   let candidates = signature
      .captures_iter(&content)
      .map(|c| (c[1].to_string(), c[2].to_string()))
      .filter(|(_, ret)| is_task(ret))
      .collect::<Vec<(String, String)>>();

   let [(method, task_type)] = candidates.as_slice() else {
      println!(
         "  Expected exactly one no-arg Task-returning method, found {} — skipping",
         candidates.len()
      );
      return Ok(());
   };

   let header = format!(".method public final {method}(){task_type}");
   let Some(start) = content.find(&header) else {
      return Ok(());
   };
   let end = content[start..]
      .find(".end method")
      .map(|i| start + i + ".end method".len())
      .context("unterminated getToken method")?;

   let replacement = format!(
        "{header}\n    .locals 2\n\n             new-instance v0, {task_type}\n\n             invoke-direct {{v0}}, {task_type}-><init>()V\n\n             invoke-static {{}}, Lcom/benzeneos/pushcompat/shim/PushCompatShim;->currentToken()Ljava/lang/String;\n\n             move-result-object v1\n\n             invoke-virtual {{v0, v1}}, {task_type}->m(Ljava/lang/Object;)V\n\n             return-object v0\n.end method"
    );

   let patched = format!("{}{}{}", &content[..start], replacement, &content[end..]);
   fs::write(&path, patched)?;
   println!("  Hooked FirebaseMessaging.{method}() -> completed {task_type}");

   Ok(())
}

/// Locate a smali file by its class path across every `smali_classes`* directory.
fn find_smali(decoded_dir: &Path, relative: &str) -> Option<PathBuf> {
   apk::find_smali_dirs(decoded_dir)
      .into_iter()
      .map(|dir| dir.join(relative))
      .find(|path| path.exists())
}

/// Patch the Application class to initialize `PushCompat`
fn patch_application_class(
   decoded_dir: &Path,
   bridge_url: &str,
   distributor: &str,
   firebase_creds: &extract::FirebaseCredentials,
   c2dm_receiver: Option<&str>,
   cert_sha1: Option<&str>,
) -> Result<()> {
   let manifest_path = decoded_dir.join("AndroidManifest.xml");

   // Find application class
   let app_class = manifest::get_application_class(&manifest_path)?;

   if let Some(class_name) = app_class {
      println!("  Application class: {class_name}");

      // Convert class name to smali path
      let smali_path = class_name_to_smali_path(decoded_dir, &class_name)?;

      if let Some(path) = smali_path {
         patch_application_on_create(
            &path,
            bridge_url,
            distributor,
            firebase_creds,
            c2dm_receiver,
            cert_sha1,
         )?;
      } else {
         println!("  Warning: Could not find Application class smali file");
         create_init_provider(
            decoded_dir,
            bridge_url,
            distributor,
            firebase_creds,
            c2dm_receiver,
            cert_sha1,
         )?;
      }
   } else {
      println!("  No custom Application class, using ContentProvider init");
      create_init_provider(
         decoded_dir,
         bridge_url,
         distributor,
         firebase_creds,
         c2dm_receiver,
         cert_sha1,
      )?;
   }

   Ok(())
}

/// Convert a Java class name to smali file path
fn class_name_to_smali_path(decoded_dir: &Path, class_name: &str) -> Result<Option<PathBuf>> {
   let relative_path = class_name.replace('.', "/") + ".smali";

   // Search in all smali directories
   for smali_dir in apk::find_smali_dirs(decoded_dir) {
      let full_path = smali_dir.join(&relative_path);
      if full_path.exists() {
         return Ok(Some(full_path));
      }
   }

   Ok(None)
}

/// Patch Application.onCreate to initialize `PushCompat`
fn patch_application_on_create(
   smali_path: &Path,
   bridge_url: &str,
   distributor: &str,
   firebase_creds: &extract::FirebaseCredentials,
   c2dm_receiver: Option<&str>,
   cert_sha1: Option<&str>,
) -> Result<()> {
   let content = fs::read_to_string(smali_path)?;

   // Remove an existing PushCompat patch so the APK can be repatched with new
   // config.
   let re_existing_patch = Regex::new(
      r"(?s)\n\s*# PushCompat:.*?Lcom/benzeneos/pushcompat/shim/PushCompatShim;->register\(Landroid/content/Context;\)V",
   )?;
   let content = re_existing_patch.replace_all(&content, "").to_string();

   // First, find the current .locals count for onCreate
   let locals_pattern = r"\.method[^\n]*onCreate\(\)V[^\n]*\n\s*\.locals (\d+)";
   let re_locals = Regex::new(locals_pattern)?;
   let current_locals = re_locals
      .captures(&content)
      .and_then(|c| c.get(1))
      .and_then(|m| m.as_str().parse::<u32>().ok())
      .unwrap_or(4);

   // We need 9 registers for our code (context + 8 string args including cert
   // SHA1) Use registers at the end of the range to avoid clobbering
   let base_reg = current_locals;
   let new_locals = current_locals + 9;

   // Get Firebase credential strings (or null placeholders)
   let fb_app_id = firebase_creds.app_id.as_deref().unwrap_or("");
   let fb_project_id = firebase_creds.project_id.as_deref().unwrap_or("");
   let fb_api_key = firebase_creds.api_key.as_deref().unwrap_or("");
   let fcm_svc_class = c2dm_receiver.unwrap_or("");
   let cert = cert_sha1.unwrap_or("");

   // Generate init code using high registers and invoke-static/range
   // Note: const/4 only works with v0-v15, use const/16 for high registers
   // Configure signature: (Context, bridgeUrl, distributor, firebaseAppId,
   // firebaseProjectId, firebaseApiKey, fcmServiceClass, certSha1)
   let init_code = format!(
      r#"
    # PushCompat: Initialize shim with Firebase credentials, FCM service class, and cert
    move-object/from16 v{base}, p0
    const-string v{url}, "{bridge_url}"
    const-string v{dist}, "{distributor}"
    const-string v{app_id}, "{fb_app_id}"
    const-string v{proj_id}, "{fb_project_id}"
    const-string v{api_key}, "{fb_api_key}"
    const-string v{fcm_svc}, "{fcm_svc_class}"
    const-string v{cert_reg}, "{cert}"
    invoke-static/range {{v{base} .. v{cert_reg}}}, Lcom/benzeneos/pushcompat/shim/PushCompatShim;->configure(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V

    # PushCompat: Register with UnifiedPush
    invoke-static/range {{v{base} .. v{base}}}, Lcom/benzeneos/pushcompat/shim/PushCompatShim;->register(Landroid/content/Context;)V
"#,
      base = base_reg,
      url = base_reg + 1,
      dist = base_reg + 2,
      app_id = base_reg + 3,
      proj_id = base_reg + 4,
      api_key = base_reg + 5,
      fcm_svc = base_reg + 6,
      cert_reg = base_reg + 7,
      bridge_url = bridge_url,
      distributor = distributor,
      fb_app_id = fb_app_id,
      fb_project_id = fb_project_id,
      fb_api_key = fb_api_key,
      fcm_svc_class = fcm_svc_class,
      cert = cert,
   );

   // Find onCreate and inject after super.onCreate()
   let super_oncreate_pattern = r"(invoke-\w+ \{[^}]*\}, L[^;]+;->onCreate\(\)V)";
   let re = Regex::new(super_oncreate_pattern)?;

   let new_content = if re.is_match(&content) {
      re.replace(&content, |caps: &regex_lite::Captures| {
         format!("{}{}", &caps[1], init_code)
      })
      .to_string()
   } else {
      // Try to find the start of onCreate method
      let oncreate_start = r"(\.method[^\n]*onCreate\(\)V[^\n]*\n\s*\.locals \d+)";
      let re2 = Regex::new(oncreate_start)?;

      if re2.is_match(&content) {
         re2.replace(&content, |caps: &regex_lite::Captures| {
            format!("{}{}", &caps[1], init_code)
         })
         .to_string()
      } else {
         println!("  Warning: Could not find suitable injection point in onCreate");
         content
      }
   };

   // Update .locals count
   let new_content = re_locals
      .replace(&new_content, |caps: &regex_lite::Captures| {
         caps[0].replace(
            &format!(".locals {current_locals}"),
            &format!(".locals {new_locals}"),
         )
      })
      .to_string();

   fs::write(smali_path, new_content)?;
   println!(
      "  Injected init code into Application.onCreate (using v{}-v{})",
      base_reg,
      base_reg + 7
   );

   Ok(())
}

/// Create a `ContentProvider` to initialize `PushCompat` if no Application class
fn create_init_provider(
   decoded_dir: &Path,
   bridge_url: &str,
   distributor: &str,
   firebase_creds: &extract::FirebaseCredentials,
   c2dm_receiver: Option<&str>,
   cert_sha1: Option<&str>,
) -> Result<()> {
   let fb_app_id = firebase_creds.app_id.as_deref().unwrap_or("");
   let fb_project_id = firebase_creds.project_id.as_deref().unwrap_or("");
   let fb_api_key = firebase_creds.api_key.as_deref().unwrap_or("");
   let fcm_svc_class = c2dm_receiver.unwrap_or("");
   let cert = cert_sha1.unwrap_or("");

   // Create a ContentProvider that initializes on app start
   let provider_smali = format!(
      r#".class public Lcom/benzeneos/pushcompat/shim/PushCompatInitProvider;
.super Landroid/content/ContentProvider;
.source "PushCompatInitProvider.java"

.method public constructor <init>()V
    .locals 0
    invoke-direct {{p0}}, Landroid/content/ContentProvider;-><init>()V
    return-void
.end method

.method public onCreate()Z
    .locals 9

    # Get context
    invoke-virtual {{p0}}, Landroid/content/ContentProvider;->getContext()Landroid/content/Context;
    move-result-object v0

    # Configure shim with Firebase credentials, FCM service class, and cert
    const-string v1, "{bridge_url}"
    const-string v2, "{distributor}"
    const-string v3, "{fb_app_id}"
    const-string v4, "{fb_project_id}"
    const-string v5, "{fb_api_key}"
    const-string v6, "{fcm_svc_class}"
    const-string v7, "{cert}"
    invoke-static/range {{v0 .. v7}}, Lcom/benzeneos/pushcompat/shim/PushCompatShim;->configure(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V

    # Register with UnifiedPush
    invoke-static {{v0}}, Lcom/benzeneos/pushcompat/shim/PushCompatShim;->register(Landroid/content/Context;)V

    const/4 v0, 0x1
    return v0
.end method

.method public delete(Landroid/net/Uri;Ljava/lang/String;[Ljava/lang/String;)I
    .locals 0
    const/4 v0, 0x0
    return v0
.end method

.method public getType(Landroid/net/Uri;)Ljava/lang/String;
    .locals 0
    const/4 v0, 0x0
    return-object v0
.end method

.method public insert(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;
    .locals 0
    const/4 v0, 0x0
    return-object v0
.end method

.method public query(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;
    .locals 0
    const/4 v0, 0x0
    return-object v0
.end method

.method public update(Landroid/net/Uri;Landroid/content/ContentValues;Ljava/lang/String;[Ljava/lang/String;)I
    .locals 0
    const/4 v0, 0x0
    return v0
.end method
"#,
   );

   // Find the best smali directory to add it to
   let next_dex = apk::get_next_dex_number(decoded_dir);
   let target_dir = decoded_dir.join(format!(
      "smali_classes{next_dex}/com/benzeneos/pushcompat/shim"
   ));
   fs::create_dir_all(&target_dir)?;

   fs::write(
      target_dir.join("PushCompatInitProvider.smali"),
      provider_smali,
   )?;

   // Add provider to manifest
   let manifest_path = decoded_dir.join("AndroidManifest.xml");
   let manifest = fs::read_to_string(&manifest_path)?;

   if !manifest.contains("PushCompatInitProvider") {
      let package_re = Regex::new(r#"package="([^"]+)""#)?;
      let package_name = package_re
         .captures(&manifest)
         .and_then(|c| c.get(1))
         .map_or("com.example", |m| m.as_str());

      let provider_decl = format!(
         r#"
        <provider
            android:name="com.benzeneos.pushcompat.shim.PushCompatInitProvider"
            android:authorities="{package_name}.pushcompat.init"
            android:exported="false"
            android:initOrder="9999"/>
    "#
      );

      let new_manifest = manifest.replace(
         "</application>",
         &format!("{provider_decl}</application>"),
      );
      fs::write(&manifest_path, new_manifest)?;
   }

   println!("  Created init ContentProvider");
   Ok(())
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
   fs::create_dir_all(dst)?;

   for entry in WalkDir::new(src) {
      let entry = entry?;
      let src_path = entry.path();
      let relative = src_path.strip_prefix(src)?;
      let dst_path = dst.join(relative);

      if entry.file_type().is_dir() {
         fs::create_dir_all(&dst_path)?;
      } else {
         if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent)?;
         }
         fs::copy(src_path, &dst_path)?;
      }
   }

   Ok(())
}
