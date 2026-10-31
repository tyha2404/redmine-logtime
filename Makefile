# Shortcuts; see README.
.PHONY: dev test build install uninstall

dev:
	npm run tauri dev

test:
	cargo test --workspace && npx tsc --noEmit

build:
	./scripts/install.sh --build-only

install:
	./scripts/install.sh

uninstall:
	./scripts/install.sh --uninstall
