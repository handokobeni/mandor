.PHONY: check fmt lint deny test cover-check sqlx-check sqlx-prepare build run setup red-check clean-build

COVER_MIN ?= 85

check: fmt lint deny sqlx-check cover-check   # gerbang penuh rust-engineering

fmt:
	cargo fmt --all --check

lint:
	cargo clippy --workspace --all-targets --locked -- -D warnings

deny:
	cargo deny check

# .sqlx/ offline metadata harus cocok dengan query (SQLX_OFFLINE di CI)
# Tanpa `//`: `sqlite://D:/...` dibaca sebagai host `D:` di Windows, sedangkan
# `sqlite:<jalur absolut>` sah di Linux, macOS, dan Windows.
SQLX_DB := sqlite:$(CURDIR)/target/sqlx-check.db

sqlx-check:
	mkdir -p target
	cd crates/mandor-store && DATABASE_URL=$(SQLX_DB) sqlx database reset -y >/dev/null
	cd crates/mandor-store && DATABASE_URL=$(SQLX_DB) SQLX_OFFLINE=false cargo sqlx prepare --check -- --tests

# Menulis ulang cache offline setelah mengubah query.
sqlx-prepare:
	mkdir -p target
	cd crates/mandor-store && DATABASE_URL=$(SQLX_DB) sqlx database reset -y >/dev/null
	cd crates/mandor-store && DATABASE_URL=$(SQLX_DB) SQLX_OFFLINE=false cargo sqlx prepare -- --tests

test:
	cargo test --workspace --locked

# mandord (wiring) dikecualikan dari floor, seperti crates/app di skill.
# Artefak workspace dan .profraw lama dibersihkan dulu, dan incremental dimatikan,
# agar target/llvm-cov-target tidak terus membesar di setiap run.
cover-check:
	cargo llvm-cov clean --workspace
	CARGO_INCREMENTAL=0 cargo llvm-cov --workspace --locked --summary-only \
		--ignore-filename-regex 'crates/mandord/' \
		--fail-under-lines $(COVER_MIN)

build:
	cargo build --release --locked --bin mandord

run:
	cargo run --bin mandord

# Membersihkan folder build daemon dan app desktop (Tauri) bila sudah menumpuk,
# mis. sebulan sekali atau setelah memperbarui dependensi. Build berikutnya
# sekitar 1-2 menit lebih lama.
clean-build:
	@du -sh target apps/desktop/src-tauri/target 2>/dev/null || true
	cargo clean
	cd apps/desktop/src-tauri && cargo clean

red-check:
	./scripts/red-check.sh

setup:
	git config core.hooksPath .githooks && chmod +x ../.githooks/*
	cargo install cargo-deny cargo-llvm-cov sqlx-cli --locked
