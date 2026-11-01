# Redmine Logtime

App trên máy tính (macOS, Ubuntu) để xem, ghi logtime hàng loạt và xin nghỉ phép trên [Redmine](https://www.redmine.org/). Mọi thứ vào hàng chờ để bạn kiểm tra, **chỉ ghi lên Redmine khi bạn bấm ghi**.

![Hàng chờ dạng bảng tuần](docs/images/week-grid.png)

## Cài đặt

```bash
curl -fsSL https://raw.githubusercontent.com/tyha2404/redmine-logtime/HEAD/scripts/install.sh | bash
```

Chạy lại lệnh này để cập nhật. Gỡ: thêm `-s -- --uninstall` sau `bash`.

Tính năng AI (tuỳ chọn) cần [Claude CLI](https://claude.ai/install.sh) đã đăng nhập và [uv](https://docs.astral.sh/uv/).

## Bắt đầu

Lần đầu mở app, nhập địa chỉ Redmine và API key (Redmine → **Tài khoản của tôi** → **API access key** → **Hiện**).

## Cách dùng

![Lịch tháng](docs/images/calendar.png)

1. **Thêm vào hàng chờ**, theo một trong ba cách:
   - Trên **Lịch tháng**, chọn ngày → **Thêm logtime vào issue** (issue có sẵn hoặc tạo task con mới).
   - Chọn ngày → **Xin nghỉ ngày này**. Cần chạy **Cài đặt → Nghỉ phép → Tìm trên Redmine** một lần trước.
   - **Ghi logtime** → dán báo cáo công việc → **Phân tích bằng AI**. Claude tìm task cha (chỉ đọc Redmine), app chia giờ cho đủ mỗi ngày.
2. **Kiểm tra** ở **Ghi logtime → Hàng chờ**:
   - Bấm ô để sửa giờ; gõ 0 để bỏ.
   - Bỏ tick những mục chưa muốn ghi.
   - Dòng **Tổng ngày** báo đủ (✓), thiếu (−) hay vượt (+) giờ.
3. **Ghi**: bấm **Ghi N logtime lên Redmine**, rồi **Ghi ngay**. Mục lỗi tự quay lại hàng chờ. Ghi nhầm thì vào **Lịch sử ghi** → **Hoàn tác lô này**.

## Gặp lỗi

- **macOS chặn app:** cài bằng lệnh `curl` ở trên. Nếu tải file bằng trình duyệt, mở app bằng chuột phải → **Open**.
- **Lỗi SSL với Redmine nội bộ:** **Cài đặt → Kết nối Redmine** → tick **Chấp nhận chứng chỉ SSL tự ký**.
- **AI báo lỗi:** **Cài đặt → Claude** → **Chạy thử Claude**.

## Phát triển

```bash
npm install && npm run tauri dev   # cần Rust và Node 20+
make mock                          # chỉ giao diện, dữ liệu mẫu
make test
```

Xem thêm [docs/development.md](docs/development.md).

## Giấy phép

[MIT](LICENSE)
