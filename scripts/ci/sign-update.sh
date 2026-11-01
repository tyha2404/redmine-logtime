#!/usr/bin/env bash
# Ký các file phát hành cho cập nhật trong app (tạo <file>.sig bên cạnh).
# Cần secret TAURI_SIGNING_PRIVATE_KEY (nội dung khoá, xem docs/development.md);
# thiếu thì bỏ qua: bản phát hành vẫn cài và cập nhật được bằng install.sh.
#   scripts/ci/sign-update.sh file1 [file2...]
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  echo "::notice::Chưa có TAURI_SIGNING_PRIVATE_KEY, bỏ qua chữ ký cập nhật trong app."
  exit 0
fi
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
for f in "$@"; do
  npx tauri signer sign "$f"
  [ -s "$f.sig" ] || { echo "Không tạo được $f.sig" >&2; exit 1; }
done
