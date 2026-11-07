//! Pengurai perintah Bash yang ketat untuk izin eksplisit (KTD-A2 plan
//! 2026-11-06-001).
//!
//! Hasilnya daftar perintah sederhana (kata dan redirect) atau
//! [`Unparseable`]. Setiap bentuk yang bisa menjalankan program lain atau
//! mengubah arti kata saat dijalankan shell ditolak, supaya izin eksplisit
//! hanya diberikan untuk perintah yang programnya terlihat semua.

/// Perintah yang tidak bisa diurai dengan pasti; alasannya untuk dev.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Unparseable(pub String);

/// Satu perintah sederhana: program, argumen, dan redirect berkasnya.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SimpleCommand {
    /// Program lalu argumen, tanpa kutip.
    pub words: Vec<String>,
    /// Redirect ke atau dari berkas (`2>&1` tidak dicatat).
    pub redirects: Vec<Redirect>,
}

/// Redirect ke atau dari berkas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    pub target: String,
    /// `true` untuk `>`, `>>`, `&>`; `false` untuk `<`.
    pub write: bool,
}

/// Program yang menjalankan perintah atau kode lain dari argumennya.
const WRAPPERS: &[&str] = &[
    "eval", "exec", "source", ".", "sh", "bash", "zsh", "dash", "ksh", "fish", "xargs", "env",
    "sudo", "doas", "nohup", "nice", "timeout", "command", "builtin", "time", "watch", "parallel",
];

fn reject<T>(reason: &str) -> Result<T, Unparseable> {
    Err(Unparseable(reason.to_owned()))
}

/// Mengurai `cmd` menjadi perintah sederhana yang dipisah `&&`, `||`, `;`,
/// `|`, atau baris baru.
///
/// # Errors
/// [`Unparseable`] untuk substitusi, ekspansi, pembungkus, here-doc,
/// assignment di depan perintah, kutipan tak tertutup, dan sintaks lain yang
/// tidak dikenali.
pub fn parse(cmd: &str) -> Result<Vec<SimpleCommand>, Unparseable> {
    let commands = Lexer::new(cmd).run()?;
    for c in &commands {
        let Some(program) = c.words.first() else {
            return reject("empty command");
        };
        let name = program.rsplit('/').next().unwrap_or(program);
        if WRAPPERS.contains(&name) {
            return reject(&format!("`{name}` runs another command"));
        }
    }
    Ok(commands)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

/// Kata yang sedang dibangun dan apakah ia assignment `NAME=…`.
#[derive(Default)]
struct Word {
    text: String,
    /// Semua karakter sejauh ini tidak dikutip (untuk deteksi assignment).
    plain_prefix: bool,
    assignment: bool,
    started: bool,
}

impl Word {
    fn fresh() -> Self {
        Self {
            plain_prefix: true,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    None,
    /// Kata berikutnya adalah target redirect.
    Target {
        write: bool,
    },
}

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    commands: Vec<SimpleCommand>,
    current: SimpleCommand,
    word: Word,
    pending: Pending,
    /// Pemisah terakhir adalah `&&`, `||`, atau `|` dan belum diikuti perintah.
    after_hard: bool,
}

impl<'a> Lexer<'a> {
    fn new(cmd: &'a str) -> Self {
        Self {
            chars: cmd.chars().peekable(),
            commands: Vec::new(),
            current: SimpleCommand::default(),
            word: Word::fresh(),
            pending: Pending::None,
            after_hard: false,
        }
    }

    fn run(mut self) -> Result<Vec<SimpleCommand>, Unparseable> {
        let mut quote = Quote::None;
        while let Some(c) = self.chars.next() {
            if c.is_control() && c != '\t' && c != '\n' {
                return reject("control character in command");
            }
            match quote {
                Quote::Single => {
                    if c == '\'' {
                        quote = Quote::None;
                    } else {
                        self.word.text.push(c);
                    }
                }
                Quote::Double => quote = self.double(c)?,
                Quote::None => quote = self.normal(c)?,
            }
        }
        if quote != Quote::None {
            return reject("unclosed quote");
        }
        self.finish()?;
        Ok(self.commands)
    }

    fn double(&mut self, c: char) -> Result<Quote, Unparseable> {
        match c {
            '"' => return Ok(Quote::None),
            '$' | '`' => return reject("expansion inside double quotes"),
            '\\' => match self.chars.next() {
                Some(n @ ('"' | '\\' | '$' | '`')) => self.word.text.push(n),
                Some('\n') => {}
                Some(n) => {
                    self.word.text.push('\\');
                    self.word.text.push(n);
                }
                None => return reject("unclosed quote"),
            },
            _ => self.word.text.push(c),
        }
        Ok(Quote::Double)
    }

    fn normal(&mut self, c: char) -> Result<Quote, Unparseable> {
        match c {
            ' ' | '\t' => self.end_word()?,
            '\n' | ';' => self.separator(false)?,
            '\'' => {
                self.mark_quoted();
                return Ok(Quote::Single);
            }
            '"' => {
                self.mark_quoted();
                return Ok(Quote::Double);
            }
            '\\' => match self.chars.next() {
                Some('\n') => {}
                Some(n) => {
                    self.mark_quoted();
                    self.word.text.push(n);
                }
                None => return reject("trailing backslash"),
            },
            '&' => {
                if self.chars.peek() == Some(&'&') {
                    self.chars.next();
                    self.separator(true)?;
                } else if self.chars.peek() == Some(&'>') {
                    self.chars.next();
                    if self.chars.peek() == Some(&'>') {
                        self.chars.next();
                    }
                    self.redirect(true)?;
                } else {
                    return reject("background or unknown `&`");
                }
            }
            '|' => {
                if self.chars.peek() == Some(&'|') {
                    self.chars.next();
                } else if self.chars.peek() == Some(&'&') {
                    return reject("`|&` is not supported");
                }
                self.separator(true)?;
            }
            '>' => self.write_redirect()?,
            '<' => match self.chars.peek() {
                Some('<') => return reject("here-doc or here-string"),
                Some('(') => return reject("process substitution"),
                Some('&') => return reject("descriptor duplication on input"),
                _ => self.redirect(false)?,
            },
            '$' => return reject("variable or command expansion"),
            '`' => return reject("command substitution"),
            '(' | ')' => return reject("subshell or grouping"),
            '{' | '}' => return reject("brace expansion or grouping"),
            '*' | '?' | '[' | ']' => return reject("glob pattern"),
            // Tilde hanya diekspansi di awal kata atau setelah `=`/`:`;
            // `HEAD~3` adalah teks biasa.
            '~' if !self.word.started || self.word.text.ends_with(['=', ':']) => {
                return reject("tilde expansion");
            }
            '#' if !self.word.started => return reject("comment"),
            '=' => {
                if self.word.plain_prefix
                    && self.current.words.is_empty()
                    && self.pending == Pending::None
                    && is_name(&self.word.text)
                {
                    self.word.assignment = true;
                }
                self.push_plain(c);
            }
            _ => {
                if c.is_ascii_digit()
                    && !self.word.started
                    && matches!(self.chars.peek(), Some('>' | '<'))
                {
                    // Descriptor sebelum redirect, mis. `2>` atau `2>&1`.
                    let op = self.chars.next();
                    if op == Some('<') {
                        return reject("descriptor redirect on input");
                    }
                    return self.write_redirect().map(|()| Quote::None);
                }
                self.push_plain(c);
            }
        }
        Ok(Quote::None)
    }

    fn push_plain(&mut self, c: char) {
        self.word.started = true;
        self.word.text.push(c);
    }

    fn mark_quoted(&mut self) {
        // Kata kosong berkutip (`''`) tetap kata.
        self.word.started = true;
        if !self.word.text.contains('=') {
            self.word.plain_prefix = false;
        }
    }

    /// `>` atau `>>`, termasuk `>&N` yang hanya menduplikasi descriptor.
    fn write_redirect(&mut self) -> Result<(), Unparseable> {
        self.end_word()?;
        if self.chars.peek() == Some(&'>') {
            self.chars.next();
        }
        if self.chars.peek() == Some(&'&') {
            self.chars.next();
            let mut fd = String::new();
            while let Some(d) = self.chars.peek().copied().filter(char::is_ascii_digit) {
                fd.push(d);
                self.chars.next();
            }
            if fd.is_empty() {
                return reject("`>&` without a descriptor");
            }
            // `>&1x` di bash berarti redirect ke berkas `1x`, bukan descriptor.
            if self
                .chars
                .peek()
                .is_some_and(|c| !matches!(c, ' ' | '\t' | '\n' | ';' | '|' | '&' | '<' | '>'))
            {
                return reject("`>&` followed by a file name");
            }
            return Ok(());
        }
        if self.chars.peek() == Some(&'|') {
            return reject("`>|` is not supported");
        }
        self.redirect(true)
    }

    fn redirect(&mut self, write: bool) -> Result<(), Unparseable> {
        self.end_word()?;
        if self.pending != Pending::None {
            return reject("redirect without a target");
        }
        self.pending = Pending::Target { write };
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), Unparseable> {
        let word = std::mem::replace(&mut self.word, Word::fresh());
        if !word.started {
            return Ok(());
        }
        if word.assignment {
            return reject("environment assignment before a command");
        }
        match self.pending {
            Pending::Target { write } => {
                self.current.redirects.push(Redirect {
                    target: word.text,
                    write,
                });
                self.pending = Pending::None;
            }
            Pending::None => self.current.words.push(word.text),
        }
        Ok(())
    }

    /// Pemisah perintah; `hard` untuk `&&`, `||`, `|` yang wajib diikuti
    /// perintah, lunak untuk `;` dan baris baru.
    fn separator(&mut self, hard: bool) -> Result<(), Unparseable> {
        self.end_word()?;
        if self.pending != Pending::None {
            return reject("redirect without a target");
        }
        let command = std::mem::take(&mut self.current);
        if command.words.is_empty() {
            if !command.redirects.is_empty() {
                return reject("redirect without a command");
            }
            if hard {
                return reject("operator without a command");
            }
            return Ok(());
        }
        self.commands.push(command);
        self.after_hard = hard;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Unparseable> {
        self.end_word()?;
        if self.pending != Pending::None {
            return reject("redirect without a target");
        }
        let command = std::mem::take(&mut self.current);
        if command.words.is_empty() {
            if !command.redirects.is_empty() {
                return reject("redirect without a command");
            }
            if self.after_hard {
                return reject("operator without a following command");
            }
            if self.commands.is_empty() {
                return reject("empty command");
            }
            return Ok(());
        }
        self.commands.push(command);
        self.after_hard = false;
        Ok(())
    }
}

fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use pretty_assertions::assert_eq;

    fn words(cmd: &str) -> Vec<Vec<String>> {
        parse(cmd)
            .unwrap_or_else(|e| panic!("{cmd}: {e}"))
            .into_iter()
            .map(|c| c.words)
            .collect()
    }

    #[test]
    fn perintah_gabungan_dengan_path_berkurung_dalam_kutipan() {
        assert_eq!(
            words(r#"cd kelola-web && git diff -- "src/app/(app)/x.tsx""#),
            [
                vec!["cd", "kelola-web"],
                vec!["git", "diff", "--", "src/app/(app)/x.tsx"]
            ]
        );
    }

    #[test]
    fn redirect_descriptor_bukan_target_berkas() {
        let parsed = parse(r#"npx vitest run "src/app/(app)/x.test.tsx" 2>&1 | tail -30"#).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0].words,
            ["npx", "vitest", "run", "src/app/(app)/x.test.tsx"]
        );
        assert!(parsed[0].redirects.is_empty());
        assert_eq!(parsed[1].words, ["tail", "-30"]);
    }

    #[test]
    fn target_redirect_berkas_dicatat() {
        let parsed = parse("echo x > .githooks/pre-push; cat < in.txt >> out.log").unwrap();
        assert_eq!(
            parsed[0].redirects,
            [Redirect {
                target: ".githooks/pre-push".to_owned(),
                write: true
            }]
        );
        assert_eq!(
            parsed[1].redirects,
            [
                Redirect {
                    target: "in.txt".to_owned(),
                    write: false
                },
                Redirect {
                    target: "out.log".to_owned(),
                    write: true
                }
            ]
        );
        assert!(
            parse("echo x >&1x").is_err(),
            "berkas tersembunyi di balik >&"
        );
        assert!(parse("echo x >&2; echo y 2>&1|tail").is_ok());
        let all = parse("make 2> err.txt &> both.txt").unwrap();
        assert_eq!(all[0].words, ["make"]);
        assert_eq!(all[0].redirects.len(), 2);
    }

    #[test]
    fn kutip_tunggal_membuat_karakter_khusus_literal() {
        assert_eq!(words("cat '$HOME'"), [vec!["cat", "$HOME"]]);
        assert_eq!(words("grep 'a*b' f.txt"), [vec!["grep", "a*b", "f.txt"]]);
        assert_eq!(words(r#"echo "a b" c\ d"#), [vec!["echo", "a b", "c d"]]);
        assert_eq!(
            words("git log --format=%H"),
            [vec!["git", "log", "--format=%H"]]
        );
    }

    #[test]
    fn bentuk_berbahaya_tidak_bisa_diurai() {
        for cmd in [
            "echo $(docker ps)",
            "echo `id`",
            "diff <(curl x) a",
            "eval \"x\"",
            "bash -c \"ls\"",
            "sh -c ls",
            "/bin/bash script.sh",
            "find . | xargs rm",
            "env FOO=1 ls",
            "cat <<EOF",
            "cat <<<'x'",
            "GIT_SSH_COMMAND=x git diff",
            "FOO=\"x\" ls",
            "cat ~/.aws/credentials",
            "cat .gi[t]/config",
            "ls *.rs",
            "cd $X && git status",
            "cat \"${HOME}/x\"",
            "echo $'\\x41'",
            "ls {a,b}",
            "(cd a; ls)",
            "echo 'unterminated",
            "echo \"unterminated",
            "echo a\u{7}b",
            "sleep 1 &",
            "ls && && ls",
            "ls &&",
            "| ls",
            "> out",
            "echo x >",
            "# komentar",
            "sudo ls",
            "timeout 5 ls",
        ] {
            assert!(parse(cmd).is_err(), "{cmd:?} should be unparseable");
        }
    }

    #[test]
    fn tilde_di_tengah_kata_adalah_teks() {
        assert_eq!(words("git log HEAD~3"), [vec!["git", "log", "HEAD~3"]]);
        assert!(parse("ls ~").is_err());
        assert!(parse("cat --file=~/x").is_err());
        assert!(parse("cat a:~/x").is_err());
    }

    #[test]
    fn assignment_setelah_program_adalah_argumen_biasa() {
        assert_eq!(
            words("git config --get user.name=x"),
            [vec!["git", "config", "--get", "user.name=x"]]
        );
        assert_eq!(words("make TARGET=a"), [vec!["make", "TARGET=a"]]);
    }

    #[test]
    fn baris_baru_dan_titik_koma_memisah_perintah() {
        assert_eq!(words("ls\npwd;"), [vec!["ls"], vec!["pwd"]]);
        assert_eq!(words("a || b | c"), [vec!["a"], vec!["b"], vec!["c"]]);
    }
}
