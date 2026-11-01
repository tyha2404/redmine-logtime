# Shortcuts; see README.

# rustup cài cargo vào ~/.cargo/bin; thêm vào PATH phòng khi shell chưa nạp.
export PATH := $(HOME)/.cargo/bin:$(PATH)
.PHONY: dev mock test build install uninstall release publish-macos install-command

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

# Bản macOS khi GitLab không có runner macOS: GITLAB_TOKEN=... make publish-macos
publish-macos:
	./scripts/publish-macos.sh

# In lệnh cài một dòng cho project GitLab trong git remote origin.
install-command:
	@./scripts/install-command.sh
