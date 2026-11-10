//! Sesi lab data dan adapter `PostgreSQL` (plan 2026-11-10-001, U12).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mandor_domain::lab_data::SessionPhase;
use mandor_store::{LabSessionsStore, Store};
use mandor_usecase::run_ports::{LabSessionRecord, LabSessions};
use pretty_assertions::assert_eq;

async fn sessions(dir: &tempfile::TempDir) -> LabSessionsStore {
    LabSessionsStore::new(Store::open(&dir.path().join("mandor.db")).await.unwrap())
}

fn record(id: &str, phase: SessionPhase) -> LabSessionRecord {
    LabSessionRecord {
        id: id.to_owned(),
        run_id: "run1".to_owned(),
        repo_id: "repo1".to_owned(),
        lock_keys: vec![
            "dir:/srv/uploads".to_owned(),
            "pg:local:5432/kelola".to_owned(),
        ],
        phase,
        snapshot_dir: Some("/home/dev/.mandor/lab/repo1/private/snap-1".into()),
        scripts_digest: "ab".repeat(32),
        fingerprints: Vec::new(),
        error: None,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

#[tokio::test]
async fn sesi_bolak_balik_dan_sidik_tersimpan() {
    let dir = tempfile::tempdir().unwrap();
    let s = sessions(&dir).await;
    let rec = record("s1", SessionPhase::Snapshotting);
    s.create_session(&rec).await.unwrap();
    assert_eq!(s.session("s1").await.unwrap(), Some(rec.clone()));
    let fps = vec![("tenant".to_owned(), "aa".to_owned())];
    s.set_snapshot("s1", std::path::Path::new("/snap/s1"), &fps, 5)
        .await
        .unwrap();
    let got = s.session("s1").await.unwrap().unwrap();
    assert_eq!(got.fingerprints, fps);
    assert_eq!(got.snapshot_dir, Some("/snap/s1".into()));
    assert_eq!(got.updated_at_ms, 5);
    assert_eq!(s.session("tidak-ada").await.unwrap(), None);
}

/// Dua transisi bersamaan dari `running`: hanya satu berhasil.
#[tokio::test]
async fn transisi_bersamaan_hanya_satu_menang() {
    let dir = tempfile::tempdir().unwrap();
    let s = sessions(&dir).await;
    s.create_session(&record("s1", SessionPhase::Running))
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        s.transition(
            "s1",
            SessionPhase::Running,
            SessionPhase::Restoring,
            None,
            2
        ),
        s.transition(
            "s1",
            SessionPhase::Running,
            SessionPhase::Restoring,
            None,
            3
        ),
    );
    assert_eq!([a.unwrap(), b.unwrap()].iter().filter(|x| **x).count(), 1);
    assert!(
        s.transition(
            "s1",
            SessionPhase::Restoring,
            SessionPhase::RestoreFailed,
            Some("restoring tenant failed"),
            4
        )
        .await
        .unwrap()
    );
    let got = s.session("s1").await.unwrap().unwrap();
    assert_eq!(got.phase, SessionPhase::RestoreFailed);
    assert_eq!(got.error.as_deref(), Some("restoring tenant failed"));
    // Fase lama tidak lagi cocok.
    assert!(
        !s.transition("s1", SessionPhase::Running, SessionPhase::Done, None, 5)
            .await
            .unwrap()
    );
}

/// KTD-L22: pemulihan hanya untuk fase yang belum selesai; `restore_failed`
/// tetap memegang kunci tetapi menunggu developer.
#[tokio::test]
async fn sesi_untuk_pemulihan_dan_pemegang_kunci() {
    let dir = tempfile::tempdir().unwrap();
    let s = sessions(&dir).await;
    for (id, phase) in [
        ("a", SessionPhase::Snapshotting),
        ("b", SessionPhase::Running),
        ("c", SessionPhase::Restoring),
        ("d", SessionPhase::RestoreWaiting),
        ("e", SessionPhase::RestoreFailed),
        ("f", SessionPhase::Done),
    ] {
        s.create_session(&record(id, phase)).await.unwrap();
    }
    let ids = |v: Vec<LabSessionRecord>| v.into_iter().map(|r| r.id).collect::<Vec<_>>();
    assert_eq!(
        ids(s.pending_recovery().await.unwrap()),
        ["a", "b", "c", "d"]
    );
    assert_eq!(ids(s.holding().await.unwrap()), ["a", "b", "c", "d", "e"]);
}

/// Test `PostgreSQL` sungguhan (`MANDOR_E2E=1`): server superuser `postgres`
/// dengan password `mandor-e2e` di `127.0.0.1:55432`, mis. kontainer
/// sementara `docker run --rm -d -p 127.0.0.1:55432:5432
/// -e POSTGRES_PASSWORD=mandor-e2e postgres:18`.
mod pg {
    use std::time::Duration;

    use mandor_domain::lab_data::{DbTarget, LabDataBlock, role_block};
    use mandor_store::PgLabDatabase;
    use mandor_usecase::run_ports::{DbCredentials, LabDatabase};
    use sqlx::Connection;

    fn enabled() -> bool {
        std::env::var("MANDOR_E2E").is_ok_and(|v| v == "1")
    }

    fn creds(user: &str, password: &str) -> DbCredentials {
        DbCredentials {
            target: DbTarget {
                host: "127.0.0.1".to_owned(),
                port: 55432,
                database: "postgres".to_owned(),
            },
            user: user.to_owned(),
            password: secrecy::SecretString::from(password.to_owned()),
        }
    }

    async fn setup() {
        let mut c = sqlx::PgConnection::connect(
            "postgres://postgres:mandor-e2e@127.0.0.1:55432/postgres?sslmode=disable",
        )
        .await
        .unwrap();
        for sql in [
            "DROP TABLE IF EXISTS kb_values",
            "DROP ROLE IF EXISTS lab_min",
            "DROP ROLE IF EXISTS lab_wide",
            "REVOKE CREATE ON SCHEMA public FROM PUBLIC",
            "CREATE TABLE kb_values (id int primary key, score int)",
            "CREATE ROLE lab_min LOGIN PASSWORD 'lab-min-pass'",
            "GRANT SELECT, INSERT, UPDATE, DELETE ON kb_values TO lab_min",
            "CREATE ROLE lab_wide LOGIN PASSWORD 'lab-wide-pass' IN ROLE pg_read_all_data",
        ] {
            sqlx::query(sql).execute(&mut c).await.unwrap();
        }
        c.close().await.unwrap();
    }

    async fn exec(sqls: &[&'static str]) {
        let mut c = sqlx::PgConnection::connect(
            "postgres://postgres:mandor-e2e@127.0.0.1:55432/postgres?sslmode=disable",
        )
        .await
        .unwrap();
        for sql in sqls {
            sqlx::query(*sql).execute(&mut c).await.unwrap();
        }
        c.close().await.unwrap();
    }

    async fn extensions_follow_usability(db: &PgLabDatabase, admin: &DbCredentials) {
        let block = |p: &mandor_domain::lab_data::RolePrivileges| role_block(p);
        exec(&[
            "DROP EXTENSION IF EXISTS dblink CASCADE",
            "DROP EXTENSION IF EXISTS postgres_fdw CASCADE",
            "CREATE EXTENSION dblink",
        ])
        .await;
        // Fungsi dblink bisa dijalankan PUBLIC: role lab bisa membuka koneksi.
        let p = db.role_privileges(admin, "lab_min").await.unwrap();
        assert_eq!(
            block(&p),
            Some(LabDataBlock::Extension("dblink".to_owned())),
            "{p:?}"
        );
        exec(&["REVOKE EXECUTE ON ALL FUNCTIONS IN SCHEMA public FROM PUBLIC"]).await;
        let p = db.role_privileges(admin, "lab_min").await.unwrap();
        assert_eq!(block(&p), None, "{p:?}");
        // postgres_fdw milik app: fungsi handler bisa dijalankan PUBLIC, tetapi
        // tanpa USAGE server dan tanpa user mapping role lab tidak bisa memakai.
        exec(&[
            "CREATE EXTENSION postgres_fdw",
            "CREATE SERVER landlord_server FOREIGN DATA WRAPPER postgres_fdw OPTIONS (dbname 'postgres')",
            "CREATE USER MAPPING FOR postgres SERVER landlord_server OPTIONS (user 'postgres', password 'mandor-e2e')",
            "CREATE FOREIGN TABLE remote_kb (id int) SERVER landlord_server OPTIONS (table_name 'kb_values')",
        ])
        .await;
        let p = db.role_privileges(admin, "lab_min").await.unwrap();
        assert_eq!(block(&p), None, "{p:?}");
        exec(&["GRANT SELECT ON remote_kb TO lab_min"]).await;
        let p = db.role_privileges(admin, "lab_min").await.unwrap();
        assert_eq!(
            block(&p),
            Some(LabDataBlock::Extension("postgres_fdw".to_owned())),
            "{p:?}"
        );
        exec(&[
            "REVOKE SELECT ON remote_kb FROM lab_min",
            "GRANT USAGE ON FOREIGN SERVER landlord_server TO lab_min",
        ])
        .await;
        let p = db.role_privileges(admin, "lab_min").await.unwrap();
        assert_eq!(
            block(&p),
            Some(LabDataBlock::Extension("postgres_fdw".to_owned())),
            "{p:?}"
        );
        exec(&[
            "DROP EXTENSION postgres_fdw CASCADE",
            "DROP EXTENSION dblink CASCADE",
        ])
        .await;
    }

    #[tokio::test]
    async fn adapter_pada_postgres_sungguhan() {
        if !enabled() {
            return;
        }
        setup().await;
        let db = PgLabDatabase::with_statement_timeout(Duration::from_secs(1));
        let admin = creds("postgres", "mandor-e2e");
        let lab = creds("lab_min", "lab-min-pass");
        assert!(db.server_is_local(&admin).await.unwrap());
        assert!(db.reads_all_stats(&admin).await.unwrap());
        assert!(
            !db.reads_all_stats(&lab).await.unwrap(),
            "role lab tanpa pg_read_all_stats"
        );
        // Kueri hak role dipetakan aturan positif U11.
        let min = db.role_privileges(&admin, "lab_min").await.unwrap();
        assert_eq!(role_block(&min), None, "{min:?}");
        let wide = db.role_privileges(&admin, "lab_wide").await.unwrap();
        assert!(
            matches!(role_block(&wide), Some(LabDataBlock::MemberOf(r)) if r == "pg_read_all_data")
        );
        assert!(db.role_privileges(&admin, "tidak_ada").await.is_err());
        // Extension terlarang hanya menolak bila role lab bisa memakainya.
        extensions_follow_usability(&db, &admin).await;
        // pg_stat_activity terbaca beserta pid sendiri.
        let (rows, own) = db.activity(&admin).await.unwrap();
        assert!(rows.iter().any(|r| r.pid == own));
        assert!(
            db.tables(&admin)
                .await
                .unwrap()
                .contains(&"public.kb_values".to_owned())
        );
        // Pernyataan lab berjalan dengan role lab; yang lambat dihentikan.
        db.run_statement(&lab, "INSERT INTO kb_values VALUES (1, 4)")
            .await
            .unwrap();
        let r = db
            .run_statement(&lab, "SELECT id, score FROM kb_values")
            .await
            .unwrap();
        assert_eq!(r.rows, 1);
        assert!(r.text.contains("1\t4"), "{}", r.text);
        assert!(db.run_statement(&lab, "SELECT pg_sleep(3)").await.is_err());
        // Hasil besar dibaca bertahap dan dipotong, tidak dimuat seluruhnya.
        let big = db
            .run_statement(&lab, "SELECT g FROM generate_series(1, 200000) g")
            .await
            .unwrap();
        assert!(big.truncated);
        assert!(big.rows <= 1_000, "{}", big.rows);
        assert!(
            db.run_statement(&lab, "CREATE TABLE x (a int)")
                .await
                .is_err()
        );
    }
}
