//! Retry dengan backoff eksponensial + jitter (helper kecil di repo, tanpa crate).

use std::future::Future;
use std::time::Duration;

use rand::RngExt;

use crate::LarkError;

/// Kebijakan retry.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub attempts: u32,
    pub base: Duration,
    pub max: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: 4,
            base: Duration::from_millis(600),
            max: Duration::from_secs(10),
        }
    }
}

impl RetryPolicy {
    /// Jeda sebelum percobaan ke-`attempt` (mulai 1), dengan jitter ±25%.
    #[must_use]
    pub fn delay(&self, attempt: u32) -> Duration {
        let exp = self
            .base
            .saturating_mul(2u32.saturating_pow(attempt.saturating_sub(1)));
        let capped = exp.min(self.max);
        let jitter: f64 = rand::rng().random_range(0.75..1.25);
        capped.mul_f64(jitter)
    }
}

/// Menjalankan `op` sampai berhasil, error tidak bisa dicoba ulang, atau percobaan habis.
///
/// # Errors
/// Error terakhir dari `op`.
pub async fn with_retry<T, F, Fut>(policy: RetryPolicy, mut op: F) -> Result<T, LarkError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, LarkError>>,
{
    let mut attempt = 1;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if e.is_retryable() && attempt < policy.attempts => {
                tracing::debug!(attempt, error = %e, "mencoba ulang permintaan Lark");
                tokio::time::sleep(policy.delay(attempt)).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn fast() -> RetryPolicy {
        RetryPolicy {
            attempts: 3,
            base: Duration::from_millis(1),
            max: Duration::from_millis(2),
        }
    }

    #[tokio::test]
    async fn mencoba_ulang_error_sementara_lalu_berhasil() {
        let calls = AtomicU32::new(0);
        let out = with_retry(fast(), || async {
            if calls.fetch_add(1, Ordering::SeqCst) < 2 {
                Err(LarkError::WriteConflict)
            } else {
                Ok(7)
            }
        })
        .await
        .unwrap();
        assert_eq!(out, 7);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn tidak_mencoba_ulang_reauth() {
        let calls = AtomicU32::new(0);
        let out: Result<(), _> = with_retry(fast(), || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(LarkError::ReauthRequired)
        })
        .await;
        assert!(matches!(out, Err(LarkError::ReauthRequired)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn jeda_dibatasi_maksimum() {
        let p = RetryPolicy {
            attempts: 10,
            base: Duration::from_millis(100),
            max: Duration::from_millis(400),
        };
        assert!(p.delay(8) <= Duration::from_millis(500));
        assert!(p.delay(1) >= Duration::from_millis(75));
    }
}
