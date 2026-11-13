//! Bentuk simpan JSON tipe review lab (plan 2026-11-10-001, U2).

use mandor_domain::lab::{
    BlockedRequest, Evidence, LabAccount, LabEquipment, LabNeed, LoginForm, LookAdvice,
    LookRecommendation, Simulated, SuccessMarker,
};
use mandor_domain::lab_data::{DbTarget, LabData, LabSeed, LabStore, StoreKind};
use serde::{Deserialize, Serialize};

use crate::StoreError;

#[derive(Serialize, Deserialize, Default)]
struct StoredEvidence {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    steps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    screenshots: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prepared: Option<String>,
    #[serde(default)]
    simulated: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    blocked: Vec<StoredBlocked>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    lab: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredBlocked {
    method: String,
    url: String,
}

pub(crate) fn evidence_json(e: Option<&Evidence>) -> Result<Option<String>, StoreError> {
    let Some(e) = e else { return Ok(None) };
    serde_json::to_string(&StoredEvidence {
        steps: e.steps.clone(),
        screenshots: e.screenshots.clone(),
        prepared: e.prepared.clone(),
        simulated: e.simulated.as_str().to_owned(),
        blocked: e
            .blocked_requests
            .iter()
            .map(|b| StoredBlocked {
                method: b.method.clone(),
                url: b.url.clone(),
            })
            .collect(),
        lab: e.lab,
    })
    .map(Some)
    .map_err(StoreError::internal)
}

pub(crate) fn evidence_of(raw: Option<&str>) -> Result<Option<Evidence>, StoreError> {
    let Some(raw) = raw else { return Ok(None) };
    let s: StoredEvidence = serde_json::from_str(raw).map_err(StoreError::internal)?;
    Ok(Some(Evidence {
        steps: s.steps,
        screenshots: s.screenshots,
        prepared: s.prepared,
        simulated: Simulated::parse(&s.simulated).unwrap_or_default(),
        blocked_requests: s
            .blocked
            .into_iter()
            .map(|b| BlockedRequest {
                method: b.method,
                url: b.url,
            })
            .collect(),
        lab: s.lab,
    }))
}

#[derive(Serialize, Deserialize)]
struct StoredLook {
    advice: String,
    reason: String,
}

pub(crate) fn look_json(l: Option<&LookRecommendation>) -> Result<Option<String>, StoreError> {
    let Some(l) = l else { return Ok(None) };
    serde_json::to_string(&StoredLook {
        advice: l.advice.as_str().to_owned(),
        reason: l.reason.clone(),
    })
    .map(Some)
    .map_err(StoreError::internal)
}

pub(crate) fn look_of(raw: Option<&str>) -> Result<Option<LookRecommendation>, StoreError> {
    let Some(raw) = raw else { return Ok(None) };
    let s: StoredLook = serde_json::from_str(raw).map_err(StoreError::internal)?;
    let advice = LookAdvice::parse(&s.advice).ok_or_else(|| corrupt("look advice", &s.advice))?;
    Ok(Some(LookRecommendation {
        advice,
        reason: s.reason,
    }))
}

pub(crate) fn lab_need_of(raw: Option<&str>) -> Result<Option<LabNeed>, StoreError> {
    raw.map(|n| LabNeed::parse(n).ok_or_else(|| corrupt("lab_need", n)))
        .transpose()
}

#[derive(Serialize, Deserialize)]
struct StoredEquipment {
    accounts: Vec<StoredAccount>,
    login: StoredLogin,
    #[serde(default)]
    safe_posts: Vec<String>,
    #[serde(default)]
    fixtures_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<StoredData>,
}

#[derive(Serialize, Deserialize)]
struct StoredData {
    scripts_dir: String,
    stores: Vec<StoredStore>,
    #[serde(default)]
    seeds: Vec<StoredSeed>,
}

#[derive(Serialize, Deserialize)]
struct StoredStore {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    postgres: Option<StoredDb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder: Option<String>,
    snapshot: String,
    restore: String,
    fingerprint: String,
    test_change: String,
}

#[derive(Serialize, Deserialize)]
struct StoredDb {
    host: String,
    port: u16,
    database: String,
}

#[derive(Serialize, Deserialize)]
struct StoredSeed {
    name: String,
    command: String,
}

fn data_stored(d: &LabData) -> StoredData {
    StoredData {
        scripts_dir: d.scripts_dir.clone(),
        stores: d
            .stores
            .iter()
            .map(|s| {
                let (postgres, folder) = match &s.kind {
                    StoreKind::Postgres(t) => (
                        Some(StoredDb {
                            host: t.host.clone(),
                            port: t.port,
                            database: t.database.clone(),
                        }),
                        None,
                    ),
                    StoreKind::Folder(p) => (None, Some(p.clone())),
                };
                StoredStore {
                    name: s.name.clone(),
                    postgres,
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
            .map(|s| StoredSeed {
                name: s.name.clone(),
                command: s.command.clone(),
            })
            .collect(),
    }
}

fn data_of(s: StoredData) -> Result<LabData, StoreError> {
    let stores = s
        .stores
        .into_iter()
        .map(|st| {
            let kind = match (st.postgres, st.folder) {
                (Some(db), None) => StoreKind::Postgres(DbTarget {
                    host: db.host,
                    port: db.port,
                    database: db.database,
                }),
                (None, Some(p)) => StoreKind::Folder(p),
                _ => return Err(corrupt("lab store kind", &st.name)),
            };
            Ok(LabStore {
                name: st.name,
                kind,
                snapshot: st.snapshot,
                restore: st.restore,
                fingerprint: st.fingerprint,
                test_change: st.test_change,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LabData {
        scripts_dir: s.scripts_dir,
        stores,
        seeds: s
            .seeds
            .into_iter()
            .map(|x| LabSeed {
                name: x.name,
                command: x.command,
            })
            .collect(),
    })
}

#[derive(Serialize, Deserialize)]
struct StoredAccount {
    role: String,
    username: String,
    #[serde(default)]
    default: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredLogin {
    url: String,
    username_field: String,
    password_field: String,
    submit: String,
    submit_path: String,
    success_kind: String,
    success: String,
    authenticated_url: String,
}

pub(crate) fn equipment_json(eq: &LabEquipment) -> Result<String, StoreError> {
    let (kind, value) = match &eq.login.success {
        SuccessMarker::PathPrefix(p) => ("path", p),
        SuccessMarker::Text(t) => ("text", t),
    };
    serde_json::to_string(&StoredEquipment {
        accounts: eq
            .accounts
            .iter()
            .map(|a| StoredAccount {
                role: a.role.clone(),
                username: a.username.clone(),
                default: a.default,
            })
            .collect(),
        login: StoredLogin {
            url: eq.login.url.clone(),
            username_field: eq.login.username_field.clone(),
            password_field: eq.login.password_field.clone(),
            submit: eq.login.submit.clone(),
            submit_path: eq.login.submit_path.clone(),
            success_kind: kind.to_owned(),
            success: value.clone(),
            authenticated_url: eq.login.authenticated_url.clone(),
        },
        safe_posts: eq.safe_posts.clone(),
        fixtures_dir: eq.fixtures_dir.clone(),
        data: eq.data.as_ref().map(data_stored),
    })
    .map_err(StoreError::internal)
}

pub(crate) fn equipment_of(raw: &str) -> Result<LabEquipment, StoreError> {
    let s: StoredEquipment = serde_json::from_str(raw).map_err(StoreError::internal)?;
    let success = match s.login.success_kind.as_str() {
        "path" => SuccessMarker::PathPrefix(s.login.success),
        "text" => SuccessMarker::Text(s.login.success),
        other => return Err(corrupt("login success kind", other)),
    };
    Ok(LabEquipment {
        accounts: s
            .accounts
            .into_iter()
            .map(|a| LabAccount {
                role: a.role,
                username: a.username,
                default: a.default,
            })
            .collect(),
        login: LoginForm {
            url: s.login.url,
            username_field: s.login.username_field,
            password_field: s.login.password_field,
            submit: s.login.submit,
            submit_path: s.login.submit_path,
            success,
            authenticated_url: s.login.authenticated_url,
        },
        safe_posts: s.safe_posts,
        fixtures_dir: s.fixtures_dir,
        data: s.data.map(data_of).transpose()?,
    })
}

/// Nilai lab yang tidak dikenal di database.
#[derive(Debug, thiserror::Error)]
#[error("unknown {field} value: {value}")]
struct Corrupt {
    field: &'static str,
    value: String,
}

fn corrupt(field: &'static str, value: &str) -> StoreError {
    StoreError::internal(Corrupt {
        field,
        value: value.to_owned(),
    })
}
