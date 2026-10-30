//! Menjalankan satu proses sampai selesai, dengan timeout dan process group.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use secrecy::SecretString;

use crate::redact::redact;

/// Timeout default untuk proses pendek (git, gh).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Variabel lingkungan berisi token yang tidak boleh diteruskan ke proses anak
/// bila [`Cmd::scrub_tokens`] dipakai.
const TOKEN_VARS: &[&str] = &[
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
];

/// Prefiks variabel lingkungan yang selalu dibuang oleh [`Cmd::scrub_tokens`].
const TOKEN_VAR_PREFIXES: &[&str] = &["LARK_", "MANDOR_SECRET"];

/// Cara membangun environment proses anak.
#[derive(Debug, Clone, Default)]
pub enum EnvPolicy {
    /// Mewarisi environment daemon.
    #[default]
    Inherit,
    /// Hanya variabel yang diberikan (environment lain dibersihkan).
    Explicit(Vec<(String, String)>),
}

/// Deskripsi satu proses yang akan dijalankan.
#[derive(Debug, Clone)]
pub struct Cmd {
    pub(crate) program: OsString,
    pub(crate) args: Vec<OsString>,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) env: EnvPolicy,
    pub(crate) extra_env: Vec<(String, String)>,
    pub(crate) scrub: bool,
    pub(crate) stdin: Option<Vec<u8>>,
    pub(crate) timeout: Duration,
}

impl Cmd {
    /// Membuat perintah baru dengan timeout default 60 detik.
    #[must_use]
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            args: Vec::new(),
            cwd: None,
            env: EnvPolicy::Inherit,
            extra_env: Vec::new(),
            scrub: false,
            stdin: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Menambah satu argumen.
    #[must_use]
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }

    /// Menambah beberapa argumen.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|a| a.as_ref().to_owned()));
        self
    }

    /// Direktori kerja proses.
    #[must_use]
    pub fn cwd(mut self, dir: impl AsRef<Path>) -> Self {
        self.cwd = Some(dir.as_ref().to_owned());
        self
    }

    /// Kebijakan environment dasar.
    #[must_use]
    pub fn env_policy(mut self, policy: EnvPolicy) -> Self {
        self.env = policy;
        self
    }

    /// Menambah satu variabel lingkungan di atas kebijakan dasar.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_env.push((key.into(), value.into()));
        self
    }

    /// Membuang variabel lingkungan warisan berisi token (GitHub, Lark) dari proses
    /// anak. Variabel yang disetel eksplisit dengan [`Self::env`] tidak dibuang.
    #[must_use]
    pub fn scrub_tokens(mut self) -> Self {
        self.scrub = true;
        self
    }

    /// Data yang ditulis ke stdin lalu ditutup.
    #[must_use]
    pub fn stdin_bytes(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(data.into());
        self
    }

    /// Batas waktu total proses.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Direktori kerja perintah, bila diatur.
    #[must_use]
    pub fn cwd_path(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    /// Nama program untuk pesan error dan log.
    #[must_use]
    pub fn program_name(&self) -> String {
        self.program.to_string_lossy().into_owned()
    }

    /// Argumen perintah sebagai teks, untuk log dan test.
    #[must_use]
    pub fn arg_list(&self) -> Vec<String> {
        self.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    pub(crate) fn build(&self) -> CommandWrap {
        let mut wrap = CommandWrap::with_new(&self.program, |c| {
            c.args(&self.args);
            if let Some(dir) = &self.cwd {
                c.current_dir(dir);
            }
            match &self.env {
                EnvPolicy::Inherit => {}
                EnvPolicy::Explicit(vars) => {
                    c.env_clear();
                    c.envs(vars.iter().map(|(k, v)| (k, v)));
                }
            }
            // Pembersihan hanya untuk variabel warisan; variabel yang disetel
            // eksplisit lewat `env()` (mis. GH_TOKEN milik daemon) tetap dipakai.
            if self.scrub {
                for key in scrub_keys(&self.env) {
                    c.env_remove(key);
                }
            }
            c.envs(self.extra_env.iter().map(|(k, v)| (k, v)));
            c.stdin(if self.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });
            c.stdout(Stdio::piped());
            c.stderr(Stdio::piped());
        });
        #[cfg(unix)]
        wrap.wrap(process_wrap::tokio::ProcessGroup::leader());
        #[cfg(windows)]
        wrap.wrap(process_wrap::tokio::JobObject);
        wrap.wrap(KillOnDrop);
        wrap
    }
}

fn scrub_keys(base: &EnvPolicy) -> Vec<String> {
    let mut keys: Vec<String> = TOKEN_VARS.iter().map(|k| (*k).to_owned()).collect();
    let inherited: Vec<String> = match base {
        EnvPolicy::Inherit => std::env::vars_os()
            .filter_map(|(k, _)| k.into_string().ok())
            .collect(),
        EnvPolicy::Explicit(vars) => vars.iter().map(|(k, _)| k.clone()).collect(),
    };
    keys.extend(
        inherited
            .into_iter()
            .filter(|k| TOKEN_VAR_PREFIXES.iter().any(|p| k.starts_with(p))),
    );
    keys
}

/// Hasil proses yang sudah selesai; stdout dan stderr sudah diredaksi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Kode keluar; `None` bila proses dihentikan sinyal.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// `true` bila kode keluar 0.
    #[must_use]
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Kegagalan menjalankan proses.
#[derive(Debug, thiserror::Error)]
pub enum ProcError {
    #[error("failed to run {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{program} exceeded the {after:?} timeout and was stopped")]
    Timeout { program: String, after: Duration },
    #[error("I/O with {program} failed: {source}")]
    Io {
        program: String,
        #[source]
        source: std::io::Error,
    },
}

/// Menjalankan proses sampai selesai atau sampai timeout.
///
/// Saat timeout, seluruh process group dihentikan (SIGKILL / Job Object).
///
/// # Errors
/// [`ProcError::Spawn`] bila program tidak bisa dijalankan, [`ProcError::Timeout`]
/// bila melewati batas waktu, [`ProcError::Io`] bila pipe gagal.
pub async fn run(cmd: &Cmd) -> Result<Output, ProcError> {
    let (code, out, err) = run_raw(cmd).await?;
    Ok(Output {
        code,
        stdout: redact(&String::from_utf8_lossy(&out)).into_owned(),
        stderr: redact(&String::from_utf8_lossy(&err)).into_owned(),
    })
}

/// Hasil [`run_secret`]: stdout adalah rahasia, stderr tetap diredaksi.
#[derive(Debug)]
pub struct SecretOutput {
    /// Kode keluar; `None` bila proses dihentikan sinyal.
    pub code: Option<i32>,
    pub stdout: SecretString,
    pub stderr: String,
}

impl SecretOutput {
    /// `true` bila kode keluar 0.
    #[must_use]
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Seperti [`run`], tetapi stdout tidak diredaksi dan dikembalikan sebagai
/// rahasia, untuk perintah yang memang mencetak token (mis. `gh auth token`).
///
/// # Errors
/// Sama dengan [`run`].
pub async fn run_secret(cmd: &Cmd) -> Result<SecretOutput, ProcError> {
    let (code, out, err) = run_raw(cmd).await?;
    Ok(SecretOutput {
        code,
        stdout: SecretString::from(String::from_utf8_lossy(&out).into_owned()),
        stderr: redact(&String::from_utf8_lossy(&err)).into_owned(),
    })
}

async fn run_raw(cmd: &Cmd) -> Result<(Option<i32>, Vec<u8>, Vec<u8>), ProcError> {
    let program = cmd.program_name();
    let mut child = cmd.build().spawn().map_err(|source| ProcError::Spawn {
        program: program.clone(),
        source,
    })?;

    let stdin = child.stdin().take();
    let stdout = child.stdout().take();
    let stderr = child.stderr().take();
    let input = cmd.stdin.clone();

    let outcome = tokio::time::timeout(cmd.timeout, async {
        let write = async {
            if let (Some(mut pipe), Some(data)) = (stdin, input) {
                pipe.write_all(&data).await?;
                pipe.shutdown().await?;
            }
            Ok::<(), std::io::Error>(())
        };
        let (w, out, err, status) =
            tokio::join!(write, read_all(stdout), read_all(stderr), child.wait());
        w?;
        Ok::<_, std::io::Error>((out?, err?, status?))
    })
    .await;

    match outcome {
        Ok(Ok((out, err, status))) => Ok((status.code(), out, err)),
        Ok(Err(source)) => Err(ProcError::Io { program, source }),
        Err(_elapsed) => {
            kill_group(child.as_mut()).await;
            Err(ProcError::Timeout {
                program,
                after: cmd.timeout,
            })
        }
    }
}

async fn read_all<R: AsyncRead + Unpin>(pipe: Option<R>) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    if let Some(mut p) = pipe {
        p.read_to_end(&mut buf).await?;
    }
    Ok(buf)
}

pub(crate) async fn kill_group(child: &mut dyn ChildWrapper) {
    if let Err(err) = child.start_kill() {
        tracing::debug!(error = %err, "start_kill gagal (proses mungkin sudah selesai)");
    }
    if let Err(err) = Box::into_pin(child.kill()).await {
        tracing::debug!(error = %err, "menunggu proses yang dihentikan gagal");
    }
}
