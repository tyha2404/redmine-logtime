#!/usr/bin/env bash
# Đổi phiên bản ở mọi nơi, commit và tạo tag. Sau đó: git push --follow-tags
#   ./scripts/release.sh 0.2.0
set -euo pipefail
cd "$(dirname "$0")/.."

die() { printf '\033[31mLỗi:\033[0m %s\n' "$*" >&2; exit 1; }

VERSION="${1:-}"
VERSION="${VERSION#v}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "Dùng: ./scripts/release.sh X.Y.Z (ví dụ 0.2.0)"
[ -z "$(git status --porcelain)" ] || die "Còn thay đổi chưa commit. Commit hoặc stash trước."
git rev-parse "v$VERSION" >/dev/null 2>&1 && die "Tag v$VERSION đã có."

node -e '
  const fs = require("fs");
  const v = process.argv[1];
  for (const f of ["package.json", "src-tauri/tauri.conf.json"]) {
    const j = JSON.parse(fs.readFileSync(f, "utf8"));
    j.version = v;
    fs.writeFileSync(f, JSON.stringify(j, null, 2) + "\n");
  }
  const lock = JSON.parse(fs.readFileSync("package-lock.json", "utf8"));
  lock.version = v;
  if (lock.packages && lock.packages[""]) lock.packages[""].version = v;
  fs.writeFileSync("package-lock.json", JSON.stringify(lock, null, 2) + "\n");
' "$VERSION"
sed -i.bak "/^\[workspace.package\]/,/^\[/s/^version = \".*\"$/version = \"$VERSION\"/" Cargo.toml && rm Cargo.toml.bak

# Cargo.lock ghi phiên bản của các crate trong workspace.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
if command -v cargo >/dev/null; then cargo metadata --format-version 1 >/dev/null; fi

scripts/ci/check-version.sh "v$VERSION"
git add package.json package-lock.json src-tauri/tauri.conf.json Cargo.toml Cargo.lock
git commit -q -m "chore: release v$VERSION"
git tag -a "v$VERSION" -m "Redmine Logtime $VERSION"
echo "Đã tạo commit và tag v$VERSION. Đẩy lên GitHub để Actions build và phát hành:"
echo "  git push --follow-tags"
