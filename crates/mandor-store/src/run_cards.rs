//! Kartu run grup di tabel `run_cards` (KTD-G1, KTD-G2 plan run grup).

use mandor_domain::group::CardState;
use mandor_usecase::run_ports::{CardCommit, GroupCardRow};
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use crate::StoreError;

/// Pesan konflik yang sama dengan run tunggal.
pub(crate) const CONFLICT: &str = "record already has an active run";

/// Bagian kartu yang disimpan di `data_json`.
#[derive(Serialize, Deserialize, Default)]
struct CardJson {
    #[serde(default)]
    changed_in_lark: bool,
    #[serde(default)]
    commits: Vec<CommitJson>,
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    fix_requested_by: Option<String>,
    #[serde(default)]
    quiet_until_ms: Option<i64>,
    #[serde(default)]
    in_fix: bool,
}

#[derive(Serialize, Deserialize)]
struct CommitJson {
    repo_id: String,
    sha: String,
}

/// Kartu terbuka di run terbuka lain atau di run tunggal terbuka: konflik.
pub(crate) async fn ensure_free(
    conn: &mut SqliteConnection,
    record_id: &str,
) -> Result<(), StoreError> {
    let taken = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM run_cards c JOIN runs r ON r.id = c.run_id
         WHERE c.record_id = ? AND c.open = 1 AND r.closed = 0",
        record_id
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(StoreError::internal)?;
    if taken > 0 {
        return Err(StoreError::Conflict(CONFLICT));
    }
    Ok(())
}

/// Run tunggal terbuka untuk kartu: konflik.
pub(crate) async fn ensure_no_single_run(
    conn: &mut SqliteConnection,
    record_id: &str,
) -> Result<(), StoreError> {
    let taken = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM runs WHERE record_id = ? AND closed = 0",
        record_id
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(StoreError::internal)?;
    if taken > 0 {
        return Err(StoreError::Conflict(CONFLICT));
    }
    Ok(())
}

/// Menulis ulang seluruh kartu run.
pub(crate) async fn write_cards(
    conn: &mut SqliteConnection,
    run_id: &str,
    cards: &[GroupCardRow],
) -> Result<(), StoreError> {
    sqlx::query!("DELETE FROM run_cards WHERE run_id = ?", run_id)
        .execute(&mut *conn)
        .await
        .map_err(StoreError::internal)?;
    for (i, c) in cards.iter().enumerate() {
        let position = i64::try_from(i).map_err(StoreError::internal)?;
        let state = c.state.as_str();
        let open = i64::from(!matches!(c.state, CardState::Removed(_)));
        let data = serde_json::to_string(&CardJson {
            changed_in_lark: c.changed_in_lark,
            commits: c
                .commits
                .iter()
                .map(|k| CommitJson {
                    repo_id: k.repo_id.clone(),
                    sha: k.sha.clone(),
                })
                .collect(),
            plan: c.plan.clone(),
            fix_requested_by: c.fix_requested_by.clone(),
            quiet_until_ms: c.quiet_until_ms,
            in_fix: c.in_fix,
        })
        .map_err(StoreError::internal)?;
        sqlx::query!(
            "INSERT INTO run_cards (run_id, record_id, position, state, data_json, open)
             VALUES (?, ?, ?, ?, ?, ?)",
            run_id,
            c.record_id,
            position,
            state,
            data,
            open
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| StoreError::from_sqlx(e, CONFLICT))?;
    }
    Ok(())
}

/// Kartu run, urut posisi.
pub(crate) async fn read_cards(
    conn: &mut SqliteConnection,
    run_id: &str,
) -> Result<Vec<GroupCardRow>, StoreError> {
    let rows = sqlx::query!(
        "SELECT record_id, state, data_json FROM run_cards WHERE run_id = ? ORDER BY position",
        run_id
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(StoreError::internal)?;
    rows.into_iter()
        .map(|r| {
            let state = CardState::parse(&r.state)
                .ok_or_else(|| StoreError::internal(UnknownState(r.state.clone())))?;
            let data: CardJson =
                serde_json::from_str(&r.data_json).map_err(StoreError::internal)?;
            Ok(GroupCardRow {
                record_id: r.record_id,
                state,
                changed_in_lark: data.changed_in_lark,
                commits: data
                    .commits
                    .into_iter()
                    .map(|k| CardCommit {
                        repo_id: k.repo_id,
                        sha: k.sha,
                    })
                    .collect(),
                plan: data.plan,
                fix_requested_by: data.fix_requested_by,
                quiet_until_ms: data.quiet_until_ms,
                in_fix: data.in_fix,
            })
        })
        .collect()
}

/// Run grup terbuka yang memuat kartu terbuka `record_id`.
pub(crate) async fn open_group_of(
    conn: &mut SqliteConnection,
    record_id: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar!(
        "SELECT c.run_id FROM run_cards c JOIN runs r ON r.id = c.run_id
         WHERE c.record_id = ? AND c.open = 1 AND r.closed = 0
         ORDER BY r.created_at_ms DESC LIMIT 1",
        record_id
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(StoreError::internal)
}

/// Nilai state kartu yang tidak dikenal di database.
#[derive(Debug, thiserror::Error)]
#[error("unknown run card state: {0}")]
struct UnknownState(String);
