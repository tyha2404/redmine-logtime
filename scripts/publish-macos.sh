#!/usr/bin/env bash
# Build bản macOS universal (Apple Silicon + Intel) và đưa lên GitLab Package
# Registry cho một phiên bản, để lệnh cài một dòng tải được trên máy Mac.
#
# Dùng khi GitLab không có runner macOS:
#   GITLAB_TOKEN=<token quyền api> ./scripts/publish-macos.sh [vX.Y.Z]
#   ./scripts/publish-macos.sh --no-upload       chỉ build file .tar.gz
#
# Mặc định lấy phiên bản trong tauri.conf.json và GitLab từ `git remote origin`.
# Ghi đè bằng LOGTIME_GITLAB_URL và LOGTIME_GITLAB_PROJECT nếu cần.
set -euo pipefail
cd "$(dirname "$0")/.."

info() { printf '\033[1;34m==>\033[0m \033[1m%s\033[0m\n' "$*"; }
die() { printf '\033[31mLỗi:\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || die "Chỉ build được bản macOS trên máy Mac."

UPLOAD=1
TAG=""
for arg in "$@"; do
  case "$arg" in
    --no-upload) UPLOAD=0 ;;
    v*) TAG="$arg" ;;
    *) die "Tham số không hợp lệ: $arg" ;;
  esac
done
VERSION="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' src-tauri/tauri.conf.json | head -1)"
[ -z "$TAG" ] || [ "${TAG#v}" = "$VERSION" ] || die "Tag $TAG khác phiên bản $VERSION trong tauri.conf.json. Checkout đúng tag trước."

APP_NAME="Redmine Logtime"
ASSET="Redmine-Logtime_${VERSION}_macos-universal.tar.gz"
OUT="target/universal-apple-darwin/release/bundle/macos"

# GitLab: CI dùng biến có sẵn, máy cá nhân lấy từ git remote.
if [ -n "${CI_UPLOAD:-}" ]; then
  REGISTRY="${PACKAGE_REGISTRY:?}"
  AUTH="JOB-TOKEN: ${CI_JOB_TOKEN:?}"
else
  url="${LOGTIME_GITLAB_URL:-}"
  project="${LOGTIME_GITLAB_PROJECT:-}"
  if [ -z "$url" ] || [ -z "$project" ]; then
    remote="$(git remote get-url origin 2>/dev/null || true)"
    remote="${remote%.git}"
    case "$remote" in
      https://*) rest="${remote#https://}"; rest="${rest#*@}"; url="https://${rest%%/*}"; project="${rest#*/}" ;;
      ssh://*) rest="${remote#ssh://}"; rest="${rest#*@}"; h="${rest%%/*}"; url="https://${h%%:*}"; project="${rest#*/}" ;;
      *@*:*) rest="${remote#*@}"; url="https://${rest%%:*}"; project="${rest#*:}" ;;
    esac
  fi
  API="${url%/}/api/v4/projects/${project//\//%2F}"
  REGISTRY="$API/packages/generic/redmine-logtime"
  AUTH="PRIVATE-TOKEN: ${GITLAB_TOKEN:-}"
  if [ "$UPLOAD" = 1 ]; then
    [ -n "$project" ] || die "Chưa có git remote origin. Đặt LOGTIME_GITLAB_URL và LOGTIME_GITLAB_PROJECT."
    [ -n "${GITLAB_TOKEN:-}" ] || die "Đặt GITLAB_TOKEN (Personal Access Token quyền api) để tải bản build lên GitLab."
  fi
fi

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

[ "$UPLOAD" = 1 ] || exit 0

info "Tải lên GitLab Package Registry"
curl -fsS --retry 3 -H "$AUTH" --upload-file "target/publish/$ASSET" "$REGISTRY/$VERSION/$ASSET" >/dev/null

# Release đã được CI tạo thì gắn thêm link; chưa có thì job release sẽ tự thấy file.
if [ -z "${CI_UPLOAD:-}" ]; then
  link="{\"name\":\"macOS (Apple Silicon + Intel)\",\"url\":\"$REGISTRY/$VERSION/$ASSET\",\"link_type\":\"package\",\"direct_asset_path\":\"/$ASSET\"}"
  if curl -fsS -o /dev/null -H "$AUTH" -H "Content-Type: application/json" -X POST --data "$link" \
    "$API/releases/v$VERSION/assets/links" 2>/dev/null; then
    info "Đã gắn vào release v$VERSION"
  else
    echo "Chưa gắn được vào release v$VERSION (chưa tạo, hoặc đã có link). Lệnh cài vẫn tải được bản macOS."
  fi
fi
info "Xong. Máy Mac giờ cài được bằng lệnh một dòng."
