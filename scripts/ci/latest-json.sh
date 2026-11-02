#!/usr/bin/env bash
# Tạo latest.json cho cập nhật trong app (tauri-plugin-updater) từ các file
# đã ký trong thư mục phát hành. Không có chữ ký thì không tạo gì.
#   scripts/ci/latest-json.sh v0.2.0 owner/repo assets
# NOTES (tuỳ chọn): mục "Có gì mới" hiện trong hộp cập nhật của app.
set -euo pipefail

TAG="$1" REPO="$2" DIR="$3"
VERSION="${TAG#v}"
MAC="$DIR/Redmine-Logtime_${VERSION}_macos-universal.tar.gz"
DEB="$DIR/redmine-logtime_${VERSION}_amd64.deb"

if [ ! -s "$MAC.sig" ] && [ ! -s "$DEB.sig" ]; then
  echo "Không có file .sig, bỏ qua latest.json."
  exit 0
fi

python3 - "$TAG" "$REPO" "$MAC" "$DEB" > "$DIR/latest.json" <<'PY'
import datetime, json, os, sys

tag, repo, mac, deb = sys.argv[1:]
base = f"https://github.com/{repo}/releases/download/{tag}/"
platforms = {}

def add(path, *targets):
    sig = path + ".sig"
    if not os.path.exists(sig):
        return
    entry = {"url": base + os.path.basename(path), "signature": open(sig).read().strip()}
    for t in targets:
        platforms[t] = entry

# The updater looks for "{os}-{arch}-{bundle}" first, then "{os}-{arch}".
add(mac, "darwin-aarch64-app", "darwin-x86_64-app", "darwin-aarch64", "darwin-x86_64")
add(deb, "linux-x86_64-deb", "linux-x86_64")
json.dump(
    {
        "version": tag.lstrip("v"),
        "notes": os.environ.get("NOTES", "").strip(),
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
    },
    sys.stdout,
    indent=2,
)
PY
echo "Đã tạo $DIR/latest.json"
