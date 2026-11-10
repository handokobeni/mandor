//! Method `lab.*`: perlengkapan review lab per repo (plan 2026-11-10-001, U3).
//!
//! Hanya koneksi token penuh milik app yang sampai di sini; koneksi token
//! kapabilitas ditolak di [`super::scoped_route`].

use mandor_domain::lab::{
    LabAccount, LabEquipment, LoginForm, SuccessMarker, equipment_canonical, validate_equipment,
};
use mandor_protocol::{
    LabAccountDto, LabApproveParams, LabEquipmentDto, LabLoginDto, LabRepoRef, LabSetParams,
    LabView, RpcError, method,
};
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
        ]
        .contains(&name)
    }

    pub(super) async fn lab_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        match name {
            method::LAB_SET => self.lab_set(params(v)?).await,
            method::LAB_APPROVE => self.lab_approve(params(v)?).await,
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
        let setup = match self.app.session().await {
            Some(s) => s.runs.lab_setup_status(repo_id),
            None => mandor_usecase::review::LabSetupStatus::default(),
        };
        ok(&LabView {
            repo_id: repo_id.to_owned(),
            approved: lab.as_ref().is_some_and(|l| l.approved_digest.is_some()),
            approved_at_ms: lab.as_ref().and_then(|l| l.approved_at_ms),
            equipment: lab.map(|l| equipment_dto(&l.equipment, &secrets)),
            proposing: setup.running,
            proposal: setup.proposal.map(|eq| equipment_dto(&eq, &secrets)),
            proposal_error: setup.error,
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
            .merge(&roles, p.passwords);
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
        let digest = approval_digest(&eq);
        let now = crate::prod::now_ms();
        let approved = self
            .app
            .store
            .approve_repo_lab(&p.repo_id, &eq, &digest, now)
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

/// SHA-256 hex dari string kanonis perlengkapan (KTD-L3). Folder skrip
/// (fase 2) ditambahkan ke string kanonis saat ada.
fn approval_digest(eq: &LabEquipment) -> String {
    let hash = Sha256::digest(equipment_canonical(eq).as_bytes());
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
    }
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

    #[test]
    fn jenis_penanda_tidak_dikenal_ditolak() {
        let mut d = dto();
        d.login.success_kind = "css".to_owned();
        assert!(equipment_of(d).is_err());
    }

    #[test]
    fn digest_hex_64_dan_stabil() {
        let eq = equipment_of(dto()).unwrap_or_else(|e| panic!("{e:?}"));
        let a = approval_digest(&eq);
        assert_eq!(a.len(), 64);
        assert_eq!(a, approval_digest(&eq));
        let mut other = eq.clone();
        other.safe_posts.push("/api/search".to_owned());
        assert_ne!(a, approval_digest(&other));
    }
}
