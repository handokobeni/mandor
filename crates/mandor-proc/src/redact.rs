//! Redaksi token dari teks sebelum masuk log, error, atau UI.

use std::borrow::Cow;

const MARK: &str = "[REDACTED]";

/// Prefiks token yang diikuti karakter token sampai pembatas.
const PREFIXES: &[&str] = &[
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "x-access-token:",
    "Bearer ",
    "bearer ",
];

/// Panjang minimal sisa token agar dianggap token (menghindari salah tangkap kata biasa).
const MIN_SECRET_LEN: usize = 8;

/// Mengganti token yang dikenal dengan `[REDACTED]`.
///
/// Mengembalikan `Cow::Borrowed` bila tidak ada yang diganti.
#[must_use]
pub fn redact(input: &str) -> Cow<'_, str> {
    if !PREFIXES.iter().any(|p| input.contains(p)) {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some((idx, prefix)) = earliest_prefix(rest) {
        let after = &rest[idx + prefix.len()..];
        let secret_len = after
            .char_indices()
            .find(|&(_, c)| !is_token_char(c))
            .map_or(after.len(), |(i, _)| i);
        out.push_str(&rest[..idx + prefix.len()]);
        if secret_len >= MIN_SECRET_LEN {
            out.push_str(MARK);
        } else {
            out.push_str(&after[..secret_len]);
        }
        rest = &after[secret_len..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

fn earliest_prefix(s: &str) -> Option<(usize, &'static str)> {
    PREFIXES
        .iter()
        .filter_map(|p| s.find(p).map(|i| (i, *p)))
        .min_by_key(|&(i, _)| i)
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '~' | '+' | '/' | '=')
}

#[cfg(test)]
mod tests {
    use super::redact;
    use pretty_assertions::assert_eq;

    #[test]
    fn redaksi_berbagai_token() {
        let cases = [
            (
                "url x-access-token",
                "https://x-access-token:ghs_AAAAAAAAAAAAAAAA@github.com/o/r",
                "https://x-access-token:[REDACTED]@github.com/o/r",
            ),
            (
                "pat klasik",
                "token ghp_abcdefghijklmnop01 kadaluarsa",
                "token ghp_[REDACTED] kadaluarsa",
            ),
            (
                "fine grained",
                "github_pat_11ABCDEFG_xyz123456789",
                "github_pat_[REDACTED]",
            ),
            (
                "bearer",
                "Authorization: Bearer u-abc.def-ghi_jkl",
                "Authorization: Bearer [REDACTED]",
            ),
            ("tanpa token", "semua baik", "semua baik"),
            (
                "prefiks pendek bukan token",
                "ghp_ab selesai",
                "ghp_ab selesai",
            ),
        ];
        for (name, input, want) in cases {
            assert_eq!(redact(input), want, "{name}");
        }
    }

    #[test]
    fn beberapa_token_dalam_satu_teks() {
        let s = "a ghp_1111111111111111 b gho_2222222222222222 c";
        assert_eq!(redact(s), "a ghp_[REDACTED] b gho_[REDACTED] c");
    }
}
