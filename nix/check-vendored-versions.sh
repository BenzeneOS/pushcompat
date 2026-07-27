#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)

if [[ -n ${ANDROID_BUILD_TOP:-} ]]; then
    android_build_top=$ANDROID_BUILD_TOP
elif [[ $repo_root == */external/rust/fcm2up ]]; then
    android_build_top=$(cd -- "$repo_root/../../.." && pwd)
else
    echo "error: set ANDROID_BUILD_TOP when running outside the Android tree" >&2
    exit 2
fi

android_crates=${ANDROID_CRATES_IO_ROOT:-"$android_build_top/external/rust/android-crates-io"}
benzeneos_crates=${BENZENEOS_CRATES_ROOT:-"$android_build_top/external/rust/benzeneos-crates"}
crate_roots=(
    "$android_crates/crates"
    "$android_crates/extra_versions/crates"
    "$benzeneos_crates/crates"
    "$benzeneos_crates/extra_versions/crates"
    "$benzeneos_crates/feature_variants/crates"
)

for crate_root in "${crate_roots[@]}"; do
    if [[ ! -d $crate_root ]]; then
        echo "error: vendored crate root does not exist: $crate_root" >&2
        exit 2
    fi
done

crate_version() {
    sed -nE 's/^version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$1/Cargo.toml" |
        head -n 1
}

semver_compatible() {
    local requested=${1%%[-+]*}
    local vendored=${2%%[-+]*}
    local requested_major requested_minor _
    local vendored_major vendored_minor

    IFS=. read -r requested_major requested_minor _ <<<"$requested"
    IFS=. read -r vendored_major vendored_minor _ <<<"$vendored"

    [[ $requested_major == "$vendored_major" ]] &&
        { [[ $requested_major != 0 ]] || [[ $requested_minor == "$vendored_minor" ]]; }
}

failed=0
while read -r name version _; do
    [[ $name == pushcompat-listener ]] && continue
    version=${version#v}
    matched=false
    candidates=()

    for crate_root in "${crate_roots[@]}"; do
        crate_dir=$crate_root/$name
        [[ -d $crate_dir ]] || continue
        vendored_version=$(crate_version "$crate_dir")
        candidates+=("$vendored_version")
        if semver_compatible "$version" "$vendored_version"; then
            matched=true
        fi
    done

    if [[ $matched == false ]]; then
        if ((${#candidates[@]} == 0)); then
            echo "MISSING $name requested=$version" >&2
        else
            echo "INCOMPATIBLE $name requested=$version vendored=${candidates[*]}" >&2
        fi
        failed=1
    fi
done < <(
    cargo tree \
        --manifest-path "$repo_root/Cargo.toml" \
        -p pushcompat-listener \
        -e no-dev,no-build \
        --prefix none \
        --format '{p}' |
        sort -u
)

for crate_dir in "$benzeneos_crates"/feature_variants/crates/*; do
    [[ -d $crate_dir ]] || continue
    name=$(sed -nE 's/^name[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$crate_dir/Cargo.toml" |
        head -n 1)
    variant_version=$(crate_version "$crate_dir")
    aosp_dir=$android_crates/crates/$name

    if [[ ! -d $aosp_dir ]]; then
        echo "VARIANT_BASE_MISSING $name variant=$variant_version" >&2
        failed=1
        continue
    fi

    aosp_version=$(crate_version "$aosp_dir")
    if [[ $variant_version != "$aosp_version" ]]; then
        echo "VARIANT_DRIFT $name variant=$variant_version aosp=$aosp_version" >&2
        failed=1
    fi
done

if ((failed)); then
    exit 1
fi

echo "vendored listener dependencies are compatible; feature variants match AOSP exactly"
