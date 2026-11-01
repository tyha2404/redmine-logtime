# Shortcuts; see README.

# rustup cài cargo vào ~/.cargo/bin; thêm vào PATH phòng khi shell chưa nạp.
export PATH := $(HOME)/.cargo/bin:$(PATH)
.PHONY: dev mock test build install uninstall release package-macos

dev:
	npm run tauri dev

# UI with canned data in the browser: http://localhost:1430/?page=calendar
mock:
	VITE_MOCK=1 npx vite --port 1430

test:
	cargo test --workspace && npx tsc --noEmit

build:
	./scripts/install.sh --build-only

install:
	./scripts/install.sh

uninstall:
	./scripts/install.sh --uninstall

# make release VERSION=0.2.0 && git push --follow-tags
release:
	@test -n "$(VERSION)" || (echo "Dùng: make release VERSION=0.2.0" && exit 1)
	./scripts/release.sh $(VERSION)

# Bản macOS universal đóng gói như trên GitHub Release (target/publish/).
package-macos:
	./scripts/package-macos.sh
