use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Versi protokol `MAJOR.MINOR`. Mayor berbeda ditolak saat handshake.
/// Minor 1: klaim membawa id daemon (KTD-55 plan VM); app hanya mengaktifkan
/// link VM bila daemon laptop melaporkan minor 1 atau lebih.
pub const PROTOCOL_VERSION: &str = "1.1";
/// Bagian mayor dari [`PROTOCOL_VERSION`].
pub const PROTOCOL_MAJOR: u32 = 1;

/// Parameter method `hello`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Hello {
    pub token: String,
    pub protocol: String,
    pub client: String,
}

/// Hasil `hello` yang berhasil.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct HelloOk {
    pub daemon_version: String,
    pub protocol: String,
}

/// Mengambil bagian mayor dari string versi `MAJOR.MINOR`.
#[must_use]
pub fn major_of(version: &str) -> Option<u32> {
    version.split('.').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mengurai_mayor() {
        assert_eq!(major_of("1.0"), Some(1));
        assert_eq!(major_of("2.7"), Some(2));
        assert_eq!(major_of("x"), None);
    }
}
