#!/usr/bin/env bash
# Build bản macOS universal (Apple Silicon + Intel) và đóng gói thành
# target/publish/Redmine-Logtime_<version>_macos-universal.tar.gz, đúng tên
# install.sh tải từ GitHub Releases. Chạy trong GitHub Actions (release.yml)
# hoặc trên máy Mac: ./scripts/package-macos.sh
set -euo pipefail
cd "$(dirname "$0")/.."

info() { printf '\033[1;34m==>\033[0m \033[1m%s\033[0m\n' "$*"; }
die() { printf '\033[31mLỗi:\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || die "Chỉ build được bản macOS trên máy Mac."

VERSION="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' src-tauri/tauri.conf.json | head -1)"
APP_NAME="Redmine Logtime"
ASSET="Redmine-Logtime_${VERSION}_macos-universal.tar.gz"
OUT="target/universal-apple-darwin/release/bundle/macos"

info "Chuẩn bị Rust cho cả Apple Silicon và Intel"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
command -v rustup >/dev/null || die "Cần rustup: curl https://sh.rustup.rs -sSf | sh"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null

info "Build $APP_NAME $VERSION (universal)"
npm ci --no-audit --no-fund
npx tauri build --target universal-apple-darwin --bundles app

[ -d "$OUT/$APP_NAME.app" ] || die "Không thấy $OUT/$APP_NAME.app"
mkdir -p target/publish
tar -czf "target/publish/$ASSET" -C "$OUT" "$APP_NAME.app"
info "Đã đóng gói target/publish/$ASSET"
