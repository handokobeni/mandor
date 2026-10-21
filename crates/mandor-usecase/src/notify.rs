//! Penyusun notifikasi untuk dev (R2).

use crate::ports::{Deps, TaskCache};
use crate::run::RunService;
use crate::run_ports::{Notice, NoticeKind, RunRecord};

/// Notifikasi dengan judul task dari cache (tanpa panggilan jaringan).
pub(crate) async fn notice<D: Deps>(
    svc: &RunService<D>,
    kind: NoticeKind,
    run: &RunRecord,
    body: &str,
) -> Notice {
    let title = match svc.sync.cache.get(&run.record_id).await {
        Ok(Some(c)) => match &c.task.key {
            Some(k) => format!("{k} {}", c.task.title),
            None => c.task.title,
        },
        _ => run.record_id.clone(),
    };
    Notice {
        kind,
        record_id: run.record_id.clone(),
        run_id: Some(run.id.clone()),
        title,
        body: body.to_owned(),
    }
}
