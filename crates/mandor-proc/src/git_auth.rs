//! Kredensial git per proses lewat `GIT_CONFIG_COUNT`/`KEY_n`/`VALUE_n`,
//! sehingga token tidak pernah masuk argv (KTD-11).

use secrecy::{ExposeSecret, SecretString};

/// Bentuk URL remote GitHub yang ditulis ulang agar memakai token.
const REWRITTEN: &[&str] = &[
    "git@github.com:",
    "ssh://git@github.com/",
    "https://github.com/",
];

/// Variabel lingkungan yang membuat git memakai `token` untuk remote GitHub.
///
/// Hanya untuk proses fetch/push milik daemon; jangan pernah diberikan ke agent.
#[must_use]
pub fn git_credential_env(token: &SecretString) -> Vec<(String, String)> {
    let base = format!(
        "https://x-access-token:{}@github.com/",
        token.expose_secret()
    );
    let key = format!("url.{base}.insteadOf");
    let mut env = vec![("GIT_CONFIG_COUNT".to_owned(), REWRITTEN.len().to_string())];
    for (i, from) in REWRITTEN.iter().enumerate() {
        env.push((format!("GIT_CONFIG_KEY_{i}"), key.clone()));
        env.push((format!("GIT_CONFIG_VALUE_{i}"), (*from).to_owned()));
    }
    env.push(("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()));
    env
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn menulis_ulang_tiga_bentuk_remote_tanpa_argv() {
        let env = git_credential_env(&SecretString::from("ghs_rahasia".to_owned()));
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(get("GIT_CONFIG_COUNT"), "3");
        assert_eq!(
            get("GIT_CONFIG_KEY_0"),
            "url.https://x-access-token:ghs_rahasia@github.com/.insteadOf"
        );
        assert_eq!(get("GIT_CONFIG_VALUE_0"), "git@github.com:");
        assert_eq!(get("GIT_CONFIG_VALUE_1"), "ssh://git@github.com/");
        assert_eq!(get("GIT_CONFIG_VALUE_2"), "https://github.com/");
        assert_eq!(get("GIT_TERMINAL_PROMPT"), "0");
    }
}
