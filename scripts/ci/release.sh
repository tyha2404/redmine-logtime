#!/bin/sh
# Chạy trong job `release` của GitLab CI (alpine + curl + jq): đăng install.sh,
# tạo GitLab Release trỏ tới các bản build trong Package Registry.
set -eu
cd "$(dirname "$0")/../.."

VERSION="${CI_COMMIT_TAG#v}"
AUTH="JOB-TOKEN: $CI_JOB_TOKEN"
INSTALL_URL="$PACKAGE_REGISTRY/latest/install.sh"
ONE_LINER="curl -fsSL $INSTALL_URL | bash"

# install.sh với địa chỉ GitLab của project này điền sẵn.
sed -e "s|__GITLAB_URL__|$CI_SERVER_URL|" -e "s|__GITLAB_PROJECT__|$CI_PROJECT_ID|" scripts/install.sh > install.sh
for v in "$VERSION" latest; do
  curl -fsS --retry 3 -H "$AUTH" --upload-file install.sh "$PACKAGE_REGISTRY/$v/install.sh" > /dev/null
done

# Các bản build đã có cho phiên bản này (macOS có thể được đẩy lên sau từ máy Mac).
links="[]"
for asset in \
  "${PACKAGE}_${VERSION}_amd64.deb|Ubuntu/Debian (amd64)" \
  "${PACKAGE}_${VERSION}_arm64.deb|Ubuntu/Debian (arm64)" \
  "Redmine-Logtime_${VERSION}_macos-universal.tar.gz|macOS (Apple Silicon + Intel)" \
  "install.sh|install.sh"; do
  file="${asset%%|*}"
  name="${asset#*|}"
  url="$PACKAGE_REGISTRY/$VERSION/$file"
  if curl -fsSI -H "$AUTH" "$url" > /dev/null 2>&1 || curl -fsS -r 0-0 -o /dev/null -H "$AUTH" "$url" 2>/dev/null; then
    echo "Có: $file"
    links="$(printf '%s' "$links" | jq --arg n "$name" --arg u "$url" --arg p "/$file" \
      '. + [{name: $n, url: $u, link_type: "package", direct_asset_path: $p}]')"
  else
    echo "Chưa có: $file"
  fi
done

description="$(cat <<MD
Cài hoặc cập nhật trên macOS và Ubuntu/Debian, không cần clone source:

\`\`\`bash
$ONE_LINER
\`\`\`

Project private: \`curl -fsSL -H "PRIVATE-TOKEN: <token>" $INSTALL_URL | GITLAB_TOKEN=<token> bash\`
MD
)"

body="$(jq -n --arg tag "$CI_COMMIT_TAG" --arg name "Redmine Logtime $VERSION" --arg desc "$description" \
  --argjson links "$links" '{tag_name: $tag, name: $name, description: $desc, assets: {links: $links}}')"

status="$(curl -sS -o /tmp/release.json -w '%{http_code}' -H "$AUTH" -H "Content-Type: application/json" \
  -X POST --data "$body" "$CI_API_V4_URL/projects/$CI_PROJECT_ID/releases")"
case "$status" in
  201) echo "Đã tạo release $CI_COMMIT_TAG." ;;
  409)
    echo "Release $CI_COMMIT_TAG đã có, bổ sung link còn thiếu."
    printf '%s' "$links" | jq -c '.[]' | while read -r link; do
      curl -sS -o /dev/null -H "$AUTH" -H "Content-Type: application/json" -X POST --data "$link" \
        "$CI_API_V4_URL/projects/$CI_PROJECT_ID/releases/$CI_COMMIT_TAG/assets/links" || true
    done ;;
  *) cat /tmp/release.json >&2; echo; echo "Tạo release lỗi (HTTP $status)." >&2; exit 1 ;;
esac

printf '\n\033[1mLệnh cài trên máy mới:\033[0m\n  %s\n\n' "$ONE_LINER"
