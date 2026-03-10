.PHONY: install lint test build dev release

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

release:
ifndef VERSION
	$(error Usage: make release VERSION=0.2.0)
endif
	@echo "Releasing v$(VERSION)..."
	@# Bump version in all three files
	sed -i '' 's/"version": "[^"]*"/"version": "$(VERSION)"/' package.json
	sed -i '' 's/"version": "[^"]*"/"version": "$(VERSION)"/' src-tauri/tauri.conf.json
	sed -i '' 's/^version = "[^"]*"/version = "$(VERSION)"/' src-tauri/Cargo.toml
	@# Regenerate Cargo.lock
	cargo update --manifest-path src-tauri/Cargo.toml --workspace
	@# Verify everything passes
	$(MAKE) lint
	$(MAKE) test
	@# Commit, tag, push
	git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
	git commit -m "release v$(VERSION)"
	git tag "v$(VERSION)"
	git push origin main "v$(VERSION)"
	@echo "Released v$(VERSION) — CI will build and publish to GitHub Releases."
