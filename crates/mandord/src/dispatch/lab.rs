//! Method `lab.*`: perlengkapan review lab per repo (plan 2026-11-10-001, U3).
//!
//! Hanya koneksi token penuh milik app yang sampai di sini; koneksi token
//! kapabilitas ditolak di [`super::scoped_route`].

use mandor_domain::lab::{
    LabAccount, LabEquipment, LoginForm, SuccessMarker, equipment_canonical, validate_equipment,
};
use mandor_domain::lab_data::{DbTarget, LabData, LabSeed, LabStore, StoreKind};
use mandor_protocol::{
    LabAccountDto, LabApproveParams, LabDataDto, LabEquipmentDto, LabLoginDto, LabRepoRef,
    LabSeedDto, LabSetParams, LabStoreDto, LabView, RpcError, method,
};
use mandor_usecase::run_ports::DbCredentials;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{AppDispatch, internal, ok, params, rejected, store_error};
use crate::lab_secrets;

impl AppDispatch {
    pub(super) fn is_lab_method(name: &str) -> bool {
        [
            method::LAB_GET,
            method::LAB_SET,
            method::LAB_APPROVE,
            method::LAB_PROPOSE,
            method::LAB_RETRY_RESTORE,
            method::LAB_DATA_RESTORED,
        ]
        .contains(&name)
    }

    pub(super) async fn lab_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        match name {
            method::LAB_SET => self.lab_set(params(v)?).await,
            method::LAB_APPROVE => self.lab_approve(params(v)?).await,
            method::LAB_RETRY_RESTORE => {
                let p: LabRepoRef = params(v)?;
                self.session()
                    .await?
                    .runs
                    .retry_lab_restore(&p.repo_id)
                    .await
                    .map_err(super::run_error)?;
                self.lab_view(&p.repo_id).await
            }
            method::LAB_DATA_RESTORED => {
                let p: mandor_protocol::LabDataRestored = params(v)?;
                self.session()
                    .await?
                    .runs
                    .confirm_lab_data_restored(&p.repo_id, p.discard_snapshot)
                    .await
                    .map_err(super::run_error)?;
                self.lab_view(&p.repo_id).await
            }
            method::LAB_PROPOSE => {
                let p: LabRepoRef = params(v)?;
                self.session()
                    .await?
                    .runs
                    .propose_lab(&p.repo_id)
                    .await
                    .map_err(super::run_error)?;
                self.lab_view(&p.repo_id).await
            }
            _ => {
                let p: LabRepoRef = params(v)?;
                self.lab_view(&p.repo_id).await
            }
        }
    }

    async fn lab_view(&self, repo_id: &str) -> Result<Value, RpcError> {
        let lab = self
            .app
            .store
            .repo_lab(repo_id)
            .await
            .map_err(store_error)?;
        let secrets = lab_secrets::load(&self.app.cfg.lab_root(), repo_id)
            .await
            .map_err(internal)?;
        let (setup, data_session) = match self.app.session().await {
            Some(s) => (
                s.runs.lab_setup_status(repo_id),
                s.runs.lab_data_session(repo_id).await,
            ),
            None => (mandor_usecase::review::LabSetupStatus::default(), None),
        };
        ok(&LabView {
            repo_id: repo_id.to_owned(),
            approved: lab.as_ref().is_some_and(|l| l.approved_digest.is_some()),
            approved_at_ms: lab.as_ref().and_then(|l| l.approved_at_ms),
            equipment: lab.map(|l| equipment_dto(&l.equipment, &secrets)),
            proposing: setup.running,
            proposal: setup.proposal.map(|eq| equipment_dto(&eq, &secrets)),
            proposal_error: setup.error,
            data_session: data_session.map(|d| mandor_protocol::LabDataSessionView {
                run_id: d.run_id,
                phase: d.phase.as_str().to_owned(),
                error: d.error,
            }),
        })
    }

    async fn lab_set(&self, p: LabSetParams) -> Result<Value, RpcError> {
        let eq = equipment_of(p.equipment)?;
        validate_equipment(&eq, &self.app_origins(&p.repo_id).await?).map_err(rejected)?;
        let root = self.app.cfg.lab_root();
        let roles: Vec<String> = eq.accounts.iter().map(|a| a.role.clone()).collect();
        let secrets = lab_secrets::load(&root, &p.repo_id)
            .await
            .map_err(internal)?
            .merge(&roles, p.passwords)
            .merge_db(&postgres_stores(&eq), p.db);
        lab_secrets::save(&root, &p.repo_id, &secrets)
            .await
            .map_err(internal)?;
        self.app
            .store
            .save_repo_lab(&p.repo_id, &eq)
            .await
            .map_err(store_error)?;
        tracing::info!(repo = %p.repo_id, roles = roles.len(), "perlengkapan lab disimpan");
        self.lab_view(&p.repo_id).await
    }

    async fn lab_approve(&self, p: LabApproveParams) -> Result<Value, RpcError> {
        let eq = equipment_of(p.equipment)?;
        validate_equipment(&eq, &self.app_origins(&p.repo_id).await?).map_err(rejected)?;
        let secrets = lab_secrets::load(&self.app.cfg.lab_root(), &p.repo_id)
            .await
            .map_err(internal)?;
        if let Some(a) = eq.accounts.iter().find(|a| !secrets.has(&a.role)) {
            return Err(rejected(format!(
                "Enter the password for role `{}` before approving",
                a.role
            )));
        }
        let scripts = match &eq.data {
            Some(data) => Some(self.approve_data(&p.repo_id, data, &secrets).await?),
            None => None,
        };
        let digest = approval_digest(&eq, scripts.as_deref());
        let now = crate::prod::now_ms();
        let approved = self
            .app
            .store
            .approve_repo_lab(&p.repo_id, &eq, &digest, scripts.as_deref(), now)
            .await
            .map_err(store_error)?;
        if !approved {
            return Err(rejected(
                "The lab equipment changed since you opened it; reload and review it again",
            ));
        }
        tracing::info!(repo = %p.repo_id, "perlengkapan lab disetujui");
        self.lab_view(&p.repo_id).await
    }

    /// Persetujuan lab data (KTD-L15–KTD-L19): app harus berhenti, folder
    /// skrip dipin, lalu uji bolak-balik setiap penyimpanan dan seed.
    /// Mengembalikan digest salinan skrip.
    async fn approve_data(
        &self,
        repo_id: &str,
        data: &LabData,
        secrets: &lab_secrets::LabSecrets,
    ) -> Result<String, RpcError> {
        if let Some(s) = self.app.session().await {
            let running = s
                .runs
                .app_owner_status()
                .await
                .map_err(super::run_error)?
                .is_some_and(|r| r.state == mandor_domain::app_run::AppRunState::Running);
            if running {
                return Err(rejected(
                    "Stop the running app before approving lab data; the check changes and restores its data",
                ));
            }
        }
        let mut creds = std::collections::BTreeMap::new();
        for store in &data.stores {
            let StoreKind::Postgres(target) = &store.kind else {
                continue;
            };
            let Some(db) = secrets.db(&store.name).filter(|d| d.complete()) else {
                return Err(rejected(format!(
                    "Enter the lab and maintenance database roles of {} before approving",
                    store.name
                )));
            };
            let cred = |user: &str, password: &secrecy::SecretString| DbCredentials {
                target: target.clone(),
                user: user.to_owned(),
                password: password.clone(),
            };
            creds.insert(
                store.name.clone(),
                crate::lab_exec::StoreCreds {
                    lab: cred(&db.lab_user, &db.lab_password),
                    maintenance: cred(&db.maintenance_user, &db.maintenance_password),
                },
            );
        }
        let repo = self
            .app
            .store
            .get_repo(repo_id)
            .await
            .map_err(store_error)?;
        let login = crate::session::login_env(&self.app.cfg).await;
        let exec = crate::lab_exec::LabExec::new(
            self.app.cfg.lab_root(),
            mandor_agent::claude::guardrails::clean_env(&login),
        );
        let worktree = std::path::PathBuf::from(&repo.path);
        let pinned = exec
            .pin_scripts(repo_id, &worktree, &data.scripts_dir)
            .await
            .map_err(rejected)?;
        let snap = exec
            .snapshot_dir(repo_id, &format!("approve-{}", crate::prod::now_ms()))
            .await
            .map_err(internal)?;
        let checked = exec
            .approval_round_trip(
                &mandor_store::PgLabDatabase::new(),
                data,
                &pinned,
                &creds,
                &worktree,
                &snap,
            )
            .await;
        exec.remove_snapshot(&snap).await;
        checked.map_err(rejected)?;
        tracing::info!(repo = %repo_id, stores = data.stores.len(), "uji bolak-balik lab data lulus");
        Ok(pinned.digest)
    }

    /// Origin app repo untuk memeriksa URL form login.
    async fn app_origins(&self, repo_id: &str) -> Result<Vec<String>, RpcError> {
        let repo = self
            .app
            .store
            .get_repo(repo_id)
            .await
            .map_err(store_error)?;
        let app = mandor_store::app_settings_of(&repo.settings);
        app.canonical_url()
            .map(|u| vec![u])
            .ok_or_else(|| rejected("Set the app URL or port in the repo settings first"))
    }
}

/// Nama penyimpanan `postgres` perlengkapan (kunci kredensial database).
fn postgres_stores(eq: &LabEquipment) -> Vec<String> {
    eq.data.as_ref().map_or_else(Vec::new, |d| {
        d.stores
            .iter()
            .filter(|s| matches!(s.kind, StoreKind::Postgres(_)))
            .map(|s| s.name.clone())
            .collect()
    })
}

/// SHA-256 hex dari string kanonis perlengkapan (KTD-L3), ditambah digest
/// salinan skrip lab data bila ada (KTD-L18).
fn approval_digest(eq: &LabEquipment, scripts: Option<&str>) -> String {
    let mut canonical = equipment_canonical(eq);
    if let Some(d) = scripts {
        canonical.push_str("\nscripts:");
        canonical.push_str(d);
    }
    let hash = Sha256::digest(canonical.as_bytes());
    hash.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn equipment_dto(eq: &LabEquipment, secrets: &lab_secrets::LabSecrets) -> LabEquipmentDto {
    let (success_kind, success) = match &eq.login.success {
        SuccessMarker::PathPrefix(p) => ("path", p.clone()),
        SuccessMarker::Text(t) => ("text", t.clone()),
    };
    LabEquipmentDto {
        accounts: eq
            .accounts
            .iter()
            .map(|a| LabAccountDto {
                role: a.role.clone(),
                username: a.username.clone(),
                default: a.default,
                has_password: secrets.has(&a.role),
            })
            .collect(),
        login: LabLoginDto {
            url: eq.login.url.clone(),
            username_field: eq.login.username_field.clone(),
            password_field: eq.login.password_field.clone(),
            submit: eq.login.submit.clone(),
            submit_path: eq.login.submit_path.clone(),
            success_kind: success_kind.to_owned(),
            success,
            authenticated_url: eq.login.authenticated_url.clone(),
        },
        safe_posts: eq.safe_posts.clone(),
        fixtures_dir: eq.fixtures_dir.clone(),
        data: eq.data.as_ref().map(|d| data_dto(d, secrets)),
    }
}

fn data_dto(d: &LabData, secrets: &lab_secrets::LabSecrets) -> LabDataDto {
    LabDataDto {
        scripts_dir: d.scripts_dir.clone(),
        stores: d
            .stores
            .iter()
            .map(|s| {
                let (kind, host, port, database, folder) = match &s.kind {
                    StoreKind::Postgres(t) => (
                        "postgres",
                        Some(t.host.clone()),
                        Some(t.port),
                        Some(t.database.clone()),
                        None,
                    ),
                    StoreKind::Folder(p) => ("folder", None, None, None, Some(p.clone())),
                };
                let db = secrets.db(&s.name);
                LabStoreDto {
                    lab_user: db.map(|d| d.lab_user.clone()),
                    maintenance_user: db.map(|d| d.maintenance_user.clone()),
                    has_credentials: db.is_some_and(lab_secrets::DbSecret::complete),
                    name: s.name.clone(),
                    kind: kind.to_owned(),
                    host,
                    port,
                    database,
                    folder,
                    snapshot: s.snapshot.clone(),
                    restore: s.restore.clone(),
                    fingerprint: s.fingerprint.clone(),
                    test_change: s.test_change.clone(),
                }
            })
            .collect(),
        seeds: d
            .seeds
            .iter()
            .map(|s| LabSeedDto {
                name: s.name.clone(),
                command: s.command.clone(),
            })
            .collect(),
    }
}

fn data_of(d: LabDataDto) -> Result<LabData, RpcError> {
    let trim = |s: String| s.trim().to_owned();
    let stores = d
        .stores
        .into_iter()
        .map(|s| {
            let kind = match s.kind.as_str() {
                "postgres" => StoreKind::Postgres(DbTarget {
                    host: s.host.map(trim).unwrap_or_default(),
                    port: s.port.unwrap_or(5432),
                    database: s.database.map(trim).unwrap_or_default(),
                }),
                "folder" => StoreKind::Folder(s.folder.map(trim).unwrap_or_default()),
                other => return Err(rejected(format!("unknown store kind: {other}"))),
            };
            Ok(LabStore {
                name: trim(s.name),
                kind,
                snapshot: trim(s.snapshot),
                restore: trim(s.restore),
                fingerprint: trim(s.fingerprint),
                test_change: trim(s.test_change),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LabData {
        scripts_dir: trim(d.scripts_dir),
        stores,
        seeds: d
            .seeds
            .into_iter()
            .map(|s| LabSeed {
                name: trim(s.name),
                command: trim(s.command),
            })
            .collect(),
    })
}

pub(super) fn equipment_of(d: LabEquipmentDto) -> Result<LabEquipment, RpcError> {
    let success = match d.login.success_kind.as_str() {
        "path" => SuccessMarker::PathPrefix(d.login.success),
        "text" => SuccessMarker::Text(d.login.success),
        other => return Err(rejected(format!("unknown signed-in marker kind: {other}"))),
    };
    Ok(LabEquipment {
        accounts: d
            .accounts
            .into_iter()
            .map(|a| LabAccount {
                role: a.role.trim().to_owned(),
                username: a.username.trim().to_owned(),
                default: a.default,
            })
            .collect(),
        login: LoginForm {
            url: d.login.url.trim().to_owned(),
            username_field: d.login.username_field.trim().to_owned(),
            password_field: d.login.password_field.trim().to_owned(),
            submit: d.login.submit.trim().to_owned(),
            submit_path: d.login.submit_path.trim().to_owned(),
            success,
            authenticated_url: d.login.authenticated_url.trim().to_owned(),
        },
        safe_posts: d
            .safe_posts
            .into_iter()
            .map(|p| p.trim().to_owned())
            .collect(),
        fixtures_dir: d
            .fixtures_dir
            .map(|f| f.trim().to_owned())
            .filter(|f| !f.is_empty()),
        data: d.data.map(data_of).transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dto() -> LabEquipmentDto {
        LabEquipmentDto {
            accounts: vec![LabAccountDto {
                role: " admin ".to_owned(),
                username: "a@uji.test".to_owned(),
                default: true,
                has_password: true,
            }],
            login: LabLoginDto {
                url: "http://localhost:3000/login".to_owned(),
                username_field: "#email".to_owned(),
                password_field: "#password".to_owned(),
                submit: "text=Sign in".to_owned(),
                submit_path: "/api/login".to_owned(),
                success_kind: "path".to_owned(),
                success: "/admin".to_owned(),
                authenticated_url: "http://localhost:3000/admin".to_owned(),
            },
            safe_posts: vec![],
            fixtures_dir: Some(" ".to_owned()),
            data: None,
        }
    }

    #[test]
    fn dto_bolak_balik_dan_dirapikan() {
        let eq = equipment_of(dto()).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(eq.accounts[0].role, "admin");
        assert_eq!(eq.fixtures_dir, None);
        let secrets = lab_secrets::LabSecrets::default();
        let back = equipment_dto(&eq, &secrets);
        assert!(!back.accounts[0].has_password);
        assert_eq!(back.login, dto().login);
    }

    /// Fase 2: bagian data bolak-balik lewat DTO; jenis penyimpanan asing
    /// ditolak.
    #[test]
    fn bagian_data_bolak_balik_lewat_dto() {
        let store = |name: &str, kind: &str| LabStoreDto {
            name: name.to_owned(),
            kind: kind.to_owned(),
            host: (kind == "postgres").then(|| " localhost ".to_owned()),
            port: (kind == "postgres").then_some(5433),
            database: (kind == "postgres").then(|| "kelola".to_owned()),
            folder: (kind == "folder").then(|| "/srv/uploads".to_owned()),
            snapshot: "snapshot.sh".to_owned(),
            restore: "restore.sh".to_owned(),
            fingerprint: "fingerprint.sh".to_owned(),
            test_change: "change.sh".to_owned(),
            lab_user: None,
            maintenance_user: None,
            has_credentials: false,
        };
        let mut d = dto();
        d.data = Some(LabDataDto {
            scripts_dir: "review-fixtures/lab".to_owned(),
            stores: vec![store("tenant", "postgres"), store("uploads", "folder")],
            seeds: vec![LabSeedDto {
                name: "kb".to_owned(),
                command: "seed-kb.sh".to_owned(),
            }],
        });
        let eq = equipment_of(d).unwrap_or_else(|e| panic!("{e:?}"));
        let data = eq.data.clone().unwrap_or_else(|| panic!("data"));
        assert_eq!(
            data.stores[0].kind,
            StoreKind::Postgres(DbTarget {
                host: "localhost".to_owned(),
                port: 5433,
                database: "kelola".to_owned()
            })
        );
        let back = equipment_dto(&eq, &lab_secrets::LabSecrets::default());
        assert_eq!(back.data.map(|d| d.stores.len()), Some(2));
        let mut bad = dto();
        bad.data = Some(LabDataDto {
            scripts_dir: "x".to_owned(),
            stores: vec![store("s3", "s3")],
            seeds: vec![],
        });
        assert!(equipment_of(bad).is_err());
    }

    #[test]
    fn jenis_penanda_tidak_dikenal_ditolak() {
        let mut d = dto();
        d.login.success_kind = "css".to_owned();
        assert!(equipment_of(d).is_err());
    }

    #[test]
    fn digest_hex_64_dan_stabil() {
        let eq = equipment_of(dto()).unwrap_or_else(|e| panic!("{e:?}"));
        let a = approval_digest(&eq, None);
        assert_eq!(a.len(), 64);
        assert_eq!(a, approval_digest(&eq, None));
        let mut other = eq.clone();
        other.safe_posts.push("/api/search".to_owned());
        assert_ne!(a, approval_digest(&other, None));
        // Isi skrip lab data ikut persetujuan (KTD-L18).
        assert_ne!(a, approval_digest(&eq, Some("aa")));
        assert_ne!(
            approval_digest(&eq, Some("aa")),
            approval_digest(&eq, Some("bb"))
        );
    }
}
