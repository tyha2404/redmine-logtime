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

## Cài đặt

Trên macOS (Apple Silicon hoặc Intel) và Ubuntu/Debian, mở Terminal và chạy lệnh in ở job `release` của pipeline GitLab (hoặc trong trang Release, hoặc `make install-command`). Dạng lệnh:

```bash
curl -fsSL https://<gitlab>/api/v4/projects/<id>/packages/generic/redmine-logtime/latest/install.sh | bash
```

Lệnh tải bản build sẵn của release mới nhất rồi cài: macOS vào `/Applications` (hoặc `~/Applications` nếu không có quyền ghi), Ubuntu qua `apt` (hỏi `sudo`). Không cần clone source, không cần Node hay Rust. Nếu release chưa có bản cho máy đó, script tự tải mã nguồn của đúng phiên bản và build (lúc này cần Node 20+, Rust được cài tự động).

- **Cập nhật**: chạy lại đúng lệnh trên. Đã là bản mới nhất thì không làm gì; app cũng báo ở góc trái khi có bản mới, và **Cài đặt → Phiên bản** có nút chép lệnh.
- **Bản cụ thể**: `... | bash -s -- --version 0.2.0`
- **Gỡ**: `... | bash -s -- --uninstall` (giữ lại cài đặt và API key)
- **Project private**: `curl -fsSL -H "PRIVATE-TOKEN: <token>" <link> | GITLAB_TOKEN=<token> bash`, token cần quyền `read_api`.

Lần đầu mở app sẽ hỏi địa chỉ Redmine và API key, sau đó vào thẳng lịch tháng.

Từ thư mục source vẫn dùng được `make install` (build rồi cài), `./scripts/install.sh --prebuilt` (tải bản build sẵn), `make build`, `make uninstall`.

## Phát hành lên GitLab

Pipeline trong `.gitlab-ci.yml`:

- Push lên nhánh / merge request: `tsc`, `cargo test`, kiểm tra cú pháp script cài.
- Push tag `vX.Y.Z`: build `.deb` (Ubuntu 22.04, amd64), đưa lên Package Registry của project, đăng `install.sh` đã điền sẵn địa chỉ GitLab, tạo GitLab Release kèm link tải và lệnh cài.

Phát hành một phiên bản:

```bash
make release VERSION=0.2.0     # đổi version ở package.json, tauri.conf.json, Cargo.toml; commit; tạo tag
git push --follow-tags
```

**Bản macOS** cần máy Mac để build:

- Có runner macOS (GitLab SaaS Premium hoặc runner tự dựng): đặt biến CI `MACOS_RUNNER_TAG` bằng tag của runner, job `build:macos` sẽ tự chạy.
- Không có: sau khi push tag, trên máy Mac checkout tag đó rồi chạy `GITLAB_TOKEN=<token quyền api> make publish-macos`. Script build bản universal (Apple Silicon + Intel), tải lên Package Registry và gắn vào release.

Thiết lập một lần trên GitLab: bật Package Registry cho project (Settings → General → Visibility). Project public thì ai cũng cài được bằng lệnh một dòng; private thì người cài cần token.

App chưa ký bằng Apple Developer ID (chỉ ký ad-hoc). Cài bằng lệnh `curl` thì macOS không chặn; nếu tải file `.tar.gz` bằng trình duyệt, lần đầu mở bằng chuột phải → Open.

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

## Nghỉ phép

Theo cách HR đang làm: dưới ESTUARY → GNA mỗi loại nghỉ có một project (Annual Leave, Sick Leave, Unpaid Leave); đầu mỗi tháng HR tạo task cha trong từng project ("Xin nghỉ phép Tháng 10.2026", "… Đột Xuất …", "… KL …"). Mỗi ngày nghỉ là một issue con dưới task cha của tháng đó, log giờ vào issue con.

- **Cài đặt → Nghỉ phép → Tìm trên Redmine**: app tìm các project có tên "Leave"/"Nghỉ", activity cùng tên, và tên bạn hay dùng trong tiêu đề (ví dụ "BE - Tỷ") từ các issue nghỉ cũ.
- **Lịch → chọn ngày → Xin nghỉ ngày này**: chọn loại (phép năm, đột xuất, không lương) và cả ngày/buổi sáng/buổi chiều. App tìm task cha của tháng, đặt tên issue kiểu "BE - Tỷ xin nghỉ phép ngày 29/10/2026" (sửa được), ngày bắt đầu = ngày kết thúc = ngày nghỉ, rồi đưa vào hàng chờ. Ngày nghỉ trong báo cáo cũng đi theo cách này; loại nghỉ đổi được ngay trong hàng chờ.
- Lịch tính giờ log trong các project nghỉ là ngày nghỉ. Nếu HR chưa tạo task cha cho tháng, app báo và chưa cho thêm.

## Tính năng AI

Cần [Claude CLI](https://claude.ai/install.sh) đã đăng nhập và `uv` (để chạy [mcp-redmine](https://github.com/runekaagaard/mcp-redmine)). App tự cấu hình MCP với `REDMINE_READ_ONLY=1`: Claude chỉ đọc Redmine, mọi thao tác ghi do app làm sau khi bạn duyệt.

1. **Cài đặt → Cấu trúc Redmine → Phân tích**: Claude tìm cách nhận biết task cha do BA tạo và tracker task con. Có thể thêm ghi chú riêng để Claude ưu tiên.
2. **Ghi logtime**, phần trên: dán báo cáo tuần + báo cáo ngày. Claude tìm task cha/task con có sẵn; app chia giờ (bội số 0.5h) cho đủ giờ còn trống mỗi ngày. Sửa task cha/tên task con nếu cần rồi đưa vào hàng chờ.
3. **Ghi logtime**, phần dưới: hàng chờ theo từng ngày. Sửa giờ, ghi chú, hoặc bấm **Đổi task** để đổi task cha, tên task con, hay log thẳng vào một issue có sẵn; rồi ghi lên Redmine. Có thể hoàn tác trong **Lịch sử ghi**.

Nếu `~/.claude/settings.json` đặt `ANTHROPIC_API_KEY` mà key đó không dùng được ở chế độ `claude -p`, bật tuỳ chọn “Không dùng ~/.claude/settings.json khi gọi Claude” để dùng tài khoản claude.ai đã đăng nhập.

Thử thật với Redmine local (tốn một ít lượt dùng Claude):

```bash
REDMINE_API_KEY=... cargo run -p ai-bridge --example local_ai
```

## Xem giao diện bằng dữ liệu mẫu

Không cần Redmine hay Tauri: chạy UI trong trình duyệt với dữ liệu tháng 10/2026 giả lập (`src/mock.ts`, không có trong bản build thật):

```bash
make mock          # = VITE_MOCK=1 npx vite --port 1430
# mở http://localhost:1430/?page=calendar  (report | review | history | settings)
# màn chào lần đầu: http://localhost:1430/?setup=1
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

Ra `.app`/`.dmg` trên macOS và `.deb`/`.AppImage` trên Ubuntu (build trên từng hệ điều hành). Bản phát hành chính thức đi qua GitLab CI, xem phần trên.
