//! Pencocokan perintah sederhana dengan allowlist giliran, meniru semantik
//! `Bash(prefix…)` Claude Code (KTD-A5 plan 2026-11-06-001).

use super::shell::SimpleCommand;

/// Satu pola Bash dari allowlist, dicocokkan per kata agar kutipan tidak
/// bisa menyamarkan batas kata.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pattern {
    /// Kata yang harus sama persis.
    words: Vec<String>,
    /// Kata terakhir pola hanya awalan kata perintah (`Bash(git diff*)`).
    partial: Option<String>,
    /// Kata lain boleh menyusul (pola berakhiran `*` atau `:*`).
    open: bool,
    /// Pola `Bash(cd *)`: minimal satu kata lagi harus menyusul.
    needs_more: bool,
}

impl Pattern {
    fn parse(inner: &str) -> Self {
        let (body, open) = if let Some(p) = inner.strip_suffix(":*") {
            (p, true)
        } else if let Some(p) = inner.strip_suffix('*') {
            (p, true)
        } else {
            (inner, false)
        };
        let mut words: Vec<String> = body.split(' ').map(str::to_owned).collect();
        let partial = if open && !body.ends_with(' ') && !inner.ends_with(":*") {
            words.pop()
        } else {
            None
        };
        words.retain(|w| !w.is_empty());
        Self {
            words,
            partial,
            open,
            needs_more: open && body.ends_with(' '),
        }
    }

    fn matches(&self, command: &[String]) -> bool {
        let fixed = self.words.len();
        if command.len() < fixed || command[..fixed] != self.words[..] {
            return false;
        }
        let rest = &command[fixed..];
        match &self.partial {
            Some(p) => rest
                .first()
                .is_some_and(|w| w.starts_with(p.as_str()) && !w.contains(char::is_whitespace)),
            None if self.open => !(self.needs_more && rest.is_empty()),
            None => rest.is_empty(),
        }
    }
}

/// Pola Bash dari allowlist giliran; pola tool lain diabaikan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BashAllowlist {
    patterns: Vec<Pattern>,
}

impl BashAllowlist {
    /// Membangun dari entri allowlist apa adanya (mis. `Read`, `Bash(ls*)`).
    #[must_use]
    pub fn new<S: AsRef<str>>(entries: &[S]) -> Self {
        let patterns = entries
            .iter()
            .filter_map(|e| {
                let inner = e.as_ref().strip_prefix("Bash(")?.strip_suffix(')')?;
                Some(Pattern::parse(inner))
            })
            .filter(|p| !p.words.is_empty() || p.partial.as_ref().is_some_and(|w| !w.is_empty()))
            .collect();
        Self { patterns }
    }

    /// Perintah sederhana cocok dengan salah satu pola. Redirect tidak
    /// dinilai di sini: targetnya diperiksa keputusan izin (R16).
    #[must_use]
    pub fn allows(&self, command: &SimpleCommand) -> bool {
        self.patterns.iter().any(|p| p.matches(&command.words))
    }

    /// Setiap perintah sederhana cocok dengan allowlist.
    #[must_use]
    pub fn allows_all(&self, commands: &[SimpleCommand]) -> bool {
        !commands.is_empty() && commands.iter().all(|c| self.allows(c))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::super::shell::parse;
    use super::*;

    fn list() -> BashAllowlist {
        BashAllowlist::new(&[
            "Read",
            "Bash(git diff*)",
            "Bash(cd *)",
            "Bash(pwd)",
            "Bash(npm run test:*)",
            "Bash(tail *)",
            "Bash(npx *)",
        ])
    }

    fn allows(cmd: &str) -> bool {
        list().allows_all(&parse(cmd).unwrap())
    }

    #[test]
    fn prefix_dan_exact_seperti_claude_code() {
        assert!(allows("git diff --stat"));
        assert!(!allows("git push"));
        assert!(allows("pwd"));
        assert!(!allows("pwd -P"));
        assert!(allows("npm run test -- --watch=false"));
        assert!(!allows("npm install"));
    }

    #[test]
    fn setiap_bagian_perintah_gabungan_harus_cocok() {
        assert!(allows(
            r#"cd kelola-web && git diff -- "src/app/(app)/x.tsx""#
        ));
        assert!(allows(
            r#"npx vitest run "src/app/(app)/x.test.tsx" 2>&1 | tail -30"#
        ));
        assert!(!allows("cd kelola-web && git push"));
        assert!(
            parse("git diff | sh").is_err(),
            "pipe ke shell ditolak pengurai"
        );
    }

    #[test]
    fn kutipan_tidak_menyamarkan_batas_kata() {
        // `git "diff --output=x"` bukan `git diff …`.
        assert!(!allows(r#"git "diff --output=x""#));
        assert!(!allows(r#""git diff" --stat"#));
        assert!(!allows("cd"), "`cd *` butuh argumen");
        let colon = BashAllowlist::new(&["Bash(git diff:*)"]);
        assert!(!colon.allows_all(&parse("git difftool").unwrap()));
        assert!(colon.allows_all(&parse("git diff --stat").unwrap()));
        let partial = BashAllowlist::new(&["Bash(git diff*)"]);
        assert!(partial.allows_all(&parse("git difftool").unwrap()));
        assert!(BashAllowlist::new(&["Bash(*)"]).patterns.is_empty());
    }

    #[test]
    fn pola_non_bash_tidak_berlaku() {
        let only_read = BashAllowlist::new(&["Read", "Edit"]);
        assert!(!only_read.allows_all(&parse("ls").unwrap()));
        assert!(!list().allows_all(&[]));
    }
}
