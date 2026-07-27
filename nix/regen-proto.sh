#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
proto_dir="$repo_root/crates/listener/src/proto"
tool_root="$repo_root/target/proto-tools"
rustflags="${RUSTFLAGS:-}"
if [[ -n "$rustflags" ]]; then
  rustflags+=" "
fi

RUSTFLAGS="${rustflags}-A dangerous_implicit_autorefs" \
  cargo install pb-rs --version 0.10.0 --locked --root "$tool_root"

pb_rs="$tool_root/bin/pb-rs"
"$pb_rs" -s -D -I "$proto_dir" -o "$proto_dir/android_checkin.rs" "$proto_dir/android_checkin.proto"
"$pb_rs" -s -D -I "$proto_dir" -o "$proto_dir/checkin.rs" "$proto_dir/checkin.proto"
"$pb_rs" -s -D -I "$proto_dir" -o "$proto_dir/mcs.rs" "$proto_dir/mcs.proto"
rm -f "$proto_dir/mod.rs"

# Single-module output retains package qualifiers that do not match the listener's module layout.
sed -i 's/checkin_proto::DeviceType/DeviceType/g' "$proto_dir/android_checkin.rs"
sed -i 's|checkin_proto::AndroidCheckinProto|super::android_checkin::AndroidCheckinProto|g' "$proto_dir/checkin.rs"
sed -i '/#!\[allow(non_camel_case_types)\]/a #![allow(dead_code)]' "$proto_dir/mcs.rs"

for generated in android_checkin.rs checkin.rs mcs.rs; do
  sed -i '1a // Regenerate from the repository root: nix develop -c bash nix/regen-proto.sh' \
    "$proto_dir/$generated"
done
