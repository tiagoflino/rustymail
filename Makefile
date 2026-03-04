.PHONY: install lint test build dev

install:
	npm ci
	cargo fetch --manifest-path src-tauri/Cargo.toml

lint:
	npm run check
	cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings

test:
	npm run test
	cargo test --manifest-path src-tauri/Cargo.toml

build:
	npm run build
	cargo build --manifest-path src-tauri/Cargo.toml --release

dev:
	npm run tauri dev
