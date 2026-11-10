//! Adapter `PostgreSQL` untuk database lab (plan 2026-11-10-001, U12,
//! KTD-L15, KTD-L16, KTD-L20).
//!
//! Hanya kueri runtime (`sqlx::query`) tanpa makro, sehingga cache `.sqlx`
//! tetap milik `SQLite`. Setiap operasi membuka satu koneksi tanpa TLS dengan
//! opsi eksplisit dari kredensial (bukan DSN bebas), dibatasi waktu koneksi dan
//! pernyataan, lalu menutupnya.

use std::time::Duration;

use mandor_domain::lab_data::{
    ExtensionAccess, PgActivity, RoleAttribute, RolePrivileges, check_statement, is_local_db,
    server_addr_local,
};
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{DbCredentials, LabDatabase, SqlResult};
use secrecy::ExposeSecret;
use sqlx::postgres::{PgConnectOptions, PgConnection, PgRow, PgSslMode};
use sqlx::{AssertSqlSafe, Column, Connection, Row};

/// Batas waktu membuka koneksi.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Batas waktu kueri katalog daemon.
const CATALOG_TIMEOUT: Duration = Duration::from_secs(15);
/// Batas waktu satu pernyataan `lab_sql` (KTD-L16).
pub const STATEMENT_TIMEOUT: Duration = Duration::from_secs(60);
/// Batas keluaran `lab_sql` (KTD-L16).
pub const MAX_OUTPUT: usize = 64 * 1024;
/// Nama aplikasi koneksi daemon di `pg_stat_activity`.
const APPLICATION_NAME: &str = "mandor-lab";

/// Database lab lewat koneksi milik daemon.
#[derive(Debug, Clone, Default)]
pub struct PgLabDatabase {
    statement_timeout: Option<Duration>,
}

impl PgLabDatabase {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Batas waktu pernyataan lain (test).
    #[must_use]
    pub fn with_statement_timeout(timeout: Duration) -> Self {
        Self {
            statement_timeout: Some(timeout),
        }
    }

    fn statement_timeout(&self) -> Duration {
        self.statement_timeout.unwrap_or(STATEMENT_TIMEOUT)
    }
}

fn db_error(e: impl std::fmt::Display) -> PortError {
    PortError::Transient(format!("lab database: {e}"))
}

fn timed_out(what: &str) -> PortError {
    PortError::Transient(format!("lab database: {what} timed out"))
}

/// Opsi koneksi dari kredensial; host bukan lokal ditolak sebelum koneksi
/// dibuka (KTD-L15).
fn options(creds: &DbCredentials, statement: Duration) -> Result<PgConnectOptions, PortError> {
    let t = &creds.target;
    if !is_local_db(&t.host) {
        return Err(PortError::Rejected(format!(
            "lab only runs against local databases ({} is not local)",
            t.host
        )));
    }
    let base = PgConnectOptions::new_without_pgpass();
    let base = if t.host.starts_with('/') {
        base.socket(&t.host)
    } else {
        base.host(t.host.trim_start_matches('[').trim_end_matches(']'))
    };
    let millis = statement.as_millis().to_string();
    Ok(base
        .port(t.port)
        .database(&t.database)
        .username(&creds.user)
        .password(creds.password.expose_secret())
        .ssl_mode(PgSslMode::Disable)
        .application_name(APPLICATION_NAME)
        .options([("statement_timeout", millis.as_str())]))
}

async fn connect(creds: &DbCredentials, statement: Duration) -> Result<PgConnection, PortError> {
    let opts = options(creds, statement)?;
    tokio::time::timeout(CONNECT_TIMEOUT, PgConnection::connect_with(&opts))
        .await
        .map_err(|_| timed_out("connecting"))?
        .map_err(db_error)
}

async fn close(conn: PgConnection) {
    if tokio::time::timeout(CONNECT_TIMEOUT, conn.close())
        .await
        .is_err()
    {
        tracing::warn!("koneksi database lab tidak tertutup tepat waktu");
    }
}

/// Satu kolom teks per baris kueri katalog.
async fn strings(
    conn: &mut PgConnection,
    sql: &'static str,
    bind: &str,
) -> Result<Vec<String>, PortError> {
    let fut = sqlx::query_scalar::<_, String>(sql)
        .bind(bind.to_owned())
        .fetch_all(conn);
    tokio::time::timeout(CATALOG_TIMEOUT, fut)
        .await
        .map_err(|_| timed_out("reading the catalog"))?
        .map_err(db_error)
}

const ROLE_ATTRIBUTES: &str = "SELECT rolsuper, rolcreaterole, rolcreatedb, rolreplication, rolbypassrls \
     FROM pg_roles WHERE rolname = $1";
const ROLE_MEMBERSHIP: &str = "WITH RECURSIVE m(oid) AS ( \
       SELECT roleid FROM pg_auth_members WHERE member = (SELECT oid FROM pg_roles WHERE rolname = $1) \
       UNION SELECT a.roleid FROM pg_auth_members a JOIN m ON a.member = m.oid) \
     SELECT r.rolname::text FROM m JOIN pg_roles r ON r.oid = m.oid ORDER BY 1";
const ROLE_CREATE: &str = "SELECT 'database ' || current_database() \
       WHERE has_database_privilege($1, current_database(), 'CREATE') \
     UNION ALL SELECT 'schema ' || nspname::text FROM pg_namespace \
       WHERE nspname NOT LIKE 'pg\\_%' AND nspname <> 'information_schema' \
         AND has_schema_privilege($1, oid, 'CREATE') \
     ORDER BY 1";
/// Daftar tabel schema pengguna, terurut.
const TABLES: &str = concat!(
    "SELECT n.nspname || '.' || c.relname ",
    "FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace ",
    "WHERE c.relkind IN ('r', 'p') AND n.nspname NOT IN ('pg_catalog', 'information_schema') ",
    "AND n.nspname NOT LIKE 'pg\\_toast%' ORDER BY 1"
);
/// Extension terpasang dan jalur yang bisa dipakai role `$1`.
///
/// - Kolom kedua: fungsi extension yang bisa dijalankan, selain handler dan
///   validator foreign data wrapper.
/// - Kolom ketiga: USAGE pada wrapper atau server FDW milik extension, atau
///   hak tabel maupun kolom pada foreign table servernya atau view (bertingkat)
///   di atasnya, karena view berjalan dengan hak pemiliknya.
/// - Fungsi SECURITY DEFINER bukan milik extension yang bisa dijalankan role
///   dihitung sebagai jalur untuk setiap extension (gagal tertutup): fungsi itu
///   berjalan dengan hak pemiliknya dan bisa memanggil dblink atau foreign
///   table.
const EXTENSION_USE: &str = "\
WITH RECURSIVE reach(fdw, rel) AS ( \
    SELECT s.srvfdw, ft.ftrelid FROM pg_foreign_table ft \
      JOIN pg_foreign_server s ON s.oid = ft.ftserver \
  UNION \
    SELECT r.fdw, rw.ev_class FROM reach r \
      JOIN pg_depend d ON d.refclassid = 'pg_class'::regclass AND d.refobjid = r.rel \
        AND d.classid = 'pg_rewrite'::regclass \
      JOIN pg_rewrite rw ON rw.oid = d.objid \
    WHERE rw.ev_class <> r.rel), \
definer AS ( \
  SELECT EXISTS ( \
    SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
    WHERE p.prosecdef AND n.nspname NOT IN ('pg_catalog', 'information_schema') \
      AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.classid = 'pg_proc'::regclass \
                      AND d.objid = p.oid AND d.deptype = 'e') \
      AND has_function_privilege($1, p.oid, 'EXECUTE')) AS callable) \
SELECT e.extname::text, \
  EXISTS ( \
    SELECT 1 FROM pg_depend d JOIN pg_proc p ON p.oid = d.objid \
    WHERE d.classid = 'pg_proc'::regclass AND d.refobjid = e.oid AND d.deptype = 'e' \
      AND NOT EXISTS (SELECT 1 FROM pg_foreign_data_wrapper w \
                      WHERE w.fdwhandler = p.oid OR w.fdwvalidator = p.oid) \
      AND has_function_privilege($1, p.oid, 'EXECUTE')), \
  (SELECT callable FROM definer) OR EXISTS ( \
    SELECT 1 FROM pg_depend d JOIN pg_foreign_data_wrapper w ON w.oid = d.objid \
    WHERE d.classid = 'pg_foreign_data_wrapper'::regclass AND d.refobjid = e.oid AND d.deptype = 'e' \
      AND (has_foreign_data_wrapper_privilege($1, w.oid, 'USAGE') \
        OR EXISTS (SELECT 1 FROM pg_foreign_server s \
                   WHERE s.srvfdw = w.oid AND has_server_privilege($1, s.oid, 'USAGE')) \
        OR EXISTS (SELECT 1 FROM reach r WHERE r.fdw = w.oid \
                   AND (has_table_privilege($1, r.rel, 'SELECT, INSERT, UPDATE, DELETE') \
                     OR has_any_column_privilege($1, r.rel, 'SELECT, INSERT, UPDATE, REFERENCES'))))) \
FROM pg_extension e ORDER BY 1";

const ROLE_OWNED: &str = "SELECT ( \
       (SELECT count(*) FROM pg_class WHERE relowner = r.oid) \
     + (SELECT count(*) FROM pg_proc WHERE proowner = r.oid) \
     + (SELECT count(*) FROM pg_namespace WHERE nspowner = r.oid) \
     + (SELECT count(*) FROM pg_type WHERE typowner = r.oid AND typtype <> 'c'))::bigint \
     FROM pg_roles r WHERE r.rolname = $1";

async fn privileges(conn: &mut PgConnection, role: &str) -> Result<RolePrivileges, PortError> {
    let attrs = tokio::time::timeout(
        CATALOG_TIMEOUT,
        sqlx::query(ROLE_ATTRIBUTES)
            .bind(role.to_owned())
            .fetch_optional(&mut *conn),
    )
    .await
    .map_err(|_| timed_out("reading the role"))?
    .map_err(db_error)?
    .ok_or_else(|| PortError::Rejected(format!("database role {role} does not exist")))?;
    let mut attributes = Vec::new();
    for (i, attr) in [
        RoleAttribute::Superuser,
        RoleAttribute::CreateRole,
        RoleAttribute::CreateDb,
        RoleAttribute::Replication,
        RoleAttribute::BypassRls,
    ]
    .into_iter()
    .enumerate()
    {
        if attrs.try_get::<bool, _>(i).map_err(db_error)? {
            attributes.push(attr);
        }
    }
    let trigger_sql: &'static str = concat!(
        "SELECT n.nspname || '.' || c.relname ",
        "FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace ",
        "WHERE c.relkind IN ('r', 'p') AND n.nspname NOT IN ('pg_catalog', 'information_schema') ",
        "AND n.nspname NOT LIKE 'pg\\_toast%' AND has_table_privilege($1, c.oid, 'TRIGGER') ORDER BY 1"
    );
    let access_sql: &'static str = concat!(
        "SELECT n.nspname || '.' || c.relname ",
        "FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace ",
        "WHERE c.relkind IN ('r', 'p') AND n.nspname NOT IN ('pg_catalog', 'information_schema') ",
        "AND n.nspname NOT LIKE 'pg\\_toast%' AND NOT (",
        "has_table_privilege($1, c.oid, 'SELECT') AND has_table_privilege($1, c.oid, 'INSERT') ",
        "AND has_table_privilege($1, c.oid, 'UPDATE') AND has_table_privilege($1, c.oid, 'DELETE')) ",
        "ORDER BY 1"
    );
    let owned: i64 = tokio::time::timeout(
        CATALOG_TIMEOUT,
        sqlx::query_scalar::<_, i64>(ROLE_OWNED)
            .bind(role.to_owned())
            .fetch_one(&mut *conn),
    )
    .await
    .map_err(|_| timed_out("reading owned objects"))?
    .map_err(db_error)?;
    let extensions = tokio::time::timeout(
        CATALOG_TIMEOUT,
        sqlx::query_as::<_, (String, bool, bool)>(EXTENSION_USE)
            .bind(role.to_owned())
            .fetch_all(&mut *conn),
    )
    .await
    .map_err(|_| timed_out("reading extensions"))?
    .map_err(db_error)?
    .into_iter()
    .map(|(name, functions, wrapper)| ExtensionAccess {
        name,
        functions,
        wrapper,
    })
    .collect();
    Ok(RolePrivileges {
        attributes,
        member_of: strings(conn, ROLE_MEMBERSHIP, role).await?,
        create_on: strings(conn, ROLE_CREATE, role).await?,
        trigger_on: strings(conn, trigger_sql, role).await?,
        owned_objects: u64::try_from(owned).unwrap_or(0),
        extensions,
        missing_access: strings(conn, access_sql, role).await?,
    })
}

/// Nilai satu sel sebagai teks; tipe yang tidak dikenal ditandai.
fn cell(row: &PgRow, i: usize) -> String {
    macro_rules! try_as {
        ($t:ty) => {
            if let Ok(v) = row.try_get::<Option<$t>, _>(i) {
                return v.map_or_else(|| "NULL".to_owned(), |x| x.to_string());
            }
        };
    }
    try_as!(String);
    try_as!(i64);
    try_as!(i32);
    try_as!(i16);
    try_as!(f64);
    try_as!(f32);
    try_as!(bool);
    "<value>".to_owned()
}

/// Batas jumlah baris yang dibaca dari satu pernyataan.
const MAX_ROWS: usize = 1_000;
/// Batas panjang satu sel.
const MAX_CELL: usize = 2_000;

/// Hasil bertab yang dibangun bertahap; berhenti membaca setelah batas.
struct Rendered {
    text: String,
    rows: usize,
    truncated: bool,
}

impl Rendered {
    fn new() -> Self {
        Self {
            text: String::new(),
            rows: 0,
            truncated: false,
        }
    }

    /// Menambah satu baris; `false` bila batas tercapai dan pembacaan harus
    /// berhenti.
    fn push(&mut self, row: &PgRow) -> bool {
        if self.rows == 0 {
            let header: Vec<&str> = row.columns().iter().map(Column::name).collect();
            self.text = header.join("\t");
        }
        let line: Vec<String> = (0..row.len())
            .map(|i| {
                let mut c = cell(row, i);
                if c.len() > MAX_CELL {
                    let mut end = MAX_CELL;
                    while !c.is_char_boundary(end) {
                        end -= 1;
                    }
                    c.truncate(end);
                    c.push_str(" [truncated]");
                }
                c
            })
            .collect();
        let line = line.join("\t");
        if self.rows >= MAX_ROWS || self.text.len() + line.len() + 1 > MAX_OUTPUT {
            self.truncated = true;
            return false;
        }
        self.text.push('\n');
        self.text.push_str(&line);
        self.rows += 1;
        true
    }

    fn finish(self) -> SqlResult {
        if self.rows == 0 && !self.truncated {
            return SqlResult {
                text: "Statement ran; no rows returned.".to_owned(),
                rows: 0,
                truncated: false,
            };
        }
        SqlResult {
            text: self.text,
            rows: self.rows,
            truncated: self.truncated,
        }
    }
}

/// Membaca baris satu per satu dan berhenti di batas, sehingga hasil besar
/// tidak pernah dimuat seluruhnya ke memori.
async fn stream_rows(conn: &mut PgConnection, sql: &str) -> Result<SqlResult, PortError> {
    use futures::TryStreamExt;
    let mut out = Rendered::new();
    let mut rows = sqlx::query(AssertSqlSafe(sql.to_owned())).fetch(conn);
    while let Some(row) = rows.try_next().await.map_err(db_error)? {
        if !out.push(&row) {
            break;
        }
    }
    Ok(out.finish())
}

impl LabDatabase for PgLabDatabase {
    async fn server_is_local(&self, creds: &DbCredentials) -> Result<bool, PortError> {
        let mut conn = connect(creds, CATALOG_TIMEOUT).await?;
        let addr = tokio::time::timeout(
            CATALOG_TIMEOUT,
            sqlx::query_scalar::<_, Option<String>>("SELECT host(inet_server_addr())")
                .fetch_one(&mut conn),
        )
        .await
        .map_err(|_| timed_out("reading the server address"));
        close(conn).await;
        let addr = addr?.map_err(db_error)?;
        Ok(server_addr_local(addr.as_deref()))
    }

    async fn reads_all_stats(&self, maintenance: &DbCredentials) -> Result<bool, PortError> {
        let mut conn = connect(maintenance, CATALOG_TIMEOUT).await?;
        let ok = tokio::time::timeout(
            CATALOG_TIMEOUT,
            sqlx::query_scalar::<_, bool>(
                "SELECT rolsuper OR pg_has_role(current_user, 'pg_read_all_stats', 'MEMBER') \
                 FROM pg_roles WHERE rolname = current_user",
            )
            .fetch_one(&mut conn),
        )
        .await
        .map_err(|_| timed_out("reading the maintenance role"));
        close(conn).await;
        ok?.map_err(db_error)
    }

    async fn role_privileges(
        &self,
        maintenance: &DbCredentials,
        lab_role: &str,
    ) -> Result<RolePrivileges, PortError> {
        let mut conn = connect(maintenance, CATALOG_TIMEOUT).await?;
        let p = privileges(&mut conn, lab_role).await;
        close(conn).await;
        p
    }

    async fn activity(
        &self,
        maintenance: &DbCredentials,
    ) -> Result<(Vec<PgActivity>, i32), PortError> {
        let mut conn = connect(maintenance, CATALOG_TIMEOUT).await?;
        let read = async {
            let own: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut conn)
                .await
                .map_err(db_error)?;
            let rows = sqlx::query(
                "SELECT pid, datname::text, backend_type, usename::text, application_name, \
                 host(client_addr) FROM pg_stat_activity",
            )
            .fetch_all(&mut conn)
            .await
            .map_err(db_error)?;
            let list = rows
                .iter()
                .map(|r| {
                    Ok(PgActivity {
                        pid: r.try_get(0).map_err(db_error)?,
                        datname: r.try_get(1).map_err(db_error)?,
                        backend_type: r.try_get(2).map_err(db_error)?,
                        usename: r.try_get(3).map_err(db_error)?,
                        application_name: r.try_get(4).map_err(db_error)?,
                        client_addr: r.try_get(5).map_err(db_error)?,
                    })
                })
                .collect::<Result<Vec<_>, PortError>>()?;
            Ok::<_, PortError>((list, own))
        };
        let out = tokio::time::timeout(CATALOG_TIMEOUT, read)
            .await
            .map_err(|_| timed_out("reading pg_stat_activity"));
        close(conn).await;
        out?
    }

    async fn tables(&self, creds: &DbCredentials) -> Result<Vec<String>, PortError> {
        let mut conn = connect(creds, CATALOG_TIMEOUT).await?;
        let out = tokio::time::timeout(
            CATALOG_TIMEOUT,
            sqlx::query_scalar::<_, String>(TABLES).fetch_all(&mut conn),
        )
        .await
        .map_err(|_| timed_out("listing tables"));
        close(conn).await;
        out?.map_err(db_error)
    }

    async fn run_statement(&self, lab: &DbCredentials, sql: &str) -> Result<SqlResult, PortError> {
        check_statement(sql).map_err(|e| PortError::Rejected(e.to_string()))?;
        let limit = self.statement_timeout();
        let mut conn = connect(lab, limit).await?;
        let out = tokio::time::timeout(limit + Duration::from_secs(5), stream_rows(&mut conn, sql))
            .await
            .map_err(|_| timed_out("the statement"));
        close(conn).await;
        out?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mandor_domain::lab_data::DbTarget;

    fn creds(host: &str) -> DbCredentials {
        DbCredentials {
            target: DbTarget {
                host: host.to_owned(),
                port: 5432,
                database: "kelola".to_owned(),
            },
            user: "lab".to_owned(),
            password: secrecy::SecretString::from("rahasia".to_owned()),
        }
    }

    /// `lab_pg` menolak host bukan loopback sebelum membuka koneksi.
    #[tokio::test]
    async fn host_bukan_lokal_ditolak_sebelum_koneksi() {
        let db = PgLabDatabase::new();
        for host in ["db.staging.example.com", "10.0.0.5"] {
            let err = db.server_is_local(&creds(host)).await.unwrap_err();
            assert!(
                matches!(err, PortError::Rejected(m) if m.contains("not local")),
                "{host}"
            );
        }
        let err = db
            .run_statement(&creds("db.staging.example.com"), "SELECT 1")
            .await
            .unwrap_err();
        assert!(matches!(err, PortError::Rejected(_)));
    }

    #[tokio::test]
    async fn pernyataan_terlarang_ditolak_tanpa_koneksi() {
        let db = PgLabDatabase::new();
        let err = db
            .run_statement(&creds("localhost"), "COPY t TO PROGRAM 'id'")
            .await
            .unwrap_err();
        assert!(matches!(err, PortError::Rejected(m) if m.contains("copy")));
    }

    #[test]
    fn opsi_socket_dan_tcp() {
        assert!(options(&creds("/var/run/postgresql"), STATEMENT_TIMEOUT).is_ok());
        assert!(options(&creds("[::1]"), STATEMENT_TIMEOUT).is_ok());
        assert!(options(&creds("localhost.evil.com"), STATEMENT_TIMEOUT).is_err());
    }

    #[test]
    fn hasil_kosong_dijelaskan() {
        let r = Rendered::new().finish();
        assert_eq!(r.rows, 0);
        assert!(r.text.contains("no rows"));
    }
}
