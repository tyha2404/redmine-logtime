# Phát triển Redmine Logtime

Dành cho người chạy từ mã nguồn, đóng góp hoặc phát hành app. Người dùng chỉ cần [hướng dẫn cài đặt và sử dụng](../README.md). Quy ước đóng góp nằm trong [AGENTS.md](../AGENTS.md).

## Cấu trúc dự án

| Thư mục | Vai trò |
| --- | --- |
| `src/` | Giao diện React/TypeScript, hooks, CSS và dữ liệu mẫu |
| `src-tauri/` | App Tauri 2, commands, trạng thái và lưu API key |
| `crates/redmine-client/` | REST client: đọc Redmine, tạo task con, tạo/xóa logtime |
| `crates/logtime-core/` | Cài đặt, lịch tháng, nghỉ phép và chia giờ |
| `crates/logtime-engine/` | Ghi theo lô (SQLite), thử lại, chống trùng và hoàn tác |
| `crates/ai-bridge/` | Gọi Claude CLI, phân tích cấu trúc Redmine và báo cáo |
| `crates/google-chat/` | Đăng nhập Google (OAuth, chỉ đọc) và lấy tin nhắn Google Chat |
| `docker/` | Redmine local và dữ liệu thử nghiệm |
| `scripts/` | Cài đặt, kiểm tra phiên bản và phát hành |
| `.github/workflows/` | CI và build bản phát hành |

## Chuẩn bị môi trường

Dùng **Node.js 22** như CI, npm và Rust stable qua rustup. Chạy các lệnh bên dưới từ thư mục gốc repo.

### macOS

Cài Xcode Command Line Tools nếu chưa có:

```bash
xcode-select --install
```

### Ubuntu

Cài các gói hệ thống dùng khi build:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config curl wget file libxdo-dev libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev
```

CI build trên Ubuntu 22.04. Docker chỉ cần khi thử với Redmine local; Claude CLI đã đăng nhập và `uv` chỉ cần cho tính năng AI.

## Chạy ứng dụng

```bash
npm ci
make dev
```

`make dev` tương đương `npm run tauri dev`, khởi chạy cả giao diện và app desktop. Nhập URL Redmine và API key ở màn kết nối hoặc trong **Cài đặt**.

### Chỉ xem giao diện

```bash
make mock
```

Mở `http://localhost:1430/?page=calendar`. Có thể đổi `calendar` thành `review`, `leave`, `history`, `settings`; dùng `?setup=1` để xem màn kết nối lần đầu, thêm `&slow=1` để dữ liệu mẫu trả về chậm và xem skeleton khi tải, `&update=1` để có bản mới giả lập (xem nút cập nhật ở thanh bên trái và thanh trạng thái).

Chế độ này dùng `src/mock.ts`, với dữ liệu tháng 10/2026; không cần Tauri hoặc Redmine. Không bật `VITE_MOCK` khi build bản dùng thật. `npm run dev` chỉ chạy Vite, không tự bật dữ liệu mẫu.

## Kiểm tra trước khi gửi thay đổi

```bash
make test
npm run build
```

- `make test`: `cargo test --workspace` và `npx tsc --noEmit`.
- `npm run build`: kiểm tra TypeScript và build frontend vào `dist/`.
- `cargo test -p logtime-engine`: chạy riêng một crate khi phát triển.

Rust dùng `#[test]`, `#[tokio::test]` và Wiremock; test nằm trong các module `mod tests`. Chưa có test runner frontend hoặc mức coverage bắt buộc. Với thay đổi giao diện, kiểm tra bằng `make mock`; với kết nối desktop, kiểm tra bằng `make dev`.

CI chạy TypeScript, Rust tests và kiểm tra cú pháp shell trên các lần push và pull request. Xem cấu hình thực tế ở [ci.yml](../.github/workflows/ci.yml).

## Thử với Redmine local

Dùng môi trường này cho thao tác ghi và hoàn tác:

```bash
docker compose -f docker/docker-compose.yml up -d
docker compose -f docker/docker-compose.yml logs -f redmine
```

Khi Redmine sẵn sàng, thoát xem log bằng `Ctrl C` rồi nạp dữ liệu:

```bash
docker compose -f docker/docker-compose.yml exec redmine bin/rails runner /seed.rb
```

Seed in API key của `dev1`. Trong app, nhập URL `http://localhost:3000` và key đó. Có thể đăng nhập web bằng `dev1` / `password123`; tài khoản quản trị mặc định là `admin` / `admin`.

Thay `YOUR_LOCAL_API_KEY` trong ví dụ bằng key local của bạn. Dữ liệu mẫu và các ví dụ sử dụng tháng 10/2026.

```bash
# Đọc thông tin người dùng và logtime
REDMINE_URL=http://localhost:3000 REDMINE_API_KEY=YOUR_LOCAL_API_KEY \
  cargo run -p redmine-client --example smoke -- 2026-10-01 2026-10-31

# Tạo task con, ghi giờ, chạy lại không trùng, hoàn tác; thử lại sau lỗi
# mạng không ghi trùng; lỗi 422; chặn ngày vượt 24h
REDMINE_URL=http://localhost:3000 REDMINE_API_KEY=YOUR_LOCAL_API_KEY \
  cargo run -p logtime-engine --example local_e2e

# Thử Claude thật với Redmine local; có sử dụng hạn mức Claude
REDMINE_API_KEY=YOUR_LOCAL_API_KEY cargo run -p ai-bridge --example local_ai
```

Ví dụ `local_e2e` giữ lại task con sau khi hoàn tác logtime. Dừng môi trường bằng `docker compose -f docker/docker-compose.yml down`; dữ liệu PostgreSQL vẫn được giữ trong Docker volume.

## Luồng xử lý và dữ liệu

### Ghi lên Redmine và AI

Không có hàng chờ: người dùng chỉnh kết quả (task cha, task con, giờ) rồi xác nhận trong popup; app gửi một `WritePlan` qua `write_plan`. Trước khi gửi, `plan::check_plan` kiểm tra lại phía Rust: giờ hợp lệ, có issue, task con đủ thông tin, mỗi ngày không quá 24h tính cả giờ đã có trên Redmine.

Mỗi lần ghi là một lô trong SQLite: mục lỗi nằm lại trong lô để `retry_batch` gửi lại, mục đã ghi không bao giờ gửi lần hai (task con dùng `key` làm khoá chống trùng). Lỗi có thể Redmine đã nhận (mất kết nối, timeout, 5xx) được đánh dấu `uncertain`; khi thử lại, app tìm logtime/task con đã có trước khi tạo. Hoàn tác chỉ xóa logtime của lô, giữ task con.

App gọi `claude -p` với `mcp-redmine` qua `uvx` và `REDMINE_READ_ONLY=1`. Claude tìm cấu trúc và đề xuất công việc, không quyết định số giờ; app chia giờ theo bước 0,5 giờ cho đủ giờ mỗi ngày, trừ giờ đã có trên Redmine và giờ nghỉ. Giữ nguyên quyền đọc của AI khi thay đổi tích hợp.

Sau mỗi lần ghi từ báo cáo, app lưu “dòng báo cáo → task cha, task con” (`ai-bridge::memory`, khoá `task_memory` trong SQLite). Ghi nhớ được dùng để gợi ý trong prompt, ghi đè đề xuất của Claude cho việc đã quen, và cho **Khớp nhanh** (`quick_report`): đọc ngày trong báo cáo rồi khớp dòng với ghi nhớ, không gọi Claude.

### Google Chat

Người dùng lấy tin nhắn của chính mình trong một group Google Chat để đưa vào ô báo cáo. App chỉ xin quyền `chat.spaces.readonly`, `chat.messages.readonly` và `openid email`, không gửi hay sửa tin nhắn. Đăng nhập theo OAuth cho app cài trên máy: mở trình duyệt, nhận mã qua `http://127.0.0.1:<cổng ngẫu nhiên>`, đổi mã bằng PKCE. Refresh token lưu trong `credentials.json`.

Mỗi công ty tự tạo OAuth client; app không kèm client chung. Admin Google Workspace làm một lần:

1. Tạo project trên Google Cloud Console, bật **Google Chat API**.
2. Trong trang cấu hình Google Chat API, điền tên app, ảnh đại diện và mô tả (Google yêu cầu kể cả khi chỉ đọc). Không cần bật tính năng tương tác.
3. **OAuth consent screen**: chọn loại **Internal** để không cần Google xét duyệt quyền đọc tin nhắn.
4. **Credentials → Create credentials → OAuth client ID**, loại **Desktop app**. Gửi Client ID và Client secret cho nhân viên nhập vào **Cài đặt → Tài khoản → Google Chat**.

Thử giao diện không cần Google: `make mock`, thêm `&gchat=0` vào URL để xem trạng thái chưa kết nối.

### Nghỉ phép

Mỗi loại nghỉ tương ứng một project; mỗi tháng có task cha do HR tạo. App tạo issue con cho từng ngày nghỉ và ghi giờ vào đó. **Cài đặt → Lịch & nghỉ phép → Nghỉ phép → Tìm trên Redmine** giúp tìm project, hoạt động và tên người dùng; cần **Lưu cài đặt** sau khi kiểm tra kết quả.

Seed Docker hiện dùng một issue nghỉ chung, chưa mô phỏng đầy đủ các project nghỉ và task cha theo tháng. Muốn thử toàn bộ luồng nghỉ phép, chuẩn bị thêm cấu trúc này trên Redmine local.

### Cấu hình và thông tin đăng nhập

App lưu `settings.json` và `redmine-profile.md` trong thư mục cấu hình:

- macOS: `~/Library/Application Support/app.redmine-logtime.desktop/`
- Ubuntu: `~/.config/app.redmine-logtime.desktop/`

API key và đăng nhập Google Chat nằm trong keychain của hệ điều hành (mục `app.redmine-logtime.desktop` / `credentials`; Secret Service trên Ubuntu). Máy không có keychain dùng được thì lưu `credentials.json` trong thư mục cấu hình, quyền `0600`, thư mục `0700`; file của bản cũ được chuyển vào keychain ở lần mở đầu. App ký ad-hoc nên sau mỗi bản mới macOS có thể hỏi lại quyền dùng Keychain.

Log nằm ở `~/Library/Logs/app.redmine-logtime.desktop/` (macOS) hoặc `~/.local/share/app.redmine-logtime.desktop/logs/` (Ubuntu), giữ 3 file gần nhất. Không ghi key, token hay nội dung báo cáo vào log.

Lịch sử ghi nằm trong `logtime.db` ở thư mục dữ liệu của app: trên macOS cùng đường dẫn trên, trên Ubuntu mặc định là `~/.local/share/app.redmine-logtime.desktop/`. Gỡ app bằng script vẫn giữ cấu hình và dữ liệu này.

### Icon và logo

Icon trong app là [Codicons](https://github.com/microsoft/vscode-codicons) của VS Code (`@vscode/codicons`) qua `src/icons.tsx`; thêm icon mới bằng cách khai báo tên trong đó, không tự vẽ SVG.

Logo là hình của [logo Redmine](https://github.com/edavis10/redmine_logo) (© 2009 Martin Herr, CC BY-SA 2.5) đổi sang các sắc xanh của app, trên ô trắng: `src-tauri/icons/source.svg` và `BrandMark` trong `src/icons.tsx`. Ô theo mẫu icon macOS của Apple: 824px trong canvas 1024px, bán kính góc 185,4px (22,37%), góc liền mạch (squircle, độ mượt 60%). Bản chỉnh sửa này cũng theo CC BY-SA 2.5 (khác giấy phép MIT của mã nguồn) và phải giữ ghi công. Đổi logo: sửa `source.svg`, xuất PNG 1024px, chạy `npx tauri icon <file.png> -o <thư mục tạm>` rồi chép `32x32.png`, `64x64.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico`, `icon.png` vào `src-tauri/icons/`. Icon thanh menu macOS (`tray-template.png`, 44px, hình đen trên nền trong) dùng cùng hình.

## Build và cài từ mã nguồn

| Lệnh | Kết quả |
| --- | --- |
| `make build` | Build bản release, không cài |
| `make install` | Build rồi cài vào máy |
| `make uninstall` | Gỡ app, giữ cấu hình và dữ liệu |
| `./scripts/install.sh --prebuilt` | Tải và cài bản phát hành có sẵn |

`make build` tạo `.app` trên macOS hoặc `.deb` trên Ubuntu, trong `target/release/bundle/`. Lệnh cài tải từ GitHub sẽ chuyển sang build từ mã nguồn nếu không tìm được bản cài sẵn phù hợp.

Muốn chọn loại gói thủ công, chạy trên đúng hệ điều hành:

```bash
npm run tauri build -- --bundles app        # macOS
npm run tauri build -- --bundles deb        # Ubuntu
```

## Phát hành

Trước khi phát hành, chạy kiểm tra và commit các thay đổi để working tree sạch. Chọn phiên bản mới theo dạng `X.Y.Z`:

```bash
make release VERSION=0.2.0
git push --follow-tags
```

Muốn có mục **Có gì mới** trong hộp cập nhật của app và trên GitHub Release, viết ghi chú (tiếng Việt, mỗi dòng một ý) vào một file ngoài repo rồi truyền vào: `NOTES_FILE=/tmp/ghi-chu.md make release VERSION=0.2.0`. Ghi chú được lưu trong tag và CI chép vào `latest.json`.

Lệnh `make release` đồng bộ phiên bản npm, Tauri và Cargo, tạo commit cùng tag `v0.2.0`. Push tag kích hoạt [release.yml](../.github/workflows/release.yml), tạo GitHub Release gồm:

- `redmine-logtime_X.Y.Z_amd64.deb`: Ubuntu, build trên 22.04.
- `Redmine-Logtime_X.Y.Z_macos-universal.tar.gz`: app macOS cho Intel và Apple Silicon.
- `install.sh`: script cài và cập nhật.
- `latest.json`: cho nút cập nhật trong app, chỉ có khi đã cấu hình khoá ký bên dưới.

### Khoá ký cho cập nhật trong app

App kiểm tra `latest.json` của bản mới nhất và chỉ cài file có chữ ký khớp với `plugins.updater.pubkey` trong `src-tauri/tauri.conf.json`. Workflow ký file `.deb` và `.tar.gz` bằng secret của repo (**Settings → Secrets and variables → Actions**):

- `TAURI_SIGNING_PRIVATE_KEY`: nội dung file khoá bí mật (tạo bằng `npx tauri signer generate -w ~/.tauri/redmine-logtime.key`).
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: mật khẩu khoá. Khoá không có mật khẩu thì không tạo secret này (GitHub không cho lưu giá trị trống).

Thiếu secret thì workflow bỏ qua bước ký; bản phát hành vẫn cài và cập nhật bằng `install.sh`. Giữ khoá bí mật ngoài repo. Mất khoá thì phải tạo cặp mới, đổi `pubkey`, và người dùng cập nhật một lần bằng lệnh cài.

Có thể đóng gói macOS universal tại máy Mac bằng `make package-macos`; kết quả ở `target/publish/`.

App chưa được ký bằng Apple Developer ID. Script cài đặt gỡ thuộc tính quarantine của app đã tải; bản tải trực tiếp bằng trình duyệt có thể bị macOS chặn.
