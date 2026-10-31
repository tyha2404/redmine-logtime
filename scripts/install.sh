#!/usr/bin/env bash
# Build Redmine Logtime from source and install it (macOS or Ubuntu/Debian).
#
#   ./scripts/install.sh              build and install
#   ./scripts/install.sh --build-only build the installer, do not install
#   ./scripts/install.sh --uninstall  remove the app (keeps settings and API key)
set -euo pipefail
cd "$(dirname "$0")/.."

APP_NAME="Redmine Logtime"
DEB_PACKAGE="redmine-logtime"
OS="$(uname -s)"
MODE="${1:-install}"

info() { printf '\033[1;34m==>\033[0m \033[1m%s\033[0m\n' "$*"; }
warn() { printf '\033[33m%s\033[0m\n' "$*"; }
die() { printf '\033[31mLỗi:\033[0m %s\n' "$*" >&2; exit 1; }

mac_app_dir() {
  if [ -w /Applications ]; then echo /Applications; else mkdir -p "$HOME/Applications" && echo "$HOME/Applications"; fi
}

uninstall() {
  case "$OS" in
    Darwin)
      for dir in /Applications "$HOME/Applications"; do
        if [ -d "$dir/$APP_NAME.app" ]; then
          osascript -e "quit app \"$APP_NAME\"" >/dev/null 2>&1 || true
          rm -rf "$dir/$APP_NAME.app"
          info "Đã gỡ $dir/$APP_NAME.app"
        fi
      done ;;
    Linux)
      sudo apt-get remove -y "$DEB_PACKAGE" && info "Đã gỡ gói $DEB_PACKAGE" ;;
  esac
  echo "Cài đặt và API key vẫn được giữ, có thể xoá tay nếu muốn:"
  [ "$OS" = Darwin ] && echo "  ~/Library/Application Support/app.redmine-logtime.desktop" \
                     || echo "  ~/.config/app.redmine-logtime.desktop  ~/.local/share/app.redmine-logtime.desktop"
}

ensure_rust() {
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
  if ! command -v cargo >/dev/null 2>&1; then
    info "Cài Rust (rustup)"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    . "$HOME/.cargo/env"
  fi
}

ensure_node() {
  command -v npm >/dev/null 2>&1 || die "Cần Node.js 20 trở lên. macOS: brew install node. Ubuntu: https://nodejs.org (hoặc nvm)."
  local major
  major="$(node -p 'process.versions.node.split(".")[0]')"
  [ "$major" -ge 20 ] || die "Node.js $major quá cũ, cần 20 trở lên."
}

ensure_linux_deps() {
  command -v apt-get >/dev/null 2>&1 || die "Script hỗ trợ Ubuntu/Debian (apt). Với distro khác, cài các gói build của Tauri rồi chạy: npm run tauri build"
  local pkgs=(build-essential pkg-config curl wget file libxdo-dev libssl-dev
              libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev)
  local missing=()
  for p in "${pkgs[@]}"; do dpkg -s "$p" >/dev/null 2>&1 || missing+=("$p"); done
  if [ "${#missing[@]}" -gt 0 ]; then
    info "Cài gói hệ thống cần để build: ${missing[*]}"
    sudo apt-get update
    sudo apt-get install -y "${missing[@]}"
  fi
}

build() {
  ensure_node
  [ "$OS" = Linux ] && ensure_linux_deps
  ensure_rust
  info "Cài thư viện frontend"
  if [ -f package-lock.json ]; then npm ci --no-audit --no-fund; else npm install --no-audit --no-fund; fi
  info "Build bản release (lần đầu mất vài phút)"
  case "$OS" in
    Darwin) npx tauri build --bundles app ;;
    Linux)  npx tauri build --bundles deb ;;
    *) die "Chưa hỗ trợ hệ điều hành $OS" ;;
  esac
}

install_app() {
  case "$OS" in
    Darwin)
      local src="target/release/bundle/macos/$APP_NAME.app" dest
      [ -d "$src" ] || die "Không thấy $src"
      dest="$(mac_app_dir)"
      osascript -e "quit app \"$APP_NAME\"" >/dev/null 2>&1 || true
      rm -rf "$dest/$APP_NAME.app"
      ditto "$src" "$dest/$APP_NAME.app"
      xattr -dr com.apple.quarantine "$dest/$APP_NAME.app" 2>/dev/null || true
      info "Đã cài $dest/$APP_NAME.app"
      echo "Mở bằng Launchpad/Spotlight, hoặc: open \"$dest/$APP_NAME.app\"" ;;
    Linux)
      local deb
      deb="$(ls -t target/release/bundle/deb/*.deb 2>/dev/null | head -1)"
      [ -n "$deb" ] || die "Không thấy file .deb trong target/release/bundle/deb"
      sudo apt-get install -y "./$deb"
      info "Đã cài $(basename "$deb")"
      echo "Mở từ menu ứng dụng, hoặc chạy: $DEB_PACKAGE" ;;
  esac
}

check_ai_tools() {
  local ok=1
  command -v claude >/dev/null 2>&1 || { warn "Chưa có Claude CLI (cần cho tính năng AI): curl -fsSL https://claude.ai/install.sh | bash"; ok=0; }
  command -v uvx >/dev/null 2>&1 || { warn "Chưa có uv (để chạy MCP Redmine): curl -LsSf https://astral.sh/uv/install.sh | sh"; ok=0; }
  [ "$ok" = 1 ] && echo "Claude CLI và uv đã sẵn sàng cho tính năng AI."
  return 0
}

case "$MODE" in
  --uninstall) uninstall ;;
  --build-only) build; info "Bản build nằm trong target/release/bundle/" ;;
  install|--install) build; install_app; check_ai_tools ;;
  -h|--help) sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//' ;;
  *) die "Tham số không hợp lệ: $MODE (xem --help)" ;;
esac
