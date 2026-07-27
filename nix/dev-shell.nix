{
  android,
  pkgs,
  toolchain,
}:

pkgs.mkShell {
  packages = toolchain ++ [
    pkgs.rust-analyzer

    # Android/Java tooling
    pkgs.jdk17
    pkgs.gradle
    android.sdk

    # APK tools
    pkgs.apktool
    pkgs.apksigner
  ];

  ANDROID_SDK_ROOT = android.sdkRoot;
  ANDROID_NDK_ROOT = android.ndkRoot;
  CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "${android.ndkToolchain}/aarch64-linux-android21-clang";
  CC_aarch64_linux_android = "${android.ndkToolchain}/aarch64-linux-android21-clang";
  AR_aarch64_linux_android = "${android.ndkToolchain}/llvm-ar";
  JAVA_HOME = "${pkgs.jdk17}";
  BAKSMALI_JAR = "${android.baksmaliJar}";
  SMALI_JAR = "${android.smaliJar}";

  shellHook = /* sh */ ''
    echo "PushCompat Development Shell"
    echo ""
    echo "Build commands:"
    echo "  nix build .#pushcompat-bridge   - Build bridge server"
    echo "  nix build .#pushcompat-patcher  - Build APK patcher"
    echo "  nix build .#pushcompat-shim     - Build shim DEX (with kotlin patching)"
    echo ""
    echo "Patch GitHub APK:"
    echo "  nix run .#patch-github         - Build shim + patch APK"
    echo "  nix run .#patch-github-install - Build, patch, and install"
    echo "  BRIDGE_URL=... nix run .#patch-github  - Override bridge URL"
    echo ""
    echo "Update shim deps (after changing shim/build.gradle.kts):"
    echo "  nix run .#update-shim-deps"
    echo ""
    echo "Manual tools:"
    echo "  java -jar \$BAKSMALI_JAR d <dex> -o <out>  - Disassemble DEX"
    echo "  java -jar \$SMALI_JAR a <dir> -o <dex>     - Assemble smali to DEX"
  '';
}
