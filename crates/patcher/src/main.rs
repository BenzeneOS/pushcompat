//! `PushCompat` patcher - Patch Android APKs for push compatibility
//!
//! This tool patches Android applications to use `UnifiedPush` instead of FCM.
//! It injects a Kotlin shim library and hooks the app's Firebase messaging
//! service.

mod apk;
mod extract;
mod manifest;
mod patch;

use std::path::PathBuf;

use anyhow::Result;
use pound::Parse;

#[derive(Parse)]
#[pound(name = "pushcompat-patcher")]
struct Cli {
   #[pound(subcommand)]
   command: Commands,
}

#[derive(Parse)]
enum Commands {
   /// Patch an APK for `UnifiedPush` support
   Patch {
      /// Input APK file
      #[pound(short, long)]
      input: PathBuf,

      /// Output APK file (default: <input>-patched.apk)
      #[pound(short, long)]
      output: Option<PathBuf>,

      /// Bridge server URL
      #[pound(short, long, default = "https://fcm-bridge.example.com")]
      bridge_url: String,

      /// `UnifiedPush` distributor package
      #[pound(short, long, default = "io.heckel.ntfy")]
      distributor: String,

      /// Path to pre-built shim DEX (optional when provided by the Nix package)
      #[pound(long)]
      shim_dex: Option<PathBuf>,

      /// Keystore for signing (optional, uses debug key if not specified)
      #[pound(long)]
      keystore: Option<PathBuf>,

      /// Keystore password
      #[pound(long)]
      keystore_pass: Option<String>,

      /// Key alias
      #[pound(long)]
      key_alias: Option<String>,
   },

   /// Extract Firebase credentials from an APK (for analysis)
   Extract {
      /// Input APK file
      #[pound(short, long)]
      input: PathBuf,
   },

   /// Analyze an APK's FCM integration
   Analyze {
      /// Input APK file
      #[pound(short, long)]
      input: PathBuf,
   },
}

fn main() -> Result<()> {
   let cli = Cli::parse();

   match cli.command {
      Commands::Patch {
         input,
         output,
         bridge_url,
         distributor,
         shim_dex,
         keystore,
         keystore_pass,
         key_alias,
      } => {
         let output = output.unwrap_or_else(|| {
            let stem = input.file_stem().unwrap().to_str().unwrap();
            input.with_file_name(format!("{stem}-patched.apk"))
         });

         println!("Patching APK: {}", input.display());
         println!("Output: {}", output.display());
         println!("Bridge URL: {bridge_url}");
         println!("Distributor: {distributor}");

         let config = patch::PatchConfig {
            input,
            output,
            bridge_url,
            distributor,
            shim_dex,
            keystore,
            keystore_pass,
            key_alias,
         };

         patch::patch_apk(config)?;
      },

      Commands::Extract { input } => {
         println!("Extracting Firebase credentials from: {}", input.display());
         let creds = extract::extract_firebase_credentials(&input)?;
         println!("{}", serde_json::to_string_pretty(&creds)?);
      },

      Commands::Analyze { input } => {
         println!("Analyzing FCM integration in: {}", input.display());
         apk::analyze_fcm_integration(&input)?;
      },
   }

   Ok(())
}
