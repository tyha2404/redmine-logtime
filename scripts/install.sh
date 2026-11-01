#!/usr/bin/env bash
# Cài Redmine Logtime trên macOS hoặc Ubuntu/Debian.
#
# Không cần clone source (bản build sẵn từ GitHub Releases):
#   curl -fsSL https://raw.githubusercontent.com/tyha2404/redmine-logtime/HEAD/scripts/install.sh | bash
#   ... | bash -s -- --version 0.2.0
#   ... | bash -s -- --uninstall
#
# Từ thư mục source:
#   ./scripts/install.sh               build từ source rồi cài
#   ./scripts/install.sh --prebuilt    tải bản build sẵn thay vì build
#   ./scripts/install.sh --build-only  chỉ build, không cài
#
# Tuỳ chọn khác: --force (cài lại dù đã là bản mới nhất), --help.
# Bản fork: đặt LOGTIME_REPO=<chủ>/<repo> để tải từ repo GitHub khác.
#
# Viết cho bash 3.2 (bash mặc định của macOS): không dùng tính năng bash 4.
set -euo pipefail

REPO="${LOGTIME_REPO:-tyha2404/redmine-logtime}"

APP_NAME="Redmine Logtime"
PACKAGE="redmine-logtime"
APP_ID="app.redmine-logtime.desktop"
OS="$(uname -s)"

info() { printf '\033[1;34m==>\033[0m \033[1m%s\033[0m\n' "$*"; }
note() { printf '    %s\n' "$*"; }
warn() { printf '\033[33m%s\033[0m\n' "$*" >&2; }
die() { printf '\033[31mLỗi:\033[0m %s\n' "$*" >&2; exit 1; }

# --- Tham số ---------------------------------------------------------------

usage() {
  cat <<'USAGE'
Cài Redmine Logtime (macOS, Ubuntu/Debian).

  --version X.Y.Z   cài đúng phiên bản này (mặc định: bản mới nhất)
  --force           cài lại dù đã là bản mới nhất
  --uninstall       gỡ app (giữ cài đặt và API key)
  --prebuilt        trong thư mục source: tải bản build sẵn thay vì build
  --from-source     luôn build từ mã nguồn
  --build-only      trong thư mục source: chỉ build, không cài

Bản fork: đặt LOGTIME_REPO=<chủ>/<repo>.
USAGE
}

ACTION=install      # install | uninstall | build-only
SOURCE=auto         # auto | prebuilt | source
WANT_VERSION=""
FORCE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --uninstall) ACTION=uninstall ;;
    --build-only) ACTION=build-only; SOURCE=source ;;
    --prebuilt) SOURCE=prebuilt ;;
    --from-source) SOURCE=source ;;
    --version) [ $# -ge 2 ] || die "--version cần một số phiên bản, ví dụ 0.2.0"; WANT_VERSION="${2#v}"; shift ;;
    --version=*) WANT_VERSION="${1#--version=}"; WANT_VERSION="${WANT_VERSION#v}" ;;
    --force) FORCE=1 ;;
    -h|--help) usage; exit 0 ;;
    *) die "Tham số không hợp lệ: $1 (xem --help)" ;;
  esac
  shift
done

# Chạy từ thư mục source khi file script nằm trong repo; còn chạy qua
# `curl | bash` thì BASH_SOURCE rỗng.
SRC_DIR=""
script_path="${BASH_SOURCE[0]:-}"
if [ -n "$script_path" ] && [ -f "$script_path" ]; then
  candidate="$(cd "$(dirname "$script_path")/.." && pwd)"
  [ -f "$candidate/src-tauri/tauri.conf.json" ] && SRC_DIR="$candidate"
fi
if [ "$SOURCE" = auto ]; then
  if [ -n "$SRC_DIR" ]; then SOURCE=source; else SOURCE=prebuilt; fi
fi

# --- GitHub -------------------------------------------------------------------

gh_curl() { curl -fsSL --retry 2 "$@"; }

latest_version() {
  local tag
  tag="$(gh_curl "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)" || return 1
  [ -n "$tag" ] || return 1
  printf '%s' "${tag#v}"
}

asset_url() { printf 'https://github.com/%s/releases/download/v%s/%s' "$REPO" "$1" "$2"; }

installed_version() {
  case "$OS" in
    Darwin)
      local dir
      for dir in $(mac_app_dirs); do
        if [ -f "$dir/$APP_NAME.app/Contents/Info.plist" ]; then
          defaults read "$dir/$APP_NAME.app/Contents/Info" CFBundleShortVersionString 2>/dev/null && return 0
        fi
      done ;;
    Linux)
      dpkg-query -W -f='${Version}' "$PACKAGE" 2>/dev/null && return 0 ;;
  esac
  return 1
}

# --- Gỡ cài đặt ---------------------------------------------------------------

# LOGTIME_APP_DIR chọn thư mục cài khác trên macOS (mặc định /Applications).
mac_app_dir() {
  if [ -n "${LOGTIME_APP_DIR:-}" ]; then mkdir -p "$LOGTIME_APP_DIR" && echo "$LOGTIME_APP_DIR"; return; fi
  if [ -w /Applications ]; then echo /Applications; else mkdir -p "$HOME/Applications" && echo "$HOME/Applications"; fi
}
mac_app_dirs() { if [ -n "${LOGTIME_APP_DIR:-}" ]; then echo "$LOGTIME_APP_DIR"; else printf '%s\n' /Applications "$HOME/Applications"; fi; }

uninstall() {
  case "$OS" in
    Darwin)
      local dir found=0
      for dir in $(mac_app_dirs); do
        if [ -d "$dir/$APP_NAME.app" ]; then
          osascript -e "quit app \"$APP_NAME\"" >/dev/null 2>&1 || true
          rm -rf "$dir/$APP_NAME.app"
          info "Đã gỡ $dir/$APP_NAME.app"
          found=1
        fi
      done
      [ "$found" = 1 ] || note "Không thấy $APP_NAME.app trong /Applications hay ~/Applications." ;;
    Linux)
      if dpkg -s "$PACKAGE" >/dev/null 2>&1; then
        sudo apt-get remove -y "$PACKAGE" && info "Đã gỡ gói $PACKAGE"
      else
        note "Gói $PACKAGE chưa được cài."
      fi ;;
    *) die "Chưa hỗ trợ hệ điều hành $OS" ;;
  esac
  echo "Cài đặt và API key vẫn được giữ, có thể xoá tay nếu muốn:"
  if [ "$OS" = Darwin ]; then
    note "~/Library/Application Support/$APP_ID"
  else
    note "~/.config/$APP_ID  ~/.local/share/$APP_ID"
  fi
}

# --- Cài bản build sẵn --------------------------------------------------------

TMP=""
cleanup() { [ -n "$TMP" ] && rm -rf "$TMP"; return 0; }
trap cleanup EXIT

make_tmp() { [ -n "$TMP" ] || TMP="$(mktemp -d "${TMPDIR:-/tmp}/redmine-logtime.XXXXXX")"; }

prebuilt_asset() {
  case "$OS" in
    Darwin) printf 'Redmine-Logtime_%s_macos-universal.tar.gz' "$1" ;;
    Linux)
      command -v dpkg >/dev/null 2>&1 || return 1
      printf '%s_%s_%s.deb' "$PACKAGE" "$1" "$(dpkg --print-architecture)" ;;
    *) return 1 ;;
  esac
}

install_mac_bundle() {
  local app="$1" dest
  dest="$(mac_app_dir)"
  osascript -e "quit app \"$APP_NAME\"" >/dev/null 2>&1 || true
  rm -rf "$dest/$APP_NAME.app"
  ditto "$app" "$dest/$APP_NAME.app"
  xattr -dr com.apple.quarantine "$dest/$APP_NAME.app" 2>/dev/null || true
  info "Đã cài $dest/$APP_NAME.app"
  note "Mở bằng Launchpad/Spotlight, hoặc: open \"$dest/$APP_NAME.app\""
}

install_deb() {
  local deb="$1"
  case "$deb" in /*) ;; *) deb="./$deb" ;; esac
  command -v apt-get >/dev/null 2>&1 || die "Cần apt-get để cài gói .deb."
  sudo apt-get install -y "$deb"
  info "Đã cài $(basename "$deb")"
  note "Mở từ menu ứng dụng, hoặc chạy: $PACKAGE"
}

# Trả về 0 nếu đã cài xong, 1 nếu không có bản build sẵn cho máy này.
install_prebuilt() {
  local version="$1" asset file
  asset="$(prebuilt_asset "$version")" || return 1
  make_tmp
  file="$TMP/$asset"
  info "Tải $asset"
  gh_curl -o "$file" "$(asset_url "$version" "$asset")" 2>/dev/null || return 1
  case "$OS" in
    Darwin)
      tar -xzf "$file" -C "$TMP"
      [ -d "$TMP/$APP_NAME.app" ] || die "File tải về không chứa $APP_NAME.app"
      install_mac_bundle "$TMP/$APP_NAME.app" ;;
    Linux) install_deb "$file" ;;
  esac
}

# --- Build từ source ------------------------------------------------------------

ensure_rust() {
  if [ -f "$HOME/.cargo/env" ]; then . "$HOME/.cargo/env"; fi
  if ! command -v cargo >/dev/null 2>&1; then
    info "Cài Rust (rustup)"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    . "$HOME/.cargo/env"
  fi
}

ensure_node() {
  command -v npm >/dev/null 2>&1 || die "Cần Node.js 20 trở lên để build. macOS: brew install node. Ubuntu: https://nodejs.org (hoặc nvm)."
  local major
  major="$(node -p 'process.versions.node.split(".")[0]')"
  [ "$major" -ge 20 ] || die "Node.js $major quá cũ, cần 20 trở lên."
}

ensure_mac_tools() {
  xcode-select -p >/dev/null 2>&1 || die "Cần Xcode Command Line Tools để build: xcode-select --install"
}

ensure_linux_deps() {
  command -v apt-get >/dev/null 2>&1 || die "Script hỗ trợ Ubuntu/Debian (apt). Với distro khác, cài các gói build của Tauri rồi chạy: npm run tauri build"
  local pkgs="build-essential pkg-config curl wget file libxdo-dev libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev"
  local missing="" p
  for p in $pkgs; do dpkg -s "$p" >/dev/null 2>&1 || missing="$missing $p"; done
  if [ -n "$missing" ]; then
    info "Cài gói hệ thống cần để build:$missing"
    sudo apt-get update
    # shellcheck disable=SC2086
    sudo apt-get install -y $missing
  fi
}

build_in() {
  local dir="$1"
  case "$OS" in
    Darwin) ensure_mac_tools ;;
    Linux) ensure_linux_deps ;;
    *) die "Chưa hỗ trợ hệ điều hành $OS" ;;
  esac
  ensure_node
  ensure_rust
  cd "$dir"
  info "Cài thư viện frontend"
  if [ -f package-lock.json ]; then npm ci --no-audit --no-fund; else npm install --no-audit --no-fund; fi
  info "Build bản release (lần đầu mất vài phút)"
  case "$OS" in
    Darwin) npx tauri build --bundles app ;;
    Linux) npx tauri build --bundles deb ;;
  esac
}

install_built() {
  local dir="$1"
  case "$OS" in
    Darwin)
      local app="$dir/target/release/bundle/macos/$APP_NAME.app"
      [ -d "$app" ] || die "Không thấy $app"
      install_mac_bundle "$app" ;;
    Linux)
      local deb
      deb="$(ls -t "$dir"/target/release/bundle/deb/*.deb 2>/dev/null | head -1)"
      [ -n "$deb" ] || die "Không thấy file .deb trong target/release/bundle/deb"
      install_deb "$deb" ;;
  esac
}

# Không có thư mục source: tải mã nguồn của phiên bản (không cần git) vào
# $TMP/src. Gọi trực tiếp, không qua $(...), để $TMP còn được dọn khi thoát.
fetch_source() {
  local version="$1" ref="HEAD"
  make_tmp
  [ -n "$version" ] && ref="refs/tags/v$version"
  info "Tải mã nguồn ${version:+v$version }từ GitHub ($REPO)"
  gh_curl -o "$TMP/source.tar.gz" "https://github.com/$REPO/archive/$ref.tar.gz" \
    || die "Không tải được mã nguồn từ https://github.com/$REPO."
  mkdir -p "$TMP/src"
  tar -xzf "$TMP/source.tar.gz" -C "$TMP/src" --strip-components=1
}

check_ai_tools() {
  local ok=1
  command -v claude >/dev/null 2>&1 || { warn "Chưa có Claude CLI (cần cho tính năng AI): curl -fsSL https://claude.ai/install.sh | bash"; ok=0; }
  command -v uvx >/dev/null 2>&1 || { warn "Chưa có uv (để chạy MCP Redmine): curl -LsSf https://astral.sh/uv/install.sh | sh"; ok=0; }
  [ "$ok" = 1 ] && note "Claude CLI và uv đã sẵn sàng cho tính năng AI."
  return 0
}

# --- Chạy ------------------------------------------------------------------------

case "$OS" in Darwin|Linux) ;; *) die "Chưa hỗ trợ hệ điều hành $OS (chỉ macOS và Ubuntu/Debian)." ;; esac

case "$ACTION" in
  uninstall) uninstall; exit 0 ;;
  build-only)
    [ -n "$SRC_DIR" ] || die "--build-only cần chạy trong thư mục source."
    build_in "$SRC_DIR"
    info "Bản build nằm trong target/release/bundle/"
    exit 0 ;;
esac

current="$(installed_version || true)"

if [ "$SOURCE" = source ] && [ -n "$SRC_DIR" ]; then
  build_in "$SRC_DIR"
  install_built "$SRC_DIR"
  check_ai_tools
  exit 0
fi

version="$WANT_VERSION"
if [ -z "$version" ]; then
  version="$(latest_version || true)"
fi

if [ -n "$version" ]; then
  if [ -n "$current" ] && [ "$current" = "$version" ] && [ "$FORCE" = 0 ]; then
    info "$APP_NAME $version đã được cài, là bản mới nhất."
    note "Cài lại: thêm --force"
    exit 0
  fi
  if [ -n "$current" ]; then info "Cập nhật $APP_NAME $current → $version"; else info "Cài $APP_NAME $version"; fi
  if install_prebuilt "$version"; then
    check_ai_tools
    exit 0
  fi
  warn "Chưa có bản build sẵn cho máy này ($OS $(uname -m)) trong v$version. Chuyển sang build từ mã nguồn."
else
  warn "Chưa có release nào trên GitHub. Build từ mã nguồn mới nhất của nhánh mặc định."
fi

fetch_source "$version"
build_in "$TMP/src"
install_built "$TMP/src"
check_ai_tools
