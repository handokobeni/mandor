.PHONY: check fmt lint deny test cover-check sqlx-check sqlx-prepare build run setup red-check

COVER_MIN ?= 85

check: fmt lint deny sqlx-check cover-check   # gerbang penuh rust-engineering

fmt:
	cargo fmt --all --check

lint:
	cargo clippy --workspace --all-targets --locked -- -D warnings

deny:
	cargo deny check

# .sqlx/ offline metadata harus cocok dengan query (SQLX_OFFLINE di CI)
SQLX_DB := sqlite://$(CURDIR)/target/sqlx-check.db

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
cover-check:
	cargo llvm-cov --workspace --locked --summary-only \
		--ignore-filename-regex 'crates/mandord/' \
		--fail-under-lines $(COVER_MIN)

build:
	cargo build --release --locked --bin mandord

run:
	cargo run --bin mandord

red-check:
	./scripts/red-check.sh

setup:
	git config core.hooksPath .githooks && chmod +x ../.githooks/*
	cargo install cargo-deny cargo-llvm-cov sqlx-cli --locked
