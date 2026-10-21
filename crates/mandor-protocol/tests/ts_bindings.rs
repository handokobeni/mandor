#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Tipe TypeScript client harus sama persis dengan hasil ts-rs dari tipe Rust.
//! Jalankan dengan `MANDOR_REGEN_TS=1` untuk menulis ulang file.

use std::path::PathBuf;

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/lib/protocol")
}

#[test]
fn file_typescript_sinkron_dengan_tipe_rust() {
    let dir = out_dir();
    let regen = std::env::var("MANDOR_REGEN_TS").is_ok_and(|v| v == "1");
    if regen {
        std::fs::create_dir_all(&dir).unwrap();
    }
    for (name, body) in mandor_protocol::ts_bindings().unwrap() {
        let path = dir.join(name);
        if regen {
            std::fs::write(&path, &body).unwrap();
            continue;
        }
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            current, body,
            "{name} menyimpang dari tipe Rust; jalankan MANDOR_REGEN_TS=1 cargo test -p mandor-protocol"
        );
    }
}
