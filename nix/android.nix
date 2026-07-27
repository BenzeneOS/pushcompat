{ pkgs }:

let
  platformVersion = "36";
  buildToolsVersion = "36.0.0";
  composition = pkgs.androidenv.composeAndroidPackages {
    platformVersions = [ platformVersion ];
    buildToolsVersions = [ buildToolsVersion ];
    includeNDK = true;
  };
  sdk = composition.androidsdk;
  sdkRoot = "${sdk}/libexec/android-sdk";
  ndkRoot = "${sdkRoot}/ndk-bundle";
  ndkHostTag = if pkgs.stdenv.hostPlatform.isDarwin then "darwin-x86_64" else "linux-x86_64";
  ndkToolchain = "${ndkRoot}/toolchains/llvm/prebuilt/${ndkHostTag}/bin";
  buildTools = "${sdkRoot}/build-tools/${buildToolsVersion}";

  smaliVersion = "2.5.2";
  baksmaliJar = pkgs.fetchurl {
    url = "https://bitbucket.org/JesusFreke/smali/downloads/baksmali-${smaliVersion}.jar";
    sha256 = "sha256-0xFiSMzk+C7Fox63+V7nXa/0Ld9u7Qq1c5c9xT+60uU=";
  };
  smaliJar = pkgs.fetchurl {
    url = "https://bitbucket.org/JesusFreke/smali/downloads/smali-${smaliVersion}.jar";
    sha256 = "sha256-lUQplXixb3cdiqjq7+DTcYygNHjBbzw1by/PE2a/sRY=";
  };
in
{
  inherit
    baksmaliJar
    buildTools
    composition
    ndkHostTag
    ndkRoot
    ndkToolchain
    sdk
    sdkRoot
    smaliJar
    ;

  baksmali = pkgs.writeShellScriptBin "baksmali" ''
    exec ${pkgs.jdk17}/bin/java -jar ${baksmaliJar} "$@"
  '';

  smali = pkgs.writeShellScriptBin "smali" ''
    exec ${pkgs.jdk17}/bin/java -jar ${smaliJar} "$@"
  '';

  patchKotlinIntrinsics = pkgs.writeShellScript "patch-kotlin-intrinsics" ''
    set -euo pipefail
    SMALI_DIR="$1"

    # Find all smali files and patch Kotlin stdlib references
    find "$SMALI_DIR" -name "*.smali" -exec sed -i \
      -e 's|Lkotlin/text/Charsets;->UTF_8:Ljava/nio/charset/Charset;|Ljava/nio/charset/StandardCharsets;->UTF_8:Ljava/nio/charset/Charset;|g' \
      -e 's|Lkotlin/jvm/internal/Intrinsics;->areEqual(Ljava/lang/Object;Ljava/lang/Object;)Z|Ljava/util/Objects;->equals(Ljava/lang/Object;Ljava/lang/Object;)Z|g' \
      {} +

    # Remove checkNotNull calls - need regex for register names
    for f in $(find "$SMALI_DIR" -name "*.smali"); do
      sed -i -E \
        -e 's|invoke-static \{v[0-9]+\}, Lkotlin/jvm/internal/Intrinsics;->checkNotNull\(Ljava/lang/Object;\)V|nop|g' \
        -e 's|invoke-static \{v[0-9]+, v[0-9]+\}, Lkotlin/jvm/internal/Intrinsics;->checkNotNull\(Ljava/lang/Object;Ljava/lang/String;\)V|nop|g' \
        -e 's|invoke-static \{v[0-9]+, v[0-9]+\}, Lkotlin/jvm/internal/Intrinsics;->checkNotNullExpressionValue\(Ljava/lang/Object;Ljava/lang/String;\)V|nop|g' \
        -e 's|invoke-static \{p[0-9]+\}, Lkotlin/jvm/internal/Intrinsics;->checkNotNull\(Ljava/lang/Object;\)V|nop|g' \
        -e 's|invoke-static \{p[0-9]+, v[0-9]+\}, Lkotlin/jvm/internal/Intrinsics;->checkNotNull\(Ljava/lang/Object;Ljava/lang/String;\)V|nop|g' \
        -e 's|invoke-static \{p[0-9]+, v[0-9]+\}, Lkotlin/jvm/internal/Intrinsics;->checkNotNullExpressionValue\(Ljava/lang/Object;Ljava/lang/String;\)V|nop|g' \
        "$f"
    done
  '';
}
