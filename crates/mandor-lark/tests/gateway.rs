//! Port `LarkTasks` di atas klien Lark: pemetaan field, filter, token, dan
//! validasi opsi sebelum tulis.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{Mock, Req, fixture};
use mandor_domain::{AppField, Claim, TaskStatus};
use mandor_lark::{
    FileTokenStore, LarkGateway, RawRecord, RetryPolicy, TableRef, TokenSet, to_task, write_body,
};
use mandor_usecase::{LarkTasks, PortError};
use pretty_assertions::assert_eq;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

fn now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

fn tokens(access_expires_at_ms: i64) -> TokenSet {
    TokenSet {
        access_token: SecretString::from("u-lama".to_owned()),
        refresh_token: SecretString::from("r-lama".to_owned()),
        access_expires_at_ms,
        refresh_expires_at_ms: now_ms() + 86_400_000,
        open_id: Some("ou_dev".to_owned()),
        name: None,
    }
}

async fn gateway(
    mock: &Mock,
    stored: Option<TokenSet>,
) -> (LarkGateway, FileTokenStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = FileTokenStore::new(dir.path().join("token.json"));
    if let Some(t) = stored {
        store.save(&t).await.unwrap();
    }
    let gw = LarkGateway::new(
        mock.client(),
        TableRef {
            app_token: "app1".to_owned(),
            table_id: "tbl1".to_owned(),
            ac_table_id: "tblAC".to_owned(),
        },
        store.clone(),
    )
    .with_retry_policy(RetryPolicy {
        attempts: 2,
        base: Duration::from_millis(1),
        max: Duration::from_millis(2),
    });
    (gw, store, dir)
}

fn raw(v: &Value) -> RawRecord {
    RawRecord {
        record_id: v["record_id"].as_str().unwrap().to_owned(),
        fields: v["fields"].clone(),
        last_modified_ms: v["last_modified_time"].as_i64().unwrap(),
        last_modified_by: v["last_modified_by"]["id"].as_str().map(str::to_owned),
    }
}

#[test]
fn field_user_link_select_url_dan_text_dipetakan_ke_task() {
    let v: Value = serde_json::from_str(&fixture("record_full.json")).unwrap();
    let t = to_task(&raw(&v));
    assert_eq!(t.record_id, "recFull");
    assert_eq!(t.key.as_deref(), Some("PGS-1388"));
    assert_eq!(t.title, "Profile Data: Tab Data");
    assert_eq!(t.status, TaskStatus::WaitingFix);
    assert_eq!(t.readiness.as_deref(), Some("Ready To Dev"));
    assert_eq!(t.assignees, ["ou_dev"]);
    assert_eq!(t.reporter.as_deref(), Some("ou_pm"));
    assert_eq!(t.ac_ids, ["ac1", "ac2"]);
    assert_eq!(t.sprint_ids, ["spr9"]);
    assert_eq!(t.kanban_mirror.as_deref(), Some("review"));
    assert_eq!(t.branch.as_deref(), Some("mandor/pgs-1388"));
    assert_eq!(t.pr_url.as_deref(), Some("https://github.com/o/r/pull/7"));
    assert_eq!(
        t.claim,
        Some(Claim {
            open_id: "ou_dev".to_owned(),
            ts_ms: 1_760_000_000_000,
            nonce: "abc".to_owned()
        })
    );
    assert_eq!(t.comment_cursor.as_deref(), Some("1760000100:r1"));
    assert_eq!(t.fix_notes.as_deref(), Some("Perbaiki validasi"));
    assert_eq!(t.last_modified_ms, 1_760_000_000_000);
    assert_eq!(t.last_modified_by.as_deref(), Some("ou_pm"));
}

#[test]
fn badan_tulis_hanya_berisi_field_milik_app_dan_status() {
    let body = write_body(
        Some(&TaskStatus::InProgress),
        &[
            AppField::Branch("mandor/pgs-1".to_owned()),
            AppField::PrUrl("https://x/pull/1".to_owned()),
            AppField::Claim(None),
            AppField::KanbanMirror("in_progress".to_owned()),
        ],
    );
    assert_eq!(
        body,
        json!({
            "Task Status": "In Progress",
            "mandor_branch": "mandor/pgs-1",
            "mandor_pr": {"text": "https://x/pull/1", "link": "https://x/pull/1"},
            "mandor_owner": null,
            "kanban_column": "in_progress"
        })
    );
}

#[test]
fn link_pr_banyak_repo_memakai_url_pertama_dan_teks_lengkap_terbaca_kembali() {
    let urls = "https://x/api/pull/1\nhttps://x/web/pull/2";
    let body = write_body(None, &[AppField::PrUrl(urls.to_owned())]);
    assert_eq!(
        body,
        json!({"mandor_pr": {"text": urls, "link": "https://x/api/pull/1"}})
    );
    let t = to_task(&RawRecord {
        record_id: "rec1".to_owned(),
        fields: body,
        last_modified_ms: 0,
        last_modified_by: None,
    });
    assert_eq!(t.pr_url.as_deref(), Some(urls));
}

#[tokio::test]
async fn pencarian_memfilter_assignee_dan_waktu_ubah() {
    let mock = Mock::start(Arc::new(|r: &Req| {
        if r.path().ends_with("/fields") {
            (200, schema_with(&["Assignee (PIC)", "mandor_modified"]))
        } else if r.query_has("page_token=p2") {
            (200, fixture("search_page2.json"))
        } else {
            (200, fixture("search_page1.json"))
        }
    }))
    .await;
    let (gw, _s, _d) = gateway(&mock, Some(tokens(now_ms() + 3_600_000))).await;
    let tasks = gw
        .search_assigned("ou_dev", Some(2 * 86_400_000))
        .await
        .unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].status, TaskStatus::parse("Ready to Dev"));
    let req = searches(&mock).remove(0);
    assert_eq!(req.auth.as_deref(), Some("Bearer u-lama"));
    let conds = &req.body["filter"]["conditions"];
    assert_eq!(
        conds[0],
        json!({"field_name": "Assignee (PIC)", "operator": "contains", "value": ["ou_dev"]})
    );
    assert_eq!(
        conds[1],
        json!({"field_name": "mandor_modified", "operator": "isGreater", "value": ["ExactDate", "86400000"]})
    );
    // Rekonsiliasi penuh: tanpa filter waktu.
    gw.search_assigned("ou_dev", None).await.unwrap();
    let full = searches(&mock).pop().unwrap();
    assert_eq!(
        full.body["filter"]["conditions"].as_array().unwrap().len(),
        1
    );
}

#[tokio::test]
async fn papan_tim_mengambil_kartu_belum_done_dan_done_terbaru_tanpa_duplikat() {
    let mock = Mock::start(Arc::new(|r: &Req| {
        if r.path().ends_with("/fields") {
            (200, schema_with(&["Assignee (PIC)", "mandor_modified"]))
        } else if r.query_has("page_token=p2") {
            (200, fixture("search_page2.json"))
        } else {
            (200, fixture("search_page1.json"))
        }
    }))
    .await;
    let (gw, _s, _d) = gateway(&mock, Some(tokens(now_ms() + 3_600_000))).await;
    let tasks = gw.search_team(2 * 86_400_000).await.unwrap();
    // Kedua pencarian mengembalikan record yang sama: digabung tanpa duplikat.
    assert_eq!(tasks.len(), 2);
    let reqs = searches(&mock);
    let first = reqs
        .iter()
        .find(|r| r.body["filter"]["conditions"][0]["operator"] == "isNot")
        .unwrap();
    let active = &first.body["filter"];
    assert_eq!(active["conjunction"], "and");
    assert_eq!(
        active["conditions"],
        json!([
            {"field_name": "Task Status", "operator": "isNot", "value": ["Waiting Deploy"]},
            {"field_name": "Task Status", "operator": "isNot", "value": ["Delivered"]}
        ])
    );
    assert!(reqs.iter().any(|r| r.body["filter"]["conditions"]
        == json!([{"field_name": "mandor_modified", "operator": "isGreater", "value": ["ExactDate", "86400000"]}])));
}

#[tokio::test]
async fn tanpa_field_mandor_modified_poll_inkremental_menjadi_penuh() {
    // Bootstrap field gagal atau belum jalan: filter field yang tidak ada ditolak
    // Lark (InvalidFilter), jadi pencarian tidak boleh memakainya.
    let mock = Mock::start(Arc::new(|r: &Req| {
        if r.path().ends_with("/fields") {
            (200, schema_with(&["Assignee (PIC)"]))
        } else {
            (200, fixture("search_page2.json"))
        }
    }))
    .await;
    let (gw, _s, _d) = gateway(&mock, Some(tokens(now_ms() + 3_600_000))).await;
    gw.search_assigned("ou_dev", Some(2 * 86_400_000))
        .await
        .unwrap();
    let req = searches(&mock).remove(0);
    let conds = req.body["filter"]["conditions"].as_array().unwrap().clone();
    assert_eq!(conds.len(), 1);
    assert_eq!(conds[0]["field_name"], "Assignee (PIC)");
}

fn schema_with(names: &[&str]) -> String {
    let items: Vec<Value> = names
        .iter()
        .map(|n| json!({"field_name": n, "type": 1, "property": null}))
        .collect();
    json!({"code": 0, "msg": "ok", "data": {"has_more": false, "items": items}}).to_string()
}

fn searches(mock: &Mock) -> Vec<Req> {
    mock.requests()
        .into_iter()
        .filter(|r| r.path().ends_with("/records/search"))
        .collect()
}

#[tokio::test]
async fn token_hampir_habis_diperbarui_dan_pasangan_baru_disimpan_sebelum_dipakai() {
    let mock = Mock::start(Arc::new(|r: &Req| match r.path() {
        "/open-apis/authen/v2/oauth/token" => (
            200,
            json!({"code":0,"access_token":"u-baru","refresh_token":"r-baru","expires_in":7200,"refresh_token_expires_in":604_800}).to_string(),
        ),
        _ => (200, json!({"code":0,"data":{"records":[]}}).to_string()),
    }))
    .await;
    let (gw, store, _d) = gateway(&mock, Some(tokens(now_ms() + 60_000))).await;
    gw.fetch(&["rec1".to_owned()]).await.unwrap();
    let saved = store.load().await.unwrap().unwrap();
    assert_eq!(saved.refresh_token.expose_secret(), "r-baru");
    assert_eq!(saved.open_id.as_deref(), Some("ou_dev"));
    let log = mock.requests();
    assert_eq!(log[0].body["refresh_token"], json!("r-lama"));
    assert_eq!(log[1].auth.as_deref(), Some("Bearer u-baru"));
    // Panggilan berikutnya memakai token di memori tanpa refresh lagi.
    gw.fetch(&["rec1".to_owned()]).await.unwrap();
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.path().contains("oauth"))
            .count(),
        1
    );
}

#[tokio::test]
async fn tanpa_token_atau_refresh_dicabut_meminta_login_ulang() {
    let mock = Mock::start(Arc::new(|_: &Req| {
        (
            400,
            json!({"code":20037,"error":"invalid_grant"}).to_string(),
        )
    }))
    .await;
    let (gw, _s, _d) = gateway(&mock, None).await;
    assert_eq!(
        gw.fetch(&["x".to_owned()]).await.unwrap_err(),
        PortError::Reauth
    );
    let (gw, _s, _d) = gateway(&mock, Some(tokens(0))).await;
    assert_eq!(
        gw.fetch(&["x".to_owned()]).await.unwrap_err(),
        PortError::Reauth
    );
    // Tidak ada retry berulang untuk login ulang.
    assert_eq!(mock.requests().len(), 1);
}

#[tokio::test]
async fn opsi_select_tidak_dikenal_ditolak_sebelum_put() {
    let mock = Mock::start(Arc::new(|r: &Req| {
        if r.method == "GET" {
            (200, fixture("fields.json"))
        } else {
            (200, json!({"code":0,"data":{"record":{"record_id":"rec1","fields":r.body["fields"].clone(),"last_modified_time":3}}}).to_string())
        }
    }))
    .await;
    let (gw, _s, _d) = gateway(&mock, Some(tokens(now_ms() + 3_600_000))).await;
    let err = gw
        .write(
            "rec1",
            Some(&TaskStatus::Unknown("Blocked".to_owned())),
            &[],
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, PortError::Rejected(m) if m.contains("Blocked")),
        "{err:?}"
    );
    assert!(mock.requests().iter().all(|r| r.method != "PUT"));

    let t = gw
        .write(
            "rec1",
            Some(&TaskStatus::InProgress),
            &[AppField::KanbanMirror("in_progress".to_owned())],
        )
        .await
        .unwrap();
    assert_eq!(t.status, TaskStatus::InProgress);
    // Daftar field dibaca sekali lalu di-cache.
    assert_eq!(
        mock.requests().iter().filter(|r| r.method == "GET").count(),
        1
    );
    let put = mock
        .requests()
        .into_iter()
        .find(|r| r.method == "PUT")
        .unwrap();
    assert_eq!(
        put.path(),
        "/open-apis/bitable/v1/apps/app1/tables/tbl1/records/rec1"
    );
}

#[tokio::test]
async fn throttle_dicoba_ulang_lalu_berhasil() {
    let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let c2 = Arc::clone(&calls);
    let mock = Mock::start(Arc::new(move |_: &Req| {
        if c2.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            (
                200,
                json!({"code":1_254_290,"msg":"TooManyRequest"}).to_string(),
            )
        } else {
            (200, json!({"code":0,"data":{"records":[]}}).to_string())
        }
    }))
    .await;
    let (gw, _s, _d) = gateway(&mock, Some(tokens(now_ms() + 3_600_000))).await;
    assert!(gw.fetch(&["rec1".to_owned()]).await.unwrap().is_empty());
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn ac_dibaca_dari_tabel_tertaut_berurutan_dan_droped_ditandai() {
    let mock = Mock::start(Arc::new(|r: &Req| {
        assert!(r.path().contains("/tables/tblAC/records/batch_get"), "{}", r.path());
        (
            200,
            json!({"code":0,"data":{"records":[
                {"record_id":"ac2","fields":{"Acceptance Criteria":[{"text":"Email tampil","type":"text"}],"AC id":"AC-002","AC Type":"Droped"}},
                {"record_id":"ac1","fields":{"Acceptance Criteria":[{"text":"Nama tampil","type":"text"}],"Description":[{"text":"Huruf kapital","type":"text"}],"AC id":"AC-001","AC Type":"High"}}
            ]}})
            .to_string(),
        )
    }))
    .await;
    let (gw, _s, _d) = gateway(&mock, Some(tokens(now_ms() + 3_600_000))).await;
    let acs = gw
        .acceptance_criteria(&["ac1".to_owned(), "ac2".to_owned()])
        .await
        .unwrap();
    assert_eq!(acs.len(), 2);
    assert_eq!(acs[0].label.as_deref(), Some("AC-001"));
    assert_eq!(acs[0].text, "Nama tampil\nHuruf kapital");
    assert!(!acs[0].dropped);
    assert!(acs[1].dropped);
    assert!(gw.acceptance_criteria(&[]).await.unwrap().is_empty());
}

#[tokio::test]
async fn feedback_qa_dan_pengubah_terakhir_ac_ikut_dibaca() {
    let mock = Mock::start(Arc::new(|_: &Req| {
        (
            200,
            json!({"code":0,"data":{"records":[
                {"record_id":"ac1",
                 "fields":{"Acceptance Criteria":[{"text":"Nama tampil","type":"text"}],
                           "Feedback QA":[{"text":"Masih huruf kecil","type":"text"}]},
                 "last_modified_time":1_792_598_000_000_i64,
                 "last_modified_by":{"id":"ou_qa","name":"QA"}},
                {"record_id":"ac2","fields":{"Acceptance Criteria":[{"text":"Email","type":"text"}]}}
            ]}})
            .to_string(),
        )
    }))
    .await;
    let (gw, _s, _d) = gateway(&mock, Some(tokens(now_ms() + 3_600_000))).await;
    let acs = gw
        .acceptance_criteria(&["ac1".to_owned(), "ac2".to_owned()])
        .await
        .unwrap();
    assert_eq!(acs[0].feedback_qa.as_deref(), Some("Masih huruf kecil"));
    assert_eq!(acs[0].last_modified_ms, 1_792_598_000_000);
    assert_eq!(acs[0].last_modified_by.as_deref(), Some("ou_qa"));
    assert_eq!(acs[1].feedback_qa, None);
    assert_eq!(acs[1].last_modified_by, None);
}

#[tokio::test]
async fn sprint_dibaca_dari_tabel_tertaut_field_sprint() {
    let mock = Mock::start(Arc::new(|r: &Req| {
        if r.path().ends_with("/tables/tbl1/fields") {
            return (
                200,
                json!({"code": 0, "msg": "ok", "data": {"has_more": false, "items": [
                    {"field_name": "Task name", "type": 1, "property": null},
                    {"field_name": "Sprint", "type": 18,
                     "property": {"multiple": false, "table_id": "tblSP", "table_name": "Sprint Status"}}
                ]}})
                .to_string(),
            );
        }
        if r.path().ends_with("/tables/tblSP/records/batch_get") {
            return (
                200,
                json!({"code": 0, "msg": "ok", "data": {"records": [
                    {"record_id": "spr2", "fields": {
                        "Sprint": [{"type": "text", "text": "Sprint 42"}],
                        "Sprint Status": "In Progress",
                        "Start Date": 1_760_313_600_000_i64,
                        "End Date": 1_761_264_000_000_i64
                    }, "last_modified_time": 1},
                    {"record_id": "spr1", "fields": {
                        "Sprint": [{"type": "text", "text": "Sprint 41"}]
                    }, "last_modified_time": 1}
                ]}})
                .to_string(),
            );
        }
        (404, "{}".to_owned())
    }))
    .await;
    let (gw, _store, _dir) = gateway(&mock, Some(tokens(now_ms() + 3_600_000))).await;

    let sprints = gw
        .sprints(&["spr1".to_owned(), "spr2".to_owned()])
        .await
        .unwrap();
    assert_eq!(sprints.len(), 2);
    assert_eq!(sprints[0].record_id, "spr1");
    assert_eq!(sprints[0].name, "Sprint 41");
    assert_eq!(
        (sprints[0].start_ms, sprints[0].status.as_deref()),
        (None, None)
    );
    assert_eq!(sprints[1].name, "Sprint 42");
    assert_eq!(sprints[1].status.as_deref(), Some("In Progress"));
    assert_eq!(
        (sprints[1].start_ms, sprints[1].end_ms),
        (Some(1_760_313_600_000), Some(1_761_264_000_000))
    );
    let batch = mock
        .requests()
        .into_iter()
        .find(|r| r.path().ends_with("/batch_get"))
        .unwrap();
    assert_eq!(batch.body["record_ids"], json!(["spr1", "spr2"]));
}
