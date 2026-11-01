# Redmine Logtime

Ứng dụng cho macOS và Ubuntu giúp xem giờ làm việc, ghi giờ vào công việc (issue) và tạo yêu cầu nghỉ phép trên Redmine.

**App chỉ ghi lên Redmine sau khi bạn xem lại và bấm Ghi ngay trong popup xác nhận.**

## 1. Cài đặt

### macOS

Hỗ trợ máy Mac dùng chip Intel và Apple Silicon, macOS 11 trở lên.

1. Mở **Terminal** bằng Spotlight (`⌘ Space`, gõ `Terminal`).
2. Dán lệnh sau, nhấn Enter và chờ báo **Đã cài**:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/tyha2404/redmine-logtime/HEAD/scripts/install.sh | bash
   ```

3. Mở **Redmine Logtime** bằng Spotlight hoặc trong thư mục **Applications**.

### Ubuntu

Bản cài sẵn dành cho máy Intel/AMD 64-bit (`amd64`), được build trên Ubuntu 22.04.

1. Mở **Terminal** (`Ctrl Alt T`). Nếu máy chưa có `curl`, chạy:

   ```bash
   sudo apt update && sudo apt install -y curl
   ```

2. Cài app bằng lệnh:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/tyha2404/redmine-logtime/HEAD/scripts/install.sh | bash
   ```

3. Nhập mật khẩu máy nếu được hỏi (khi gõ sẽ không hiện ký tự). Cài xong, mở **Redmine Logtime** từ menu ứng dụng.

Nếu trình cài báo chưa có bản build sẵn và chuyển sang build từ mã nguồn, cần thêm Node.js 22, Rust và công cụ build. Xem [cách chuẩn bị môi trường](docs/development.md#chuẩn-bị-môi-trường).

## 2. Kết nối lần đầu

1. Trên Redmine, vào **Tài khoản của tôi → API access key → Hiện** để sao chép API key.
2. Mở app, nhập **Địa chỉ Redmine** của công ty và **API key**.
3. Bấm **Kết nối và mở lịch**.

API key được lưu trên máy của bạn; không chia sẻ key cho người khác. Nếu không thấy mục API access key, nhờ quản trị viên Redmine hỗ trợ.

Trong **Cài đặt → Ngày làm việc**, kiểm tra số giờ mỗi ngày, ngày làm việc và ngày lễ, rồi bấm **Lưu cài đặt**.

![Lịch tháng hiển thị giờ làm việc](docs/images/calendar.png)

## 3. Sử dụng hằng ngày

### Ghi giờ từ báo cáo công việc

Cần AI: cài [Claude CLI](https://claude.ai/install.sh) và [uv](https://docs.astral.sh/uv/), mở Terminal chạy `claude` để đăng nhập, rồi vào **Cài đặt → Chạy thử Claude**. Nên chạy **Phân tích cấu trúc Redmine** trước lần đầu.

1. Vào **Ghi logtime**, dán báo cáo có ngày và nội dung công việc → **Phân tích bằng AI**.
2. Kết quả được nhóm theo ngày, mỗi ngày có tổng giờ so với định mức. Kiểm tra từng dòng:
   - Bỏ tick việc không muốn ghi ở ngày đó; giờ trong ngày được chia lại.
   - Sửa số giờ ở ô bên phải; nhập `0` để bỏ.
   - Bấm dòng **task cha › task con** dưới tên việc để đổi: gõ số issue hoặc từ khoá để tìm task cha, chọn task con có sẵn hoặc tạo mới. Thay đổi áp dụng cho việc đó ở mọi ngày.
3. Bấm **Ghi N logtime lên Redmine**, xem lại trong popup rồi bấm **Ghi ngay**.

Nếu có mục lỗi, popup ghi rõ lý do. Bấm **Thử lại phần lỗi**, hoặc đóng popup, sửa rồi ghi lại; các mục đã ghi được đánh dấu ✓ và không ghi lần nữa. AI chỉ đọc Redmine để đề xuất; bạn quyết định mục nào được ghi.

![Kết quả phân tích báo cáo](docs/images/report-result.png)

### Ghi giờ thủ công

Trên **Lịch tháng**, chọn ngày → **Ghi logtime vào issue**, chọn issue có sẵn hoặc tạo task con mới, nhập số giờ → **Ghi lên Redmine…** → **Ghi ngay**.

### Xin nghỉ phép

Lần đầu: vào **Cài đặt → Nghỉ phép → Tìm trên Redmine**, kiểm tra loại nghỉ và tên của bạn, rồi bấm **Lưu cài đặt**.

Sau đó, chọn ngày trên **Lịch tháng → Xin nghỉ ngày này**, điền thông tin → **Ghi … nghỉ lên Redmine…** → **Ghi ngay**. Nếu không tìm thấy task cha nghỉ phép của tháng, liên hệ HR.

### Lấy báo cáo từ Google Chat (tùy chọn)

Nếu bạn gửi báo cáo vào group Google Chat, app có thể lấy lại các tin đó thay vì dán tay. Cần Client ID và Client secret do admin Google Workspace của công ty cung cấp ([hướng dẫn cho admin](docs/development.md#google-chat)).

1. Vào **Cài đặt → Google Chat**, nhập Client ID, Client secret → **Kết nối Google**, rồi đăng nhập và cho phép trong trình duyệt.
2. Vào **Ghi logtime → Lấy từ Google Chat**, chọn group và khoảng ngày; tin nhắn tự tải.
3. Bỏ tick những tin không phải báo cáo → **Đưa vào báo cáo**, rồi **Phân tích bằng AI** như trên.

App chỉ đọc tin nhắn do bạn gửi, không gửi hay sửa gì trên Google Chat.

### Hoàn tác khi ghi nhầm

Bấm **Hoàn tác lô này** ngay trong popup sau khi ghi, hoặc vào **Lịch sử ghi**. Thao tác này xóa các logtime đã tạo trong lô; **task con vẫn được giữ trên Redmine**.

## 4. Cập nhật và gỡ cài đặt

### Cập nhật lên bản mới

Khi có bản mới, biểu tượng tải xuống ở góc trên app có chấm đỏ.

1. Đóng app.
2. Mở Terminal, chạy **đúng lệnh cài đặt** (giống lúc cài lần đầu):

   ```bash
   curl -fsSL https://raw.githubusercontent.com/tyha2404/redmine-logtime/HEAD/scripts/install.sh | bash
   ```

3. Mở lại app. Cài đặt, API key và lịch sử ghi được giữ nguyên.

Nếu máy đã là bản mới nhất, lệnh báo vậy và không làm gì thêm.

### Gỡ app

Mở Terminal và chạy lệnh dưới. Lệnh này giống lệnh cài đặt, chỉ thêm `-s -- --uninstall` ở cuối:

```bash
curl -fsSL https://raw.githubusercontent.com/tyha2404/redmine-logtime/HEAD/scripts/install.sh | bash -s -- --uninstall
```

Cài đặt, API key và lịch sử ghi vẫn được giữ trên máy để dùng khi cài lại. Gỡ app không xóa logtime đã ghi trên Redmine.

## 5. Khi gặp lỗi

- **Không kết nối được Redmine:** kiểm tra địa chỉ, API key và kết nối mạng/VPN của công ty.
- **Redmine nội bộ dùng SSL tự ký:** chọn mục tương ứng ở màn kết nối, hoặc **Cài đặt → Kết nối Redmine → Chấp nhận chứng chỉ SSL tự ký** nếu công ty dùng loại chứng chỉ này.
- **macOS chặn app tải bằng trình duyệt:** thử cài lại bằng lệnh Terminal ở trên.
- **AI báo lỗi:** kiểm tra đã đăng nhập Claude, rồi vào **Cài đặt → Chạy thử Claude**.

---

Dành cho người đóng góp: [Hướng dẫn phát triển](docs/development.md) · [Quy ước đóng góp](AGENTS.md). Giấy phép: [MIT](LICENSE).
