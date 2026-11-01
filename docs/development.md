# Phát triển Redmine Logtime

Tài liệu cho người muốn chạy từ mã nguồn, đóng góp hoặc phát hành app. Hướng dẫn cho người dùng nằm ở [README](../README.md).

## Cấu trúc

```
crates/redmine-client   REST client cho Redmine (đọc, tạo task con, tạo/xoá logtime)
crates/logtime-core     Settings, tính trạng thái lịch tháng, nghỉ phép
crates/logtime-engine   Hàng chờ, duyệt, ghi theo lô (chống trùng), hoàn tác; SQLite
crates/ai-bridge        Gọi Claude CLI + mcp-redmine (chỉ đọc): phân tích cấu trúc Redmine, phân tích báo cáo
src-tauri/              App Tauri 2: commands, lưu settings và API key
src/                    Giao diện React
docker/                 Redmine local để test thao tác ghi
scripts/                Cài đặt, phát hành, CI
```

## Chạy dev

Yêu cầu: Rust (rustup), Node 20+. Trên Ubuntu cần thêm các gói build cho Tauri:
`sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev`.

```bash
npm install
npm run tauri dev      # hoặc: make dev
```

URL Redmine và API key nhập trong màn **Cài đặt**. API key được lưu trong `credentials.json` ở thư mục cấu hình của app, chỉ user hiện tại đọc được (quyền 0600, thư mục 0700):

- macOS: `~/Library/Application Support/app.redmine-logtime.desktop/`
- Ubuntu: `~/.config/app.redmine-logtime.desktop/`

Từ thư mục source cũng dùng được `make install` (build rồi cài), `./scripts/install.sh --prebuilt` (tải bản build sẵn), `make build`, `make uninstall`.

## Xem giao diện bằng dữ liệu mẫu

Không cần Redmine hay Tauri: chạy UI trong trình duyệt với dữ liệu tháng 10/2026 giả lập (`src/mock.ts`, không có trong bản build thật). Ảnh trong README được chụp từ chế độ này.

```bash
make mock          # = VITE_MOCK=1 npx vite --port 1430
# mở http://localhost:1430/?page=calendar  (review | leave | history | settings)
# màn chào lần đầu: http://localhost:1430/?setup=1
```

## Test

```bash
make test          # = cargo test --workspace && npx tsc --noEmit
```

## Redmine local

Mọi thao tác ghi nên thử trên Redmine chạy bằng Docker, không thử trên Redmine thật:

```bash
docker compose -f docker/docker-compose.yml up -d
# đợi ~30s cho Redmine khởi tạo DB
docker compose -f docker/docker-compose.yml exec redmine bin/rails runner /seed.rb
```

Seed in ra API key của user `dev1`. Trong app nhập URL `http://localhost:3000` và key đó. Đăng nhập web: `admin` / `admin`.

Thử engine ghi đầu-cuối (chỉ chạy với localhost): tạo task con, ghi logtime, chạy lại không trùng, hoàn tác:

```bash
REDMINE_API_KEY=... cargo run -p logtime-engine --example local_e2e
```

Kiểm tra nhanh client (chỉ đọc):

```bash
REDMINE_URL=http://localhost:3000 REDMINE_API_KEY=... \
  cargo run -p redmine-client --example smoke -- 2026-10-01 2026-10-31
```

Thử AI thật với Redmine local (tốn một ít lượt dùng Claude):

```bash
REDMINE_API_KEY=... cargo run -p ai-bridge --example local_ai
```

## Tính năng AI hoạt động thế nào

App gọi [Claude CLI](https://claude.ai/install.sh) (`claude -p`) với [mcp-redmine](https://github.com/runekaagaard/mcp-redmine) chạy qua `uvx`, cấu hình `REDMINE_READ_ONLY=1`: Claude chỉ đọc Redmine, mọi thao tác ghi do app làm sau khi người dùng duyệt.

- **Phân tích cấu trúc Redmine** tạo một hồ sơ (cách nhận biết task cha, tracker task con, quy ước đặt tên) và đưa hồ sơ đó cho Claude ở mỗi lần phân tích báo cáo.
- **Phân tích báo cáo** trả về danh sách việc kèm task cha đề xuất; app (không phải Claude) chia giờ theo bội số 0.5h cho đủ giờ còn trống mỗi ngày và bỏ qua việc đã log hoặc đang chờ ghi.

## Nghỉ phép

App hỗ trợ cách quản lý nghỉ phép: mỗi loại nghỉ có một project (ví dụ Annual Leave, Sick Leave, Unpaid Leave); đầu mỗi tháng HR tạo một task cha trong từng project ("Xin nghỉ phép Tháng 10.2026", "… Đột Xuất …", "… KL …"); mỗi ngày nghỉ là một issue con dưới task cha của tháng đó, log giờ vào issue con.

- **Cài đặt → Nghỉ phép → Tìm trên Redmine**: tìm các project có tên chứa "Leave"/"Nghỉ", activity cùng tên, và tên người dùng hay dùng trong tiêu đề (ví dụ "BE - Tên") từ các issue nghỉ cũ.
- Task cha của tháng được nhận ra qua tiêu đề có tháng ("10.2026", "T10.2026", "Tháng 10/2026"…).
- Lịch tính giờ log trong các project nghỉ là ngày nghỉ.

## Phát hành

GitHub Actions: `ci.yml` chạy `tsc`, `cargo test` mỗi lần push; `release.yml` chạy khi push tag `vX.Y.Z`, build `.deb` (Ubuntu 22.04, amd64) và `.app` universal (macOS), rồi tạo GitHub Release kèm `install.sh`.

```bash
make release VERSION=0.2.0     # đổi version ở mọi file, commit, tạo tag
git push --follow-tags
```

Phát hành đúng version đang có (không đổi gì để commit): `git tag -a v0.1.0 -m "Redmine Logtime 0.1.0" && git push --follow-tags`.

App chưa ký bằng Apple Developer ID. Cài bằng lệnh `curl` thì macOS không chặn.

## Build bản cài đặt thủ công

```bash
npm run tauri build
```

Ra `.app`/`.dmg` trên macOS và `.deb`/`.AppImage` trên Ubuntu (build trên từng hệ điều hành).
