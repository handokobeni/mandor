//! Contract test klien Lark terhadap server HTTP tiruan dengan fixture
//! berbentuk respons Lark nyata.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

mod common;

use common::{Mock, Req, access, fixture};

use mandor_lark::{FileTokenStore, LarkError, Pkce, SearchFilter, TokenSet, authorize_url};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

#[tokio::test]
async fn search_mengikuti_halaman_dan_mengirim_filter() {
    let mock = Mock::start(Arc::new(|r: &Req| {
        if r.query_has("page_token=p2") {
            (200, fixture("search_page2.json"))
        } else {
            (200, fixture("search_page1.json"))
        }
    }))
    .await;
    let filter =
        SearchFilter::default().and("Assignee (PIC)", "contains", vec!["ou_dev".to_owned()]);
    let recs = mock
        .client()
        .search_records(
            &access(),
            "app1",
            "tbl1",
            &filter,
            &["Task name", "Task Status"],
        )
        .await
        .unwrap();
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[0].record_id, "recA");
    assert_eq!(recs[0].last_modified_ms, 1_760_000_000_000);
    assert_eq!(recs[0].last_modified_by.as_deref(), Some("ou_pm"));
    assert_eq!(recs[1].record_id, "recB");

    let log = mock.requests();
    assert_eq!(log.len(), 2);
    let first = &log[0];
    assert_eq!(first.method, "POST");
    assert_eq!(
        first.path(),
        "/open-apis/bitable/v1/apps/app1/tables/tbl1/records/search"
    );
    assert_eq!(first.auth.as_deref(), Some("Bearer u-token"));
    assert_eq!(first.body["automatic_fields"], json!(true));
    assert_eq!(
        first.body["filter"]["conditions"][0]["field_name"],
        json!("Assignee (PIC)")
    );
    assert_eq!(
        first.body["field_names"],
        json!(["Task name", "Task Status"])
    );
}

#[tokio::test]
async fn error_api_dipetakan_ke_varian_yang_tepat() {
    let mock = Mock::start(Arc::new(|r: &Req| match r.path() {
        p if p.ends_with("/records/search") => {
            (200, r#"{"code":1254290,"msg":"TooManyRequest"}"#.to_owned())
        }
        p if p.ends_with("/records/batch_get") => {
            (400, r#"{"code":99991668,"msg":"invalid token"}"#.to_owned())
        }
        p if p.contains("/records/recW") => {
            (200, r#"{"code":1254291,"msg":"Write conflict"}"#.to_owned())
        }
        p if p.contains("/records/rec429") => (429, String::new()),
        _ => (
            200,
            r#"{"code":1254045,"msg":"FieldNameNotFound"}"#.to_owned(),
        ),
    }))
    .await;
    let c = mock.client();
    let a = access();
    let e = c
        .search_records(&a, "app", "t", &SearchFilter::default(), &[])
        .await
        .unwrap_err();
    assert!(matches!(e, LarkError::RateLimited), "{e:?}");
    let e = c
        .batch_get(&a, "app", "t", &["x".to_owned()])
        .await
        .unwrap_err();
    assert!(matches!(e, LarkError::ReauthRequired), "{e:?}");
    let e = c
        .update_record(&a, "app", "t", "recW", &json!({}))
        .await
        .unwrap_err();
    assert!(matches!(e, LarkError::WriteConflict), "{e:?}");
    let e = c
        .update_record(&a, "app", "t", "rec429", &json!({}))
        .await
        .unwrap_err();
    assert!(matches!(e, LarkError::RateLimited), "{e:?}");
    let e = c
        .update_record(&a, "app", "t", "recX", &json!({}))
        .await
        .unwrap_err();
    assert!(
        matches!(
            e,
            LarkError::Api {
                code: 1_254_045,
                ..
            }
        ),
        "{e:?}"
    );
}

#[tokio::test]
async fn batch_get_dipecah_per_100_dan_update_mengirim_field() {
    let mock = Mock::start(Arc::new(|r: &Req| {
        if r.path().ends_with("/batch_get") {
            let ids = r.body["record_ids"].as_array().unwrap();
            let recs: Vec<Value> = ids.iter().map(|id| json!({"record_id": id, "fields": {}})).collect();
            (200, json!({"code":0,"data":{"records": recs}}).to_string())
        } else {
            (200, json!({"code":0,"data":{"record":{"record_id":"recA","fields": r.body["fields"].clone(),"last_modified_time": 5}}}).to_string())
        }
    }))
    .await;
    let c = mock.client();
    let ids: Vec<String> = (0..150).map(|i| format!("rec{i}")).collect();
    let recs = c.batch_get(&access(), "app", "t", &ids).await.unwrap();
    assert_eq!(recs.len(), 150);
    let updated = c
        .update_record(
            &access(),
            "app",
            "t",
            "recA",
            &json!({"mandor_branch": "mandor/pgs-1"}),
        )
        .await
        .unwrap();
    assert_eq!(updated.fields["mandor_branch"], json!("mandor/pgs-1"));
    assert_eq!(updated.last_modified_ms, 5);

    let log = mock.requests();
    let batches: Vec<_> = log
        .iter()
        .filter(|r| r.path().ends_with("/batch_get"))
        .collect();
    assert_eq!(batches.len(), 2);
    let put = log.iter().find(|r| r.method == "PUT").unwrap();
    assert_eq!(
        put.path(),
        "/open-apis/bitable/v1/apps/app/tables/t/records/recA"
    );
}

#[tokio::test]
async fn bootstrap_field_hanya_membuat_yang_belum_ada() {
    let mock = Mock::start(Arc::new(|r: &Req| {
        if r.method == "GET" {
            (
                200,
                json!({"code":0,"data":{"has_more":false,"items":[
                {"field_name":"Task name","type":1},
                {"field_name":"mandor_branch","type":1},
                {"field_name":"Fix Notes","type":1}]}})
                .to_string(),
            )
        } else {
            (
                200,
                json!({"code":0,"data":{"field":{"field_name": r.body["field_name"].clone()}}})
                    .to_string(),
            )
        }
    }))
    .await;
    let created = mock
        .client()
        .ensure_app_fields(&access(), "app", "tbl1")
        .await
        .unwrap();
    assert_eq!(
        created,
        [
            "mandor_pr",
            "mandor_owner",
            "mandor_comment_cursor",
            "mandor_modified",
            "mandor_design_gaps"
        ]
    );
    let posts: Vec<_> = mock
        .requests()
        .into_iter()
        .filter(|r| r.method == "POST")
        .collect();
    assert_eq!(posts.len(), 5);
    let gaps = posts
        .iter()
        .find(|r| r.body["field_name"] == "mandor_design_gaps")
        .unwrap();
    assert_eq!(gaps.body["type"], json!(1), "field teks");
    // Lark menolak client_token yang bukan UUIDv4 (code 1254037).
    let tokens: Vec<String> = posts
        .iter()
        .map(|r| {
            r.target
                .split("client_token=")
                .nth(1)
                .unwrap()
                .split('&')
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    for t in &tokens {
        let parts: Vec<&str> = t.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12],
            "{t}"
        );
        assert!(parts[2].starts_with('4'), "{t}");
    }
    let mut unique = tokens.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), tokens.len());
    let modified = posts
        .iter()
        .find(|r| r.body["field_name"] == "mandor_modified")
        .unwrap();
    assert_eq!(modified.body["type"], json!(1002));
}

#[tokio::test]
async fn bootstrap_field_idempoten_bila_semua_sudah_ada() {
    let mock = Mock::start(Arc::new(|r: &Req| {
        if r.method == "GET" {
            let items: Vec<Value> = mandor_lark::APP_FIELDS
                .iter()
                .map(|f| json!({"field_name": f.name, "type": f.kind}))
                .collect();
            (
                200,
                json!({"code":0,"data":{"has_more":false,"items": items}}).to_string(),
            )
        } else {
            (500, "{}".to_owned())
        }
    }))
    .await;
    let created = mock
        .client()
        .ensure_app_fields(&access(), "app", "tbl1")
        .await
        .unwrap();
    assert!(created.is_empty());
    assert!(mock.requests().iter().all(|r| r.method == "GET"));
}

#[tokio::test]
async fn komentar_dipetakan_ke_record_termasuk_balasan_lanjutan() {
    let mock = Mock::start(Arc::new(|r: &Req| {
        if r.path().ends_with("/comments/c2/replies") {
            (200, fixture("replies_c2.json"))
        } else {
            (200, fixture("comments.json"))
        }
    }))
    .await;
    let replies = mock
        .client()
        .record_replies(&access(), "app1", None)
        .await
        .unwrap();
    let ids: Vec<_> = replies
        .iter()
        .map(|r| (r.record_id.as_str(), r.reply_id.as_str()))
        .collect();
    // c3 tanpa metadata record dilewati.
    assert_eq!(ids, [("recB", "r1"), ("recB", "r2"), ("recA", "r5")]);
    assert_eq!(replies[0].text, "Tombol simpan error @ou_dev");
    assert_eq!(replies[0].author_open_id, "ou_qa");
    assert_eq!(replies[2].text, "AC 3 belum");

    let log = mock.requests();
    assert!(log[0].query_has("file_type=bitable"));
    assert!(
        log.iter()
            .any(|r| r.path() == "/open-apis/drive/v1/files/app1/comments/c2/replies")
    );
}

#[tokio::test]
async fn oauth_tukar_kode_lalu_refresh_berotasi() {
    let mock = Mock::start(Arc::new(|r: &Req| match r.path() {
        "/open-apis/authen/v2/oauth/token" => {
            let rotated = r.body["grant_type"] == "refresh_token";
            let (a, rt) = if rotated { ("u-2", "r-2") } else { ("u-1", "r-1") };
            (200, json!({"code":0,"access_token":a,"refresh_token":rt,"expires_in":7200,"refresh_token_expires_in":604_800,"token_type":"Bearer"}).to_string())
        }
        "/open-apis/authen/v1/user_info" => (200, json!({"code":0,"data":{"open_id":"ou_me","name":"Saya"}}).to_string()),
        _ => (404, "{}".to_owned()),
    }))
    .await;
    let c = mock.client();
    let pkce = Pkce::from_verifier("verifier-uji-yang-cukup-panjang-untuk-pkce-0123456789");
    let t = c.exchange_code("kode-1", &pkce, 1_000).await.unwrap();
    assert_eq!(t.access_token.expose_secret(), "u-1");
    assert_eq!(t.open_id.as_deref(), Some("ou_me"));
    assert_eq!(t.name.as_deref(), Some("Saya"), "nama akun dari user_info");
    assert_eq!(t.access_expires_at_ms, 1_000 + 7_200_000);
    assert!(t.access_expiring(1_000 + 7_200_000 - 1_000, 5_000));

    let t2 = c.refresh(&t, 2_000).await.unwrap();
    assert_eq!(t2.refresh_token.expose_secret(), "r-2");
    assert_eq!(t2.open_id.as_deref(), Some("ou_me"));
    assert_eq!(t2.name.as_deref(), Some("Saya"));

    let log = mock.requests();
    let first = &log[0];
    assert_eq!(first.body["grant_type"], json!("authorization_code"));
    assert_eq!(
        first.body["code_verifier"],
        json!(pkce.verifier.expose_secret())
    );
    assert_eq!(
        first.body["redirect_uri"],
        json!("http://127.0.0.1:53682/callback")
    );
    let refresh = log
        .iter()
        .find(|r| r.body["grant_type"] == "refresh_token")
        .unwrap();
    assert_eq!(refresh.body["refresh_token"], json!("r-1"));
}

#[tokio::test]
async fn refresh_token_dicabut_meminta_login_ulang() {
    let mock = Mock::start(Arc::new(|_: &Req| {
        (400, json!({"code":20064,"error":"invalid_grant","error_description":"refresh token revoked"}).to_string())
    }))
    .await;
    let t = TokenSet {
        access_token: SecretString::from("u".to_owned()),
        refresh_token: SecretString::from("r".to_owned()),
        access_expires_at_ms: 0,
        refresh_expires_at_ms: 0,
        open_id: None,
        name: None,
    };
    let e = mock.client().refresh(&t, 0).await.unwrap_err();
    assert!(matches!(e, LarkError::ReauthRequired), "{e:?}");
}

#[tokio::test]
async fn url_otorisasi_memuat_pkce_state_dan_scope() {
    let mock = Mock::start(Arc::new(|_: &Req| (200, "{}".to_owned()))).await;
    let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    let url = authorize_url(&mock.client(), "st4te", &pkce);
    assert!(url.starts_with(&format!("{}/open-apis/authen/v1/authorize?", mock.base)));
    assert!(url.contains("client_id=cli_app"));
    assert!(url.contains("state=st4te"));
    assert!(url.contains("code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A53682%2Fcallback"));
    assert!(url.contains("scope=bitable%3Aapp%20"));
    // `users/batch` butuh akses kontak dasar selain field dasar pengguna.
    assert!(url.contains("contact%3Acontact.base%3Areadonly"), "{url}");
    // Nama orang di UI dibaca lewat Contact API dengan token user.
    assert!(url.contains("contact%3Auser.base%3Areadonly"));
}

#[tokio::test]
async fn token_store_atomik_0600_dan_bisa_dihapus() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileTokenStore::new(dir.path().join("lark").join("token.json"));
    assert!(store.load().await.unwrap().is_none());
    let t = TokenSet {
        access_token: SecretString::from("u-x".to_owned()),
        refresh_token: SecretString::from("r-x".to_owned()),
        access_expires_at_ms: 10,
        refresh_expires_at_ms: 20,
        open_id: Some("ou_me".to_owned()),
        name: None,
    };
    store.save(&t).await.unwrap();
    let back = store.load().await.unwrap().unwrap();
    assert_eq!(back.refresh_token.expose_secret(), "r-x");
    assert_eq!(back.open_id.as_deref(), Some("ou_me"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join("lark/token.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    std::fs::write(dir.path().join("lark/token.json"), "bukan json").unwrap();
    assert!(matches!(store.load().await, Err(LarkError::Io(_))));
    store.clear().await.unwrap();
    store.clear().await.unwrap();
    assert!(store.load().await.unwrap().is_none());
}

#[tokio::test]
async fn nama_pengguna_dari_contact_batch_per_50_id() {
    let mock = Mock::start(Arc::new(|_: &Req| (200, fixture("users_batch.json")))).await;
    let ids: Vec<String> = std::iter::once("ou_a".to_owned())
        .chain(std::iter::once("ou_b".to_owned()))
        .chain((0..50).map(|i| format!("ou_x{i}")))
        .collect();
    let names = mock.client().user_names(&access(), &ids).await.unwrap();
    assert_eq!(names.get("ou_a").map(String::as_str), Some("Beni Handoko"));
    // Nama kosong memakai nama Inggris.
    assert_eq!(names.get("ou_b").map(String::as_str), Some("Rizky Pratama"));

    let log = mock.requests();
    assert_eq!(log.len(), 2, "52 ID dipecah per 50");
    assert_eq!(log[0].method, "GET");
    assert_eq!(log[0].path(), "/open-apis/contact/v3/users/batch");
    assert!(log[0].query_has("user_id_type=open_id"));
    assert!(log[0].query_has("user_ids=ou_a") && log[0].query_has("user_ids=ou_b"));
    assert_eq!(log[0].auth.as_deref(), Some("Bearer u-token"));
}
