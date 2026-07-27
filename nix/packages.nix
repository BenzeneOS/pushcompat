{
  android,
  lib,
  pkgs,
  rustPlatform,
}:

let
  rustSrc = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.lock
      ../Cargo.toml
      ../crates
    ];
  };
in
rec {
  default = pushcompat-bridge;

  pushcompat-bridge = rustPlatform.buildRustPackage {
    pname = "pushcompat-bridge";
    version = "0.1.0";
    src = rustSrc;

    cargoLock.lockFile = ../Cargo.lock;
    buildAndTestSubdir = "crates/bridge";

    meta = {
      description = "FCM to UnifiedPush relay server";
      mainProgram = "pushcompat-bridge";
    };
  };

  pushcompat-patcher = rustPlatform.buildRustPackage {
    pname = "pushcompat-patcher";
    version = "0.1.0";
    src = rustSrc;

    cargoLock.lockFile = ../Cargo.lock;
    buildAndTestSubdir = "crates/patcher";

    nativeBuildInputs = [ pkgs.makeWrapper ];

    postInstall = ''
      wrapProgram $out/bin/pushcompat-patcher \
        --set-default PUSHCOMPAT_SHIM_DEX ${pushcompat-shim}/pushcompat-shim.dex
    '';

    meta = {
      description = "APK patcher for PushCompat";
      mainProgram = "pushcompat-patcher";
    };
  };

  pushcompat-shim = pkgs.stdenv.mkDerivation (finalAttrs: {
    pname = "pushcompat-shim";
    version = "0.1.0";
    src = ../shim;

    nativeBuildInputs = [
      pkgs.gradle
      pkgs.jdk17
      pkgs.unzip
    ];

    # Pre-fetch Gradle dependencies (run `nix run .#update-shim-deps` to update)
    mitmCache = pkgs.gradle.fetchDeps {
      pkg = finalAttrs.finalPackage;
      data = ../shim/deps.json;
    };

    __darwinAllowLocalNetworking = true;

    ANDROID_SDK_ROOT = android.sdkRoot;

    gradleFlags = [
      "-Dorg.gradle.java.home=${pkgs.jdk17}"
      "-Dorg.gradle.project.android.aapt2FromMavenOverride=${android.buildTools}/aapt2"
    ];

    gradleBuildTask = "assembleRelease";
    gradleUpdateTask = "nixDownloadDeps";

    doCheck = false;

    preBuild = ''
      export JAVA_TOOL_OPTIONS="-Duser.home=$NIX_BUILD_TOP/home"
      mkdir -p $NIX_BUILD_TOP/home/.android
      echo "sdk.dir=$ANDROID_SDK_ROOT" > local.properties
    '';

    # After Gradle build, convert AAR to DEX and patch out Kotlin intrinsics
    postBuild = ''
      echo "Converting AAR to DEX..."
      mkdir -p build/dex
      cd build/dex

      # Extract classes.jar from AAR
      ${pkgs.unzip}/bin/unzip -q ../outputs/aar/pushcompat-shim-release.aar classes.jar

      # Convert to DEX using d8
      ${android.buildTools}/d8 --release --output . classes.jar

      # Disassemble DEX to smali
      mkdir smali
      ${pkgs.jdk17}/bin/java -jar ${android.baksmaliJar} d classes.dex -o smali/

      # Patch out kotlin intrinsics
      ${android.patchKotlinIntrinsics} smali

      # Reassemble to DEX
      ${pkgs.jdk17}/bin/java -jar ${android.smaliJar} a smali/ -o pushcompat-shim.dex

      cd ../..
    '';

    installPhase = ''
      mkdir -p $out
      cp build/dex/pushcompat-shim.dex $out/
    '';

    meta.description = "PushCompat Kotlin shim compiled to DEX (kotlin-stdlib patched out)";
  });
}
