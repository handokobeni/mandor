//! Argumen berbahaya yang membatalkan izin otomatis (KTD-A3 plan
//! 2026-11-06-001): argumen yang bisa menjalankan kode atau menulis berkas di
//! luar pemeriksaan path. Hasilnya tanpa keputusan (kartu), bukan tolak.

use super::shell::SimpleCommand;
use crate::hook::program_name;
use crate::hook_review::{dangerous_git_opt, node_runs_code, rg_runs_program};

/// Alasan argumen perintah sederhana ini berbahaya, bila ada.
#[must_use]
pub fn dangerous(command: &SimpleCommand) -> Option<String> {
    let (first, args) = command.words.split_first()?;
    let prog = program_name(first).to_ascii_lowercase();
    match prog.as_str() {
        "awk" | "gawk" | "mawk" | "nawk" => awk(args),
        "sed" | "gsed" => sed(args),
        "find" => find(args),
        "git" => args
            .iter()
            .find(|a| dangerous_git_opt(a))
            .map(|o| format!("`git {o}` can run other programs or write files")),
        "tee" => Some("`tee` writes files".to_owned()),
        "sort" => sort(args),
        "node" if node_runs_code(args) => {
            Some("`node` options other than `--test <path…>` run code".to_owned())
        }
        "rg" if rg_runs_program(args) => Some("this `rg` option runs another program".to_owned()),
        _ => None,
    }
}

const AWK_FLAGS: &[&str] = &[
    "-f",
    "--file",
    "-i",
    "--include",
    "-l",
    "--load",
    "-E",
    "--exec",
];
const AWK_MARKERS: &[&str] = &["system", "|", ">", "getline"];

fn awk(args: &[String]) -> Option<String> {
    let flagged = args.iter().any(|a| {
        AWK_FLAGS
            .iter()
            .any(|f| a == f || (f.starts_with("--") && a.starts_with(&format!("{f}="))))
    });
    let runs = args
        .iter()
        .any(|a| AWK_MARKERS.iter().any(|m| a.contains(m)));
    (flagged || runs).then(|| "`awk` can run commands or write files".to_owned())
}

const FIND_ACTIONS: &[&str] = &[
    "-exec", "-execdir", "-ok", "-okdir", "-delete", "-fprint", "-fprint0", "-fprintf", "-fls",
];

fn find(args: &[String]) -> Option<String> {
    args.iter()
        .find(|a| FIND_ACTIONS.contains(&a.as_str()))
        .map(|a| format!("`find {a}` runs commands or changes files"))
}

fn sort(args: &[String]) -> Option<String> {
    args.iter()
        .any(|a| {
            a.starts_with("--output")
                || a.starts_with("--compress-program")
                || (a.starts_with('-') && !a.starts_with("--") && a.contains('o'))
        })
        .then(|| "`sort -o` writes files".to_owned())
}

/// Flag `sed` yang tidak mengubah apa yang dijalankan script.
const SED_PLAIN_FLAGS: &[&str] = &[
    "-n",
    "--quiet",
    "--silent",
    "-E",
    "-r",
    "--regexp-extended",
    "-s",
    "--separate",
    "-z",
    "--null-data",
    "-u",
    "--unbuffered",
    "--posix",
    "--debug",
];

fn sed(args: &[String]) -> Option<String> {
    let mut scripts: Vec<&str> = Vec::new();
    let mut explicit = false;
    let mut positional = false;
    let mut files_only = false;
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let a = a.as_str();
        i += 1;
        if files_only || !a.starts_with('-') || a == "-" {
            if !explicit && !positional {
                scripts.push(a);
            }
            positional = true;
            continue;
        }
        if a == "--" {
            files_only = true;
        } else if a == "-e"
            || a == "--expression"
            || (a.len() > 2
                && !a.starts_with("--")
                && a.ends_with('e')
                && a[1..a.len() - 1].chars().all(|c| "nErszu".contains(c)))
        {
            explicit = true;
            scripts.extend(args.get(i).map(String::as_str));
            i += 1;
        } else if let Some(s) = a.strip_prefix("--expression=") {
            explicit = true;
            scripts.push(s);
        } else if a.starts_with("-i") || a.starts_with("--in-place") {
            return Some("`sed -i` edits files in place".to_owned());
        } else if !SED_PLAIN_FLAGS.contains(&a) && !plain_short_flags(a) {
            return Some(format!("`sed {a}` is not checked for safety"));
        }
    }
    scripts.into_iter().find_map(sed_script)
}

/// Gabungan flag pendek `sed` yang aman, mis. `-nE`.
fn plain_short_flags(a: &str) -> bool {
    a.len() > 2 && !a.starts_with("--") && a[1..].chars().all(|c| "nErszu".contains(c))
}

/// Perintah `sed` yang aman; `e`, `w`, `W`, `r`, `R`, flag `s///e`/`w`, dan
/// perintah yang tidak dikenal berbahaya.
fn sed_script(script: &str) -> Option<String> {
    let danger = || Some("this `sed` script runs commands or reads/writes files".to_owned());
    let chars: Vec<char> = script.chars().collect();
    let mut i = 0;
    loop {
        while chars.get(i).is_some_and(|c| c.is_whitespace() || *c == ';') {
            i += 1;
        }
        if i >= chars.len() {
            return None;
        }
        i = sed_address(&chars, i)?;
        if chars.get(i) == Some(&',') {
            i = sed_address(&chars, i + 1)?;
        }
        while chars.get(i).is_some_and(|c| *c == ' ' || *c == '!') {
            i += 1;
        }
        let Some(&cmd) = chars.get(i) else {
            return danger();
        };
        i += 1;
        match cmd {
            '{' | '}' => {}
            'p' | 'd' | 'D' | 'q' | 'Q' | 'n' | 'N' | 'g' | 'G' | 'h' | 'H' | 'x' | 'l' | '='
            | 'P' | 'z' | 'F' => {
                while chars.get(i).is_some_and(char::is_ascii_digit) {
                    i += 1;
                }
            }
            's' | 'y' => {
                let delim = *chars.get(i)?;
                if delim == '\n' || delim == '\\' {
                    return danger();
                }
                i = sed_part(&chars, i + 1, delim)?;
                i = sed_part(&chars, i, delim)?;
                while let Some(&f) = chars.get(i) {
                    if f == 'e' || f == 'w' || f == 'W' {
                        return danger();
                    }
                    if !(f.is_ascii_digit() || "gpiImM".contains(f)) {
                        break;
                    }
                    i += 1;
                }
            }
            'a' | 'i' | 'c' | ':' | 'b' | 't' | 'T' => {
                while chars.get(i).is_some_and(|c| *c != '\n' && *c != ';') {
                    i += 1;
                }
            }
            _ => return danger(),
        }
    }
}

/// Melewati alamat `sed` (nomor, `$`, `/re/`, `\cREc`, langkah `~N`);
/// `None` bila tidak tertutup.
fn sed_address(chars: &[char], mut i: usize) -> Option<usize> {
    match chars.get(i) {
        Some('/') => sed_part(chars, i + 1, '/'),
        Some('\\') => {
            let delim = *chars.get(i + 1)?;
            sed_part(chars, i + 2, delim)
        }
        Some(c) if c.is_ascii_digit() || *c == '$' => {
            while chars
                .get(i)
                .is_some_and(|c| c.is_ascii_digit() || *c == '$' || *c == '~' || *c == '+')
            {
                i += 1;
            }
            Some(i)
        }
        _ => Some(i),
    }
}

/// Melewati satu bagian sampai pembatas yang tidak di-escape; mengembalikan
/// indeks setelah pembatas.
fn sed_part(chars: &[char], mut i: usize, delim: char) -> Option<usize> {
    while let Some(&c) = chars.get(i) {
        if c == '\\' {
            i += 2;
            continue;
        }
        if c == delim {
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::super::shell::parse;
    use super::*;

    fn danger(cmd: &str) -> bool {
        parse(cmd).unwrap().iter().any(|c| dangerous(c).is_some())
    }

    #[test]
    fn argumen_yang_menjalankan_kode_atau_menulis_berkas() {
        for cmd in [
            r#"awk 'BEGIN{system("x")}' f"#,
            "awk -f prog.awk f",
            "sed 'e ls' f",
            "sed -i s/a/b/ src/a.ts",
            "sed --in-place=.bak s/a/b/ f",
            "sed 's/a/b/w out' f",
            "sed 's/a/b/e' f",
            "sed -n 'w out' f",
            "sed '1r /etc/passwd' f",
            "sed -f script.sed f",
            r"find . -exec rm '{}' \;",
            "find . -delete",
            "find . -fprint out",
            "git -c core.pager=x log",
            "git diff --ext-diff",
            "git diff --output=x",
            "git --exec-path=/x log",
            "ls | tee out",
            "sort -o out f",
            "sort -uo out f",
            "node -e 1",
            "rg --pre cat x",
        ] {
            assert!(danger(cmd), "{cmd}");
        }
    }

    #[test]
    fn argumen_biasa_aman() {
        for cmd in [
            "awk '{print $1}' f",
            "sed -n '1,80p' f",
            "sed -n 1,80p f",
            "sed 's/a/b/g' f",
            "sed -E 's|x/y|z|2;/^#/d' f",
            "sed -ne '/foo/p' f",
            "sed -e 's/a/b/' -e '$d' f",
            "find . -name x -type f",
            "git log --oneline -5",
            "git diff --stat",
            "sort -u f",
            "node --test a.test.js",
            "rg -n foo src",
            "ls -la",
        ] {
            assert!(!danger(cmd), "{cmd}");
        }
    }
}
