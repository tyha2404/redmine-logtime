#!/usr/bin/env bash
# In lệnh cài một dòng cho project GitLab trong `git remote origin`.
# Link dùng tên project (nhom/du-an); CI in link theo ID số, cả hai đều chạy.
set -euo pipefail
cd "$(dirname "$0")/.."
remote="$(git remote get-url origin 2>/dev/null || true)"
[ -n "$remote" ] || { echo "Chưa có git remote origin. Thêm bằng: git remote add origin <url GitLab>" >&2; exit 1; }
remote="${remote%.git}"
case "$remote" in
  https://*) rest="${remote#https://}"; rest="${rest#*@}"; host="${rest%%/*}"; path="${rest#*/}" ;;
  ssh://*) rest="${remote#ssh://}"; rest="${rest#*@}"; host="${rest%%/*}"; host="${host%%:*}"; path="${rest#*/}" ;;
  *@*:*) rest="${remote#*@}"; host="${rest%%:*}"; path="${rest#*:}" ;;
  *) echo "Không đọc được remote: $remote" >&2; exit 1 ;;
esac
url="https://$host/api/v4/projects/${path//\//%2F}/packages/generic/redmine-logtime/latest/install.sh"
echo "curl -fsSL $url | bash"
echo
echo "Project private:"
echo "curl -fsSL -H \"PRIVATE-TOKEN: <token>\" $url | GITLAB_TOKEN=<token> bash"
