#!/usr/bin/env bash
# Pembuktian lapisan (U1): dependensi yang melanggar arah lapisan harus membuat
# gerbang merah. Setiap kasus menambah dependensi terlarang sementara, menjalankan
# `cargo deny check bans`, lalu memulihkan berkas apa pun hasilnya.
set -euo pipefail

cd "$(dirname "$0")/.."
backup="$(mktemp -d)"
cp Cargo.lock "$backup/Cargo.lock"

restore() {
  for f in "$backup"/*.toml; do
    [ -e "$f" ] || continue
    name="$(basename "$f" .toml)"
    cp "$f" "crates/$name/Cargo.toml"
  done
  cp "$backup/Cargo.lock" Cargo.lock
  rm -rf "$backup"
}
trap restore EXIT

expect_red() {
  local crate="$1" dep="$2"
  cp "crates/$crate/Cargo.toml" "$backup/$crate.toml"
  sed -i.bak "s/^\[dependencies\]$/[dependencies]\n$dep.workspace = true/" "crates/$crate/Cargo.toml"
  rm -f "crates/$crate/Cargo.toml.bak"
  if cargo deny --log-level error check bans >/dev/null 2>&1; then
    echo "GAGAL: $dep di $crate tidak membuat gerbang merah" >&2
    exit 1
  fi
  echo "ok: $dep di $crate membuat gerbang merah"
  cp "$backup/$crate.toml" "crates/$crate/Cargo.toml"
  cp "$backup/Cargo.lock" Cargo.lock
}

expect_red mandor-domain sqlx
expect_red mandor-usecase axum
expect_red mandor-domain reqwest
echo "red-check lulus: lapisan ditegakkan oleh cargo deny"
