//! Adapter `PostgreSQL` untuk database lab (plan 2026-11-10-001, U12,
//! KTD-L15, KTD-L16, KTD-L20).
//!
//! Hanya kueri runtime (`sqlx::query`) tanpa makro, sehingga cache `.sqlx`
//! tetap milik `SQLite`. Setiap operasi membuka satu koneksi tanpa TLS dengan
//! opsi eksplisit dari kredensial (bukan DSN bebas), dibatasi waktu koneksi dan
//! pernyataan, lalu menutupnya.

use std::time::Duration;

use mandor_domain::lab_data::{
    PgActivity, RoleAttribute, RolePrivileges, check_statement, is_local_db, server_addr_local,
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
        sqlx::query_scalar::<_, String>("SELECT extname::text FROM pg_extension ORDER BY 1")
            .fetch_all(&mut *conn),
    )
    .await
    .map_err(|_| timed_out("reading extensions"))?
    .map_err(db_error)?;
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

/// Baris hasil sebagai teks bertab, dipotong [`MAX_OUTPUT`].
fn render(rows: &[PgRow]) -> SqlResult {
    let Some(first) = rows.first() else {
        return SqlResult {
            text: "Statement ran; no rows returned.".to_owned(),
            rows: 0,
            truncated: false,
        };
    };
    let header: Vec<&str> = first.columns().iter().map(Column::name).collect();
    let mut text = header.join("\t");
    let mut truncated = false;
    for row in rows {
        let line: Vec<String> = (0..row.len()).map(|i| cell(row, i)).collect();
        let line = line.join("\t");
        if text.len() + line.len() + 1 > MAX_OUTPUT {
            truncated = true;
            break;
        }
        text.push('\n');
        text.push_str(&line);
    }
    SqlResult {
        text,
        rows: rows.len(),
        truncated,
    }
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
        let rows = tokio::time::timeout(
            limit + Duration::from_secs(5),
            sqlx::query(AssertSqlSafe(sql.to_owned())).fetch_all(&mut conn),
        )
        .await
        .map_err(|_| timed_out("the statement"));
        close(conn).await;
        Ok(render(&rows?.map_err(db_error)?))
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
        let r = render(&[]);
        assert_eq!(r.rows, 0);
        assert!(r.text.contains("no rows"));
    }
}
