//! Hash kecil yang stabil lintas versi Rust, untuk nilai yang disimpan.

/// FNV-1a 64-bit dalam heksadesimal. Bukan untuk keamanan.
#[must_use]
pub fn fnv1a_hex(s: &str) -> String {
    format!("{:016x}", fnv1a(s))
}

fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sesuai_nilai_referensi_fnv1a() {
        assert_eq!(fnv1a_hex(""), "cbf29ce484222325");
        assert_eq!(fnv1a_hex("a"), "af63dc4c8601ec8c");
    }
}
