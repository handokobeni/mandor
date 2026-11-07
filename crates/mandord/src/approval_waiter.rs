//! Penantian jawaban kartu persetujuan (KTD-A13 plan 2026-11-06-001):
//! channel tokio per kartu, tenggat, dan pembatalan per run.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use mandor_usecase::run_ports::{ApprovalAnswer, ApprovalWaiter, WaitOutcome};
use tokio::sync::oneshot;

struct Slot {
    run_id: String,
    tx: oneshot::Sender<WaitOutcome>,
}

/// Penunggu kartu milik daemon; state di memori dibangun ulang dari nol saat
/// restart karena kartu Pending dibatalkan saat start.
#[derive(Clone, Default)]
pub struct TokioWaiter {
    slots: Arc<Mutex<HashMap<String, Slot>>>,
}

impl std::fmt::Debug for TokioWaiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokioWaiter").finish_non_exhaustive()
    }
}

impl TokioWaiter {
    fn take(&self, card_id: &str) -> Option<Slot> {
        self.slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(card_id)
    }
}

/// Tiket penantian satu kartu.
#[derive(Debug)]
pub struct Ticket {
    card_id: String,
    rx: oneshot::Receiver<WaitOutcome>,
}

impl ApprovalWaiter for TokioWaiter {
    type Ticket = Ticket;

    fn register(&self, card_id: &str, run_id: &str) -> Ticket {
        let (tx, rx) = oneshot::channel();
        self.slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                card_id.to_owned(),
                Slot {
                    run_id: run_id.to_owned(),
                    tx,
                },
            );
        Ticket {
            card_id: card_id.to_owned(),
            rx,
        }
    }

    async fn wait(&self, ticket: Ticket, timeout: Duration) -> WaitOutcome {
        let outcome = match tokio::time::timeout(timeout, ticket.rx).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => WaitOutcome::Cancelled,
            Err(_) => WaitOutcome::TimedOut,
        };
        drop(self.take(&ticket.card_id));
        outcome
    }

    fn answered(&self, card_id: &str, answer: ApprovalAnswer) {
        if let Some(slot) = self.take(card_id) {
            let _ = slot.tx.send(WaitOutcome::Answered(answer));
        }
    }

    fn cancel_run(&self, run_id: &str) {
        let cancelled: Vec<Slot> = {
            let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
            let ids: Vec<String> = slots
                .iter()
                .filter(|(_, s)| s.run_id == run_id)
                .map(|(id, _)| id.clone())
                .collect();
            ids.iter().filter_map(|id| slots.remove(id)).collect()
        };
        for slot in cancelled {
            let _ = slot.tx.send(WaitOutcome::Cancelled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn jawaban_tenggat_dan_pembatalan_run() {
        let w = TokioWaiter::default();
        let t = w.register("c1", "r1");
        w.answered("c1", ApprovalAnswer::AllowOnce);
        assert_eq!(
            w.wait(t, Duration::from_secs(5)).await,
            WaitOutcome::Answered(ApprovalAnswer::AllowOnce)
        );
        let t = w.register("c2", "r1");
        assert_eq!(
            w.wait(t, Duration::from_millis(10)).await,
            WaitOutcome::TimedOut
        );
        let a = w.register("c3", "r1");
        let b = w.register("c4", "r2");
        w.cancel_run("r1");
        assert_eq!(
            w.wait(a, Duration::from_secs(5)).await,
            WaitOutcome::Cancelled
        );
        w.answered("c4", ApprovalAnswer::Deny);
        assert_eq!(
            w.wait(b, Duration::from_secs(5)).await,
            WaitOutcome::Answered(ApprovalAnswer::Deny)
        );
        assert!(w.slots.lock().unwrap().is_empty());
    }
}
