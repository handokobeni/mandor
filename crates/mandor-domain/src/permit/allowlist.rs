//! Pencocokan perintah sederhana dengan allowlist giliran, meniru semantik
//! `Bash(prefix…)` Claude Code (KTD-A5 plan 2026-11-06-001).

use super::shell::SimpleCommand;

/// Satu pola Bash dari allowlist.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pattern {
    /// `Bash(git diff*)` atau `Bash(npm run test:*)`.
    Prefix(String),
    /// `Bash(pwd)`.
    Exact(String),
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
                let pattern = if let Some(p) = inner.strip_suffix(":*") {
                    Pattern::Prefix(p.to_owned())
                } else if let Some(p) = inner.strip_suffix('*') {
                    Pattern::Prefix(p.to_owned())
                } else {
                    Pattern::Exact(inner.to_owned())
                };
                Some(pattern)
            })
            .collect();
        Self { patterns }
    }

    /// Perintah sederhana cocok dengan salah satu pola.
    #[must_use]
    pub fn allows(&self, command: &SimpleCommand) -> bool {
        let text = command.words.join(" ");
        self.patterns.iter().any(|p| match p {
            Pattern::Prefix(prefix) => text.starts_with(prefix.as_str()),
            Pattern::Exact(exact) => text == *exact,
        })
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
    fn pola_non_bash_tidak_berlaku() {
        let only_read = BashAllowlist::new(&["Read", "Edit"]);
        assert!(!only_read.allows_all(&parse("ls").unwrap()));
        assert!(!list().allows_all(&[]));
    }
}
