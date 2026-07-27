{
  android,
  pkgs,
  self,
  system,
}:

let
  patchGithub = pkgs.writeShellScript "patch-github" ''
    set -euo pipefail

    BRIDGE_URL="''${BRIDGE_URL:-https://push.benzeneos.org}"
    DISTRIBUTOR="''${DISTRIBUTOR:-io.heckel.ntfy}"
    GH_DIR="$HOME/projects/gh-android"
    INPUT_APK="$GH_DIR/apks/base.apk"
    OUTPUT_APK="$GH_DIR/github-pushcompat.apk"

    if [ ! -f "$INPUT_APK" ]; then
      echo "Error: Input APK not found at $INPUT_APK"
      echo "Pull it from your device with:"
      echo "  adb shell pm path com.github.android"
      echo "  adb pull <path>/base.apk $GH_DIR/apks/"
      exit 1
    fi

    echo "Building patcher..."
    PATCHER=$(nix build .#pushcompat-patcher --no-link --print-out-paths)/bin/pushcompat-patcher

    echo "Patching APK..."
    echo "  Input:  $INPUT_APK"
    echo "  Output: $OUTPUT_APK"
    echo "  Bridge: $BRIDGE_URL"
    echo "  Distributor: $DISTRIBUTOR"

    # Set up environment for apksigner, apktool, baksmali, smali
    export PATH="${android.buildTools}:${pkgs.apktool}/bin:${android.baksmali}/bin:${android.smali}/bin:$PATH"

    "$PATCHER" patch \
      -i "$INPUT_APK" \
      -o "$OUTPUT_APK" \
      -b "$BRIDGE_URL" \
      -d "$DISTRIBUTOR"

    echo ""
    echo "Done! Patched APK: $OUTPUT_APK"
    echo ""
    echo "To install:"
    echo "  adb install -r $OUTPUT_APK"
    echo "Or for split APKs:"
    echo "  adb install-multiple $OUTPUT_APK $GH_DIR/apks/split_config.arm64_v8a.apk $GH_DIR/apks/split_config.xhdpi.apk"
  '';

  patchGithubInstall = pkgs.writeShellScript "patch-github-install" ''
    set -euo pipefail

    GH_DIR="$HOME/projects/gh-android"

    # Run the patch script
    ${patchGithub}

    echo "Installing..."
    ${pkgs.android-tools}/bin/adb install-multiple \
      "$GH_DIR/github-pushcompat.apk" \
      "$GH_DIR/apks/split_config.arm64_v8a.apk" \
      "$GH_DIR/apks/split_config.xhdpi.apk"

    echo ""
    echo "Installed! Check logs with:"
    echo "  adb logcat -s 'PushCompat:*' 'PushCompatShim:*'"
  '';
in
{
  update-shim-deps = {
    type = "app";
    program = "${self.packages.${system}.pushcompat-shim.mitmCache.updateScript}";
    meta.description = "Update the shim's pinned Gradle dependencies";
  };

  patch-github = {
    type = "app";
    program = "${patchGithub}";
    meta.description = "Patch a GitHub Android APK for UnifiedPush";
  };

  patch-github-install = {
    type = "app";
    program = "${patchGithubInstall}";
    meta.description = "Patch and install a GitHub Android APK";
  };
}
