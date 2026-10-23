#!/bin/sh
# Versi app desktop dan daemon harus sama (KTD-48 plan service OS): installer
# Windows membawa daemon dengan versi yang sama dengan app.
set -eu
cd "$(dirname "$0")/.."
toml() { sed -n 's/^version = "\(.*\)"/\1/p' "$1" | head -n1; }
json() { sed -n 's/^  "version": "\(.*\)",/\1/p' "$1" | head -n1; }
daemon=$(toml crates/mandord/Cargo.toml)
app=$(json apps/desktop/src-tauri/tauri.conf.json)
desktop=$(toml apps/desktop/src-tauri/Cargo.toml)
web=$(json apps/desktop/package.json)
echo "mandord=$daemon tauri.conf=$app mandor-desktop=$desktop package.json=$web"
if [ -z "$daemon" ] || [ "$daemon" != "$app" ] || [ "$daemon" != "$desktop" ] || [ "$daemon" != "$web" ]; then
  echo "versions differ: bump them together" >&2
  exit 1
fi
