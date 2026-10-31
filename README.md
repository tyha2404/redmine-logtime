# Redmine Logtime

Ứng dụng desktop (macOS, Ubuntu) để xem lịch logtime trên Redmine và (sắp tới) dùng AI chuyển báo cáo công việc thành task con và logtime.

## Cấu trúc

```
crates/redmine-client   REST client cho Redmine (đọc, tạo task con, tạo/xoá logtime)
crates/logtime-core     Settings, tính trạng thái lịch tháng
crates/logtime-engine   Hàng chờ, duyệt, ghi theo lô (chống trùng), hoàn tác; SQLite
crates/ai-bridge        Gọi Claude CLI + mcp-redmine (chỉ đọc): phân tích cấu trúc Redmine, phân tích báo cáo
src-tauri/              App Tauri 2: commands, lưu settings và API key
src/                    Giao diện React
docker/                 Redmine local để test thao tác ghi
```

## Cài đặt bằng một lệnh

Trên macOS hoặc Ubuntu/Debian, từ thư mục dự án:

```bash
./scripts/install.sh        # hoặc: make install  /  npm run app:install
```

Script tự cài Rust nếu thiếu (cần sẵn Node.js 20+), cài các gói hệ thống cần cho Tauri trên Ubuntu (hỏi `sudo`), build bản release rồi cài:

- macOS: chép `Redmine Logtime.app` vào `/Applications` (hoặc `~/Applications` nếu không có quyền ghi)
- Ubuntu: build `.deb` rồi `sudo apt install` gói `redmine-logtime`

Các lệnh khác: `./scripts/install.sh --build-only` (chỉ build), `./scripts/install.sh --uninstall` (gỡ app, giữ lại cài đặt và API key). Cài lại bằng lệnh install sẽ ghi đè bản cũ.

## Chạy dev

Yêu cầu: Rust (rustup), Node 20+. Trên Ubuntu cần thêm các gói build cho Tauri:
`sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev`.

```bash
npm install
npm run tauri dev
```

URL Redmine và API key nhập trong màn **Cài đặt**. API key được lưu trong `credentials.json` ở thư mục cấu hình của app, chỉ user hiện tại đọc được (quyền 0600, thư mục 0700):

- macOS: `~/Library/Application Support/app.redmine-logtime.desktop/`
- Ubuntu: `~/.config/app.redmine-logtime.desktop/`

## Tính năng AI

Cần [Claude CLI](https://claude.ai/install.sh) đã đăng nhập và `uv` (để chạy [mcp-redmine](https://github.com/runekaagaard/mcp-redmine)). App tự cấu hình MCP với `REDMINE_READ_ONLY=1`: Claude chỉ đọc Redmine, mọi thao tác ghi do app làm sau khi bạn duyệt.

1. **Cài đặt → Cấu trúc Redmine → Phân tích**: Claude tìm cách nhận biết task cha do BA tạo, tracker task con, issue nghỉ phép. Có thể thêm ghi chú riêng để Claude ưu tiên.
2. **Nhập báo cáo**: dán báo cáo tuần + báo cáo ngày. Claude tìm task cha/task con có sẵn; app chia giờ (bội số 0.5h) cho đủ giờ còn trống mỗi ngày. Sửa task cha/tên task con nếu cần rồi đưa vào hàng chờ.
3. **Duyệt và ghi**: kiểm tra từng ngày, ghi lên Redmine; có thể hoàn tác trong **Lịch sử ghi**.

Nếu `~/.claude/settings.json` đặt `ANTHROPIC_API_KEY` mà key đó không dùng được ở chế độ `claude -p`, bật tuỳ chọn “Không dùng ~/.claude/settings.json khi gọi Claude” để dùng tài khoản claude.ai đã đăng nhập.

Thử thật với Redmine local (tốn một ít lượt dùng Claude):

```bash
REDMINE_API_KEY=... cargo run -p ai-bridge --example local_ai
```

## Xem giao diện bằng dữ liệu mẫu

Không cần Redmine hay Tauri: chạy UI trong trình duyệt với dữ liệu tháng 10/2026 giả lập (`src/mock.ts`, không có trong bản build thật):

```bash
VITE_MOCK=1 npx vite --port 1430
# mở http://localhost:1430/?page=calendar  (report | review | history | settings)
```

## Test

```bash
cargo test --workspace
```

## Redmine local

Không có project test trên Redmine thật, nên mọi thao tác ghi được thử trên Redmine chạy bằng Docker:

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

## Build bản cài đặt

```bash
npm run tauri build
```

Ra `.dmg`/`.app` trên macOS và `.deb`/`.AppImage` trên Ubuntu (build trên từng hệ điều hành).

Workflow `.github/workflows/release.yml` build cả 3 bản (macOS Apple Silicon, macOS Intel, Ubuntu) khi push tag `v*` và đính vào một GitHub release nháp. App chưa được ký: trên macOS lần đầu mở bằng chuột phải → Open.
