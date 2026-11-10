//! Operasi lab data produksi (plan 2026-11-10-001, U14): perlengkapan data
//! yang disetujui dari store, kredensial database dari `lab_secrets`, perintah
//! dari salinan skrip yang dipin (`lab_exec`), database lewat
//! `PgLabDatabase`, dan kunci penyimpanan (`lab_lock`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mandor_domain::lab_data::{
    LabData, StoreKind, StoreRestore, app_env_block, describe_clients, find_seed, lock_keys,
    other_clients, restore_outcome, role_block, scripts_match,
};
use mandor_store::{LabSessionsStore, PgLabDatabase, Store};
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{
    DataReady, DbCredentials, LabDatabase, LockOutcome, RestoreResult, SqlResult,
};

use crate::lab_exec::{LabExec, Pinned, StoreCommand, StoreCreds};
use crate::lab_lock::LabLocks;
use crate::lab_secrets;

/// Kunci entri daftar tabel penyimpanan di sidik sesi.
fn schema_key(store: &str) -> String {
    format!("{store}#schema")
}

/// Batas membaca `.env` repo.
const ENV_TIMEOUT: Duration = Duration::from_secs(5);

/// Operasi lab data milik daemon.
pub struct DataOps {
    store: Store,
    lab_root: PathBuf,
    exec: LabExec,
    db: PgLabDatabase,
    locks: LabLocks,
    pub sessions: LabSessionsStore,
}

impl std::fmt::Debug for DataOps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataOps")
            .field("lab_root", &self.lab_root)
            .finish_non_exhaustive()
    }
}

/// Perlengkapan data yang disetujui.
struct Approved {
    data: LabData,
    pinned: Pinned,
}

impl DataOps {
    #[must_use]
    pub fn new(store: Store, lab_root: PathBuf, exec: LabExec, locks: LabLocks) -> Arc<Self> {
        Arc::new(Self {
            sessions: LabSessionsStore::new(store.clone()),
            store,
            lab_root,
            exec,
            db: PgLabDatabase::new(),
            locks,
        })
    }

    async fn approved(&self, repo_id: &str) -> Result<Approved, String> {
        let lab = self
            .store
            .repo_lab(repo_id)
            .await
            .map_err(|e| e.to_string())?
            .filter(|l| l.approved_digest.is_some())
            .ok_or_else(|| "lab data is not approved".to_owned())?;
        let data = lab
            .equipment
            .data
            .ok_or_else(|| "lab data is not set up".to_owned())?;
        let digest = lab
            .scripts_digest
            .ok_or_else(|| "lab data is not approved".to_owned())?;
        let dir = lab_secrets::private_dir(&self.lab_root, repo_id)
            .join("scripts")
            .join(&digest);
        if !tokio::fs::metadata(&dir).await.is_ok_and(|m| m.is_dir()) {
            return Err(
                "the approved lab scripts are missing; approve the lab equipment again".to_owned(),
            );
        }
        Ok(Approved {
            data,
            pinned: Pinned { digest, dir },
        })
    }

    /// Kredensial setiap penyimpanan `postgres`.
    async fn creds(
        &self,
        repo_id: &str,
        data: &LabData,
    ) -> Result<BTreeMap<String, StoreCreds>, String> {
        let secrets = lab_secrets::load(&self.lab_root, repo_id)
            .await
            .map_err(|e| e.to_string())?;
        let mut out = BTreeMap::new();
        for store in &data.stores {
            let StoreKind::Postgres(target) = &store.kind else {
                continue;
            };
            let db = secrets
                .db(&store.name)
                .filter(|d| d.complete())
                .ok_or_else(|| format!("the database roles of {} are missing", store.name))?;
            let cred = |user: &str, password: &secrecy::SecretString| DbCredentials {
                target: target.clone(),
                user: user.to_owned(),
                password: password.clone(),
            };
            out.insert(
                store.name.clone(),
                StoreCreds {
                    lab: cred(&db.lab_user, &db.lab_password),
                    maintenance: cred(&db.maintenance_user, &db.maintenance_password),
                },
            );
        }
        Ok(out)
    }

    pub async fn prepare(&self, repo_id: &str, worktree: &Path) -> Result<DataReady, String> {
        let a = self.approved(repo_id).await?;
        let current = self
            .exec
            .worktree_digest(worktree, &a.data.scripts_dir)
            .await
            .map_err(|e| e.to_string())?;
        scripts_match(&a.pinned.digest, &current).map_err(|e| e.to_string())?;
        let creds = self.creds(repo_id, &a.data).await?;
        for (name, c) in &creds {
            if !self
                .db
                .server_is_local(&c.maintenance)
                .await
                .map_err(|e| e.to_string())?
            {
                return Err(format!(
                    "lab only runs against local databases ({name} is not local)"
                ));
            }
            let p = self
                .db
                .role_privileges(&c.maintenance, &c.lab.user)
                .await
                .map_err(|e| e.to_string())?;
            if let Some(block) = role_block(&p) {
                return Err(block.to_string());
            }
        }
        self.check_app_env(repo_id).await?;
        let mut stores = a.data.stores.clone();
        for s in &mut stores {
            if let StoreKind::Folder(p) = &s.kind
                && let Ok(real) = tokio::fs::canonicalize(p).await
            {
                s.kind = StoreKind::Folder(real.display().to_string());
            }
        }
        Ok(DataReady {
            lock_keys: lock_keys(&stores),
            scripts_digest: a.pinned.digest,
            sql_stores: creds.keys().cloned().collect(),
            seeds: a.data.seeds.iter().map(|s| s.name.clone()).collect(),
        })
    }

    /// Env database app yang diteruskan dari `.env` repo harus lokal.
    async fn check_app_env(&self, repo_id: &str) -> Result<(), String> {
        let repo = self
            .store
            .get_repo(repo_id)
            .await
            .map_err(|e| e.to_string())?;
        let app = mandor_store::app_settings_of(&repo.settings);
        if app.env_keys.is_empty() {
            return Ok(());
        }
        let path = PathBuf::from(&repo.path).join(".env");
        let text = tokio::time::timeout(ENV_TIMEOUT, tokio::fs::read_to_string(&path))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        let pairs = mandor_domain::app_run::parse_env(&text);
        app_env_block(&pairs, &app.env_keys).map_or(Ok(()), Err)
    }

    pub async fn other_clients(&self, repo_id: &str) -> Option<String> {
        let a = self.approved(repo_id).await.ok()?;
        let creds = match self.creds(repo_id, &a.data).await {
            Ok(c) => c,
            Err(e) => return Some(e),
        };
        let mut found = Vec::new();
        for c in creds.values() {
            match self.db.activity(&c.maintenance).await {
                Ok((rows, own)) => {
                    let dbs = [c.maintenance.target.database.clone()];
                    let others = other_clients(&rows, &dbs, &[own]);
                    if !others.is_empty() {
                        found.push(describe_clients(&others));
                    }
                }
                // Gagal tertutup: aktivitas yang tidak terbaca dihitung klien.
                Err(e) => found.push(format!("database activity could not be read ({e})")),
            }
        }
        (!found.is_empty()).then(|| found.join("; "))
    }

    pub async fn lock(&self, keys: &[String], wait: Duration) -> LockOutcome {
        self.locks.acquire(keys, wait).await
    }

    pub async fn mark(
        &self,
        keys: &[String],
        session: &str,
        phase: mandor_domain::lab_data::SessionPhase,
    ) {
        self.locks.mark(keys, session, phase).await;
    }

    pub async fn release(&self, keys: &[String]) {
        self.locks.release(keys).await;
    }

    /// Penanda milik daemon ini tanpa sesi aktif dihapus saat start.
    pub async fn clear_stale(&self, active: &[String]) {
        self.locks.clear_stale(active).await;
    }

    async fn run(
        &self,
        a: &Approved,
        creds: &BTreeMap<String, StoreCreds>,
        store: &mandor_domain::lab_data::LabStore,
        which: StoreCommand,
        snap: &Path,
    ) -> Result<crate::lab_exec::ExecResult, String> {
        let maint = creds.get(&store.name).map(|c| &c.maintenance);
        self.exec
            .run_store(&a.pinned, store, which, maint, snap)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn snapshot(
        &self,
        repo_id: &str,
        session: &str,
    ) -> Result<(PathBuf, Vec<(String, String)>), String> {
        let a = self.approved(repo_id).await?;
        let creds = self.creds(repo_id, &a.data).await?;
        let snap = self
            .exec
            .snapshot_dir(repo_id, session)
            .await
            .map_err(|e| e.to_string())?;
        for store in &a.data.stores {
            let r = self
                .run(&a, &creds, store, StoreCommand::Snapshot, &snap)
                .await?;
            if !r.success {
                return Err(format!(
                    "the snapshot of {} failed: {}",
                    store.name,
                    r.output.trim()
                ));
            }
        }
        if let Ok(loose) = self.exec.tighten_snapshot(&snap).await
            && !loose.is_empty()
        {
            tracing::warn!(
                repo = repo_id,
                files = loose.len(),
                "berkas snapshot lab bermode longgar dikencangkan"
            );
        }
        let fps = self.fingerprints_of(&a, &creds, &snap).await?;
        Ok((snap, fps))
    }

    async fn fingerprints_of(
        &self,
        a: &Approved,
        creds: &BTreeMap<String, StoreCreds>,
        snap: &Path,
    ) -> Result<Vec<(String, String)>, String> {
        let mut out = Vec::new();
        for store in &a.data.stores {
            let r = self
                .run(a, creds, store, StoreCommand::Fingerprint, snap)
                .await?;
            let fp = r.output.trim().to_owned();
            if !r.success || fp.is_empty() {
                return Err(format!(
                    "the fingerprint of {} could not be read",
                    store.name
                ));
            }
            out.push((store.name.clone(), fp));
        }
        // Daftar tabel ikut sidik agar migrasi dari run lain terdeteksi
        // sebagai perubahan skema (KTD-L20).
        for (name, c) in creds {
            let tables = self
                .db
                .tables(&c.maintenance)
                .await
                .map_err(|e| format!("the tables of {name} could not be listed: {e}"))?;
            out.push((schema_key(name), tables.join(",")));
        }
        Ok(out)
    }

    pub async fn fingerprints(
        &self,
        repo_id: &str,
        snap: &Path,
    ) -> Result<Vec<(String, String)>, String> {
        let a = self.approved(repo_id).await?;
        let creds = self.creds(repo_id, &a.data).await?;
        self.fingerprints_of(&a, &creds, snap).await
    }

    pub async fn restore(
        &self,
        repo_id: &str,
        snap: &Path,
        before: &[(String, String)],
    ) -> RestoreResult {
        if let Some(clients) = self.other_clients(repo_id).await {
            return RestoreResult::Waiting(clients);
        }
        let (a, creds) = match self.approved(repo_id).await {
            Ok(a) => match self.creds(repo_id, &a.data).await {
                Ok(c) => (a, c),
                Err(e) => return RestoreResult::Failed(e),
            },
            Err(e) => return RestoreResult::Failed(e),
        };
        let mut results = Vec::new();
        let mut schema_changed = Vec::new();
        for (name, c) in &creds {
            let before_tables = before
                .iter()
                .find(|(n, _)| n == &schema_key(name))
                .map(|(_, t)| t);
            if let (Some(old), Ok(now)) = (before_tables, self.db.tables(&c.maintenance).await)
                && &now.join(",") != old
            {
                schema_changed.push(name.clone());
            }
        }
        for store in &a.data.stores {
            let r = match self
                .run(&a, &creds, store, StoreCommand::Restore, snap)
                .await
            {
                Ok(r) => r,
                Err(e) => return RestoreResult::Failed(e),
            };
            let after = if r.success {
                self.run(&a, &creds, store, StoreCommand::Fingerprint, snap)
                    .await
                    .ok()
                    .filter(|f| f.success)
                    .map(|f| f.output.trim().to_owned())
            } else {
                None
            };
            results.push(StoreRestore {
                store: store.name.clone(),
                succeeded: r.success,
                timed_out: r.timed_out,
                before: before
                    .iter()
                    .find(|(n, _)| n == &store.name)
                    .map(|(_, f)| f.clone())
                    .unwrap_or_default(),
                after,
                schema_changed: schema_changed.contains(&store.name),
            });
        }
        match restore_outcome(&results) {
            mandor_domain::lab_data::RestoreOutcome::Restored => RestoreResult::Restored,
            mandor_domain::lab_data::RestoreOutcome::Failed(why) => RestoreResult::Failed(why),
        }
    }

    pub async fn discard(&self, snap: &Path) {
        self.exec.remove_snapshot(snap).await;
    }

    pub async fn sql(&self, repo_id: &str, store: &str, sql: &str) -> Result<SqlResult, PortError> {
        let a = self.approved(repo_id).await.map_err(PortError::Rejected)?;
        let creds = self
            .creds(repo_id, &a.data)
            .await
            .map_err(PortError::Rejected)?;
        let c = creds
            .get(store)
            .ok_or_else(|| PortError::Rejected(format!("unknown database store {store}")))?;
        self.db.run_statement(&c.lab, sql).await
    }

    pub async fn seed(
        &self,
        repo_id: &str,
        name: &str,
        worktree: &Path,
    ) -> Result<String, PortError> {
        let a = self.approved(repo_id).await.map_err(PortError::Rejected)?;
        let seed = find_seed(&a.data, name)
            .ok_or_else(|| PortError::Rejected(format!("unknown seed {name}")))?
            .clone();
        let creds = self
            .creds(repo_id, &a.data)
            .await
            .map_err(PortError::Rejected)?;
        let lab = creds.values().next().map(|c| &c.lab);
        let r = self
            .exec
            .run_seed(&a.pinned, &seed, lab, worktree)
            .await
            .map_err(|e| PortError::Rejected(e.to_string()))?;
        if r.success {
            Ok(r.output)
        } else {
            Err(PortError::Rejected(format!(
                "seed {name} failed: {}",
                r.output.trim()
            )))
        }
    }
}
