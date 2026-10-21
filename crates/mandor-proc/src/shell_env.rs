//! Menangkap environment login shell pengguna (PATH, SSH agent, dsb.) sekali
//! saat daemon start, agar proses anak menemukan `git`, `gh`, dan `claude`
//! seperti di terminal pengguna.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use crate::cmd::{Cmd, ProcError, run};

const DELIM: &str = "__MANDOR_ENV_DELIM__";

/// Kegagalan menangkap environment login shell.
#[derive(Debug, thiserror::Error)]
pub enum ShellEnvError {
    #[error(transparent)]
    Proc(#[from] ProcError),
    #[error("shell produced no environment block (code {code:?}): {stderr}")]
    NoOutput { code: Option<i32>, stderr: String },
}

/// Menjalankan login shell dan mengembalikan environment-nya.
///
/// Mendukung bash, zsh, dan fish (`--login --interactive`), tanpa hardcode zsh.
///
/// # Errors
/// [`ShellEnvError::Proc`] bila shell gagal dijalankan atau melewati `timeout`;
/// [`ShellEnvError::NoOutput`] bila output tidak memuat blok environment.
pub async fn capture_login_env(
    shell: &Path,
    timeout: Duration,
) -> Result<BTreeMap<String, String>, ShellEnvError> {
    let script = format!("echo {DELIM}; env; echo {DELIM}");
    let is_fish = shell
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("fish"));
    let cmd = if is_fish {
        Cmd::new(shell).args(["--login", "--interactive", "-c", &script])
    } else {
        Cmd::new(shell).args(["-l", "-i", "-c", &script])
    };
    let out = run(&cmd.timeout(timeout)).await?;
    let vars = parse_env_block(&out.stdout);
    if vars.is_empty() {
        return Err(ShellEnvError::NoOutput {
            code: out.code,
            stderr: out.stderr,
        });
    }
    Ok(vars)
}

/// Mengurai output `env` di antara dua baris pembatas. Pembatas harus berupa
/// baris utuh, karena variabel `_` bisa berisi teks pembatas. Baris yang tidak
/// berbentuk `KEY=VALUE` dianggap lanjutan nilai sebelumnya (nilai multi-baris).
#[must_use]
pub fn parse_env_block(stdout: &str) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    let mut lines = stdout.lines().skip_while(|l| l.trim_end() != DELIM).skip(1);
    let mut last: Option<String> = None;
    for line in lines.by_ref() {
        if line.trim_end() == DELIM {
            break;
        }
        match split_assignment(line) {
            Some((k, v)) => {
                vars.insert(k.to_owned(), v.to_owned());
                last = Some(k.to_owned());
            }
            None => {
                if let Some(value) = last.as_ref().and_then(|k| vars.get_mut(k)) {
                    value.push('\n');
                    value.push_str(line);
                }
            }
        }
    }
    vars
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    let valid = !key.is_empty()
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !key.starts_with(|c: char| c.is_ascii_digit());
    valid.then_some((key, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn mengurai_blok_env_dengan_noise_dan_multibaris() {
        let out = format!(
            "motd banner\n{DELIM}\nPATH=/usr/bin:/bin\n_={DELIM}\nMULTI=baris1\nbaris2\nEMPTY=\n{DELIM}\nsesudah=diabaikan\n"
        );
        let env = parse_env_block(&out);
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin:/bin"));
        assert_eq!(env.get("MULTI").map(String::as_str), Some("baris1\nbaris2"));
        assert_eq!(env.get("EMPTY").map(String::as_str), Some(""));
        assert!(!env.contains_key("sesudah"));
        assert!(!env.contains_key("motd banner"));
    }

    #[test]
    fn tanpa_pembatas_menghasilkan_kosong() {
        assert!(parse_env_block("PATH=/bin\n").is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn menangkap_env_dari_sh_sungguhan() {
        let env = capture_login_env(Path::new("/bin/sh"), Duration::from_secs(10))
            .await
            .unwrap();
        assert!(env.contains_key("PATH"), "{env:?}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shell_yang_menggantung_dibatasi_timeout() {
        // Program yang mengabaikan argumen shell dan tidur lama.
        let err = capture_login_env(Path::new("/bin/sleep"), Duration::from_millis(200)).await;
        assert!(err.is_err());
    }
}
