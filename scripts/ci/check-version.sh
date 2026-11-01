#!/bin/sh
# Tag vX.Y.Z phải khớp phiên bản trong tauri.conf.json, Cargo.toml và package.json.
set -eu
cd "$(dirname "$0")/../.."
want="${1#v}"
tauri="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' src-tauri/tauri.conf.json | head -1)"
cargo="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
npm="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' package.json | head -1)"
ok=1
for pair in "src-tauri/tauri.conf.json:$tauri" "Cargo.toml:$cargo" "package.json:$npm"; do
  if [ "${pair#*:}" != "$want" ]; then
    echo "Phiên bản trong ${pair%%:*} là '${pair#*:}', tag là v$want." >&2
    ok=0
  fi
done
[ "$ok" = 1 ] || { echo "Dùng 'make release VERSION=$want' để đổi phiên bản đồng bộ." >&2; exit 1; }
echo "Phiên bản $want khớp tag."
