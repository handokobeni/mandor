#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // integration tests
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_mandord");

fn daemon_info(home: &Path) -> Option<Value> {
    let body = std::fs::read(home.join("daemon.json")).ok()?;
    serde_json::from_slice(&body).ok()
}

fn wait_for_info(home: &Path) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Some(info) = daemon_info(home) {
            return info;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("daemon.json tidak muncul");
}

fn stop(home: &Path) {
    if let Some(info) = daemon_info(home) {
        let pid = info["pid"].as_u64().unwrap().to_string();
        let _ = Command::new("kill").args(["-TERM", &pid]).status();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && home.join("daemon.json").exists() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn serve(home: &Path) -> Child {
    Command::new(BIN)
        .arg("serve")
        .env("MANDOR_HOME", home)
        .env_remove("INVOCATION_ID")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

/// Mengirim baris lewat `mandord connect` dan membaca jawaban.
fn connect_roundtrip(home: &Path, lines: &[Value]) -> Vec<Value> {
    let mut child = Command::new(BIN)
        .arg("connect")
        .env("MANDOR_HOME", home)
        .env_remove("INVOCATION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut replies = Vec::new();
    for line in lines {
        writeln!(stdin, "{line}").unwrap();
        let mut buf = String::new();
        out.read_line(&mut buf).unwrap();
        replies.push(serde_json::from_str(&buf).unwrap());
    }
    drop(stdin);
    let _ = child.wait();
    replies
}

fn hello(token: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "hello",
           "params": {"token": token, "protocol": "1.0", "client": "test"}})
}

#[test]
fn instance_kedua_gagal_karena_lock_dan_berkas_rahasia_0600() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let mut first = serve(home.path());
    let info = wait_for_info(home.path());

    let second = Command::new(BIN)
        .arg("serve")
        .env("MANDOR_HOME", home.path())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(!second.status.success(), "instance kedua tidak boleh jalan");
    assert_eq!(
        daemon_info(home.path()).unwrap()["pid"],
        info["pid"],
        "daemon.json instance pertama rusak"
    );

    let file_mode = std::fs::metadata(home.path().join("daemon.json"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    let dir_mode = std::fs::metadata(home.path()).unwrap().permissions().mode() & 0o777;
    assert_eq!(file_mode, 0o600);
    assert_eq!(dir_mode, 0o700);

    stop(home.path());
    let _ = first.wait();
}

#[test]
fn connect_menjalankan_daemon_terlepas_dan_tetap_hidup_setelah_relay_ditutup() {
    let home = tempfile::tempdir().unwrap();
    // Tanpa daemon: connect harus menjalankannya. Token dibaca dari daemon.json
    // setelah daemon siap, jadi kirim hello salah dulu untuk memicu start.
    let replies = connect_roundtrip(home.path(), &[hello("salah")]);
    assert_eq!(replies[0]["error"]["code"], "unauthorized");

    let info = wait_for_info(home.path());
    let token = info["token"].as_str().unwrap().to_owned();
    let replies = connect_roundtrip(
        home.path(),
        &[
            hello(&token),
            json!({"jsonrpc": "2.0", "id": 2, "method": "status", "params": null}),
        ],
    );
    assert_eq!(replies[0]["result"]["protocol"], "1.0");
    assert_eq!(replies[1]["result"]["repos"], 0);
    assert_eq!(replies[1]["result"]["busy"], false);
    assert_eq!(replies[1]["result"]["managed_by"], "detached");

    // Relay sudah ditutup; daemon tetap hidup dengan pid yang sama.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(daemon_info(home.path()).unwrap()["pid"], info["pid"]);
    stop(home.path());
}

#[test]
fn token_mencetak_token_yang_sama_dengan_daemon_json() {
    let home = tempfile::tempdir().unwrap();
    let out = Command::new(BIN)
        .arg("token")
        .env("MANDOR_HOME", home.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let printed: Value = serde_json::from_slice(&out.stdout).unwrap();
    let info = wait_for_info(home.path());
    assert_eq!(printed["token"], info["token"]);
    stop(home.path());
}

fn rpc(id: u64, method: &str, params: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "uji")
        .env("GIT_AUTHOR_EMAIL", "uji@example.com")
        .env("GIT_COMMITTER_NAME", "uji")
        .env("GIT_COMMITTER_EMAIL", "uji@example.com")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// Repo uji dengan origin bare.
fn test_repo(root: &Path) -> std::path::PathBuf {
    let origin = root.join("origin.git");
    let repo = root.join("app web");
    git(
        root,
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            origin.to_str().unwrap(),
        ],
    );
    git(
        root,
        &[
            "clone",
            "-q",
            origin.to_str().unwrap(),
            repo.to_str().unwrap(),
        ],
    );
    std::fs::write(repo.join("README.md"), "x\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "awal"]);
    git(&repo, &["push", "-q", "origin", "main"]);
    git(&repo, &["remote", "set-head", "origin", "main"]);
    repo
}

#[test]
fn rpc_pengaturan_login_dan_repo_tanpa_sesi_lark() {
    let home = tempfile::tempdir().unwrap();
    let mut daemon = serve(home.path());
    let token = wait_for_info(home.path())["token"]
        .as_str()
        .unwrap()
        .to_owned();

    let root = tempfile::tempdir().unwrap();
    let repo = test_repo(root.path());

    let settings = json!({
        "app_id": "cli_uji", "app_secret": "rahasia-app", "app_token": "appX", "table_id": "tblX",
        "ac_table_id": "tblAC", "redirect_port": 0, "open_base": null, "accounts_base": null, "quiet_secs": null
    });
    let replies = connect_roundtrip(
        home.path(),
        &[
            hello(&token),
            rpc(2, "status", &Value::Null),
            rpc(3, "board.list", &Value::Null),
            rpc(4, "lark.settings.set", &settings),
            rpc(5, "lark.settings.get", &Value::Null),
            rpc(6, "lark.login.begin", &Value::Null),
            rpc(
                7,
                "lark.login.complete",
                &json!({"code": "c", "state": "palsu"}),
            ),
            rpc(
                8,
                "repos.add",
                &json!({"path": repo.to_str().unwrap(), "gh_profile": "dev"}),
            ),
            rpc(
                9,
                "repos.update",
                &json!({"id": "app-web", "gh_profile": null, "companions": ["/specs"],
                                          "trusted_reviewers": ["ou_qa"], "extra_allowed": ["Bash(just *)"]}),
            ),
            rpc(10, "repos.list", &Value::Null),
            rpc(11, "notifications.take", &Value::Null),
            rpc(12, "metode.tidak.ada", &Value::Null),
            rpc(13, "notifications.list", &Value::Null),
            rpc(14, "notifications.read", &json!({"ids": null})),
            rpc(
                15,
                "notifications.read",
                &json!({"ids": (0..501).collect::<Vec<_>>()}),
            ),
            rpc(16, "people.names", &json!({"open_ids": ["ou_a"]})),
        ],
    );
    assert_eq!(replies[1]["result"]["lark"]["configured"], false);
    assert_eq!(replies[2]["error"]["code"], "unauthorized");
    assert_eq!(replies[3]["result"]["configured"], true);
    let view = &replies[4]["result"];
    assert_eq!(view["has_secret"], true);
    assert_eq!(view["redirect_port"], 53_682);
    assert!(!replies[4].to_string().contains("rahasia-app"));
    let begin = &replies[5]["result"];
    let url = begin["url"].as_str().unwrap();
    assert!(url.starts_with("https://accounts.larksuite.com/open-apis/authen/v1/authorize?"));
    assert!(url.contains("client_id=cli_uji") && url.contains("code_challenge_method=S256"));
    assert_eq!(begin["port"], 53_682);
    assert_eq!(replies[6]["error"]["code"], "rejected");
    assert!(
        replies[6]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("state")
    );
    assert_eq!(replies[7]["result"]["id"], "app-web");
    assert_eq!(replies[7]["result"]["default_branch"], "main");
    assert_eq!(replies[8]["result"]["trusted_reviewers"], json!(["ou_qa"]));
    assert_eq!(replies[8]["result"]["gh_profile"], Value::Null);
    assert_eq!(
        replies[9]["result"][0]["extra_allowed"],
        json!(["Bash(just *)"])
    );
    assert_eq!(replies[10]["result"], json!([]));
    assert_eq!(replies[11]["error"]["code"], "method_not_found");
    assert_eq!(replies[12]["result"], json!([]));
    assert_eq!(replies[13]["result"], json!({}));
    assert_eq!(replies[14]["error"]["code"], "rejected");
    // Nama orang butuh sesi Lark.
    assert_eq!(replies[15]["error"]["code"], "unauthorized");

    let secret_file = std::fs::read_to_string(home.path().join("lark.json")).unwrap();
    assert!(secret_file.contains("rahasia-app"));
    stop(home.path());
    let _ = daemon.wait();
}

#[test]
fn jeda_tenang_diatur_terpisah_dan_tidak_direset_saat_simpan_pengaturan_lark() {
    let home = tempfile::tempdir().unwrap();
    let mut daemon = serve(home.path());
    let token = wait_for_info(home.path())["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let settings = json!({
        "app_id": "cli_uji", "app_secret": "rahasia-app", "app_token": "appX", "table_id": "tblX",
        "ac_table_id": "tblAC", "redirect_port": 0, "open_base": null, "accounts_base": null, "quiet_secs": null
    });
    let replies = connect_roundtrip(
        home.path(),
        &[
            hello(&token),
            rpc(2, "waiting_fix.quiet.set", &json!({"secs": 300})),
            rpc(3, "lark.settings.set", &settings),
            rpc(4, "waiting_fix.quiet.set", &json!({"secs": 300})),
            rpc(5, "lark.settings.get", &Value::Null),
            rpc(6, "lark.settings.set", &settings),
            rpc(7, "lark.settings.get", &Value::Null),
            rpc(8, "waiting_fix.quiet.set", &json!({"secs": 10})),
            rpc(9, "waiting_fix.quiet.set", &json!({"secs": 901})),
        ],
    );
    // Sebelum Lark diatur tidak ada tempat menyimpan.
    assert_eq!(replies[1]["error"]["code"], "rejected");
    assert_eq!(replies[3]["result"]["quiet_secs"], 300);
    assert_eq!(replies[4]["result"]["quiet_secs"], 300);
    // Menyimpan pengaturan Lark tanpa nilai jeda mempertahankan jeda lama.
    assert_eq!(replies[6]["result"]["quiet_secs"], 300);
    assert_eq!(replies[7]["error"]["code"], "rejected");
    assert_eq!(replies[8]["error"]["code"], "rejected");
    stop(home.path());
    let _ = daemon.wait();
}

#[test]
fn saran_repo_dan_penjelajah_folder_di_home_pengguna() {
    let user_home = tempfile::tempdir().unwrap();
    let data = user_home.path().join(".mandor");
    std::fs::create_dir_all(&data).unwrap();
    let repo_dir = user_home.path().join("Work/pegasus-web");
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
    std::fs::write(
        repo_dir.join(".git/config"),
        "[remote \"origin\"]\n\turl = git@github.com:o/pegasus-web.git\n",
    )
    .unwrap();

    let mut daemon = Command::new(BIN)
        .arg("serve")
        .env("MANDOR_HOME", &data)
        .env("HOME", user_home.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let token = wait_for_info(&data)["token"].as_str().unwrap().to_owned();
    let replies = connect_roundtrip(
        &data,
        &[
            hello(&token),
            rpc(2, "repos.discover", &Value::Null),
            rpc(3, "fs.list_dirs", &json!({"path": null})),
            rpc(4, "fs.list_dirs", &json!({"path": "relatif"})),
            rpc(5, "agent.skills", &Value::Null),
        ],
    );
    // HOME sementara tanpa ~/.claude: tidak ada skill.
    assert_eq!(replies[4]["result"], json!([]));
    let found = &replies[1]["result"];
    assert_eq!(found[0]["name"], "pegasus-web");
    assert_eq!(found[0]["path"], repo_dir.display().to_string());
    assert_eq!(found[0]["added"], false);
    let listing = &replies[2]["result"];
    assert_eq!(listing["path"], user_home.path().display().to_string());
    assert_eq!(listing["entries"][0]["name"], "Work");
    assert_eq!(replies[3]["error"]["code"], "rejected");

    stop(&data);
    let _ = daemon.wait();
}

/// Repo uji `<parent>/<name>` dengan origin bare di `<parent>/.origin/<name>.git`.
fn repo_in(parent: &Path, name: &str) -> std::path::PathBuf {
    let origin = parent.join(".origin").join(format!("{name}.git"));
    let repo = parent.join(name);
    std::fs::create_dir_all(origin.parent().unwrap()).unwrap();
    git(
        parent,
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            origin.to_str().unwrap(),
        ],
    );
    git(
        parent,
        &[
            "clone",
            "-q",
            origin.to_str().unwrap(),
            repo.to_str().unwrap(),
        ],
    );
    std::fs::write(repo.join("README.md"), "x\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "awal"]);
    git(&repo, &["push", "-q", "origin", "main"]);
    git(&repo, &["remote", "set-head", "origin", "main"]);
    repo
}

#[test]
fn rpc_workspace_dari_folder_induk_dengan_mcp_terpilih() {
    let home = tempfile::tempdir().unwrap();
    let mut daemon = serve(home.path());
    let token = wait_for_info(home.path())["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("kelola-v2");
    std::fs::create_dir_all(&parent).unwrap();
    let api = repo_in(&parent, "kelola-api");
    let web = repo_in(&parent, "kelola-web");
    std::fs::write(
        parent.join(".mcp.json"),
        r#"{"mcpServers":{"graft":{"command":"node"},"figma":{"command":"f"}}}"#,
    )
    .unwrap();
    let parent_s = parent.to_str().unwrap();

    let replies = connect_roundtrip(
        home.path(),
        &[
            hello(&token),
            rpc(
                2,
                "repos.add",
                &json!({"path": api.to_str().unwrap(), "gh_profile": null}),
            ),
            rpc(
                3,
                "repos.add",
                &json!({"path": web.to_str().unwrap(), "gh_profile": null}),
            ),
            rpc(4, "workspaces.mcp_servers", &json!({"root": parent_s})),
            rpc(
                5,
                "workspaces.add",
                &json!({"name": "Kelola V2", "root": parent_s,
                        "repo_ids": ["kelola-api", "kelola-web"], "mcp_servers": ["graft"]}),
            ),
            rpc(
                6,
                "workspaces.add",
                &json!({"name": "x", "root": parent_s, "repo_ids": ["kelola-api"],
                        "mcp_servers": ["github"]}),
            ),
            rpc(
                7,
                "workspaces.add",
                &json!({"name": "x", "root": parent_s, "repo_ids": ["tidak-ada"], "mcp_servers": []}),
            ),
            rpc(
                8,
                "workspaces.update",
                &json!({"id": "kelola-v2", "repo_ids": ["kelola-web"], "mcp_servers": []}),
            ),
            rpc(9, "workspaces.list", &Value::Null),
            rpc(10, "workspaces.remove", &json!({"id": "kelola-v2"})),
            rpc(11, "workspaces.list", &Value::Null),
            rpc(12, "workspaces.mcp_servers", &json!({"root": "relatif"})),
        ],
    );
    assert_eq!(replies[3]["result"], json!(["figma", "graft"]));
    let ws = &replies[4]["result"];
    assert_eq!(ws["id"], "kelola-v2");
    assert_eq!(ws["name"], "Kelola V2");
    assert_eq!(ws["repo_ids"], json!(["kelola-api", "kelola-web"]));
    assert_eq!(ws["mcp_servers"], json!(["graft"]));
    assert_eq!(
        replies[5]["error"]["code"], "rejected",
        "MCP server tidak ada di .mcp.json"
    );
    assert_eq!(replies[6]["error"]["code"], "not_found");
    assert_eq!(replies[7]["result"]["repo_ids"], json!(["kelola-web"]));
    assert_eq!(replies[8]["result"][0]["mcp_servers"], json!([]));
    assert_eq!(replies[10]["result"], json!([]));
    assert_eq!(replies[11]["error"]["code"], "rejected");
    stop(home.path());
    let _ = daemon.wait();
}

#[test]
fn rpc_sumber_desain_diuji_didaftarkan_dan_ditolak_bila_tidak_layak() {
    let home = tempfile::tempdir().unwrap();
    let mut daemon = serve(home.path());
    let token = wait_for_info(home.path())["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let root = tempfile::tempdir().unwrap();
    let mock = root.path().join("kelola-mock");
    std::fs::create_dir_all(&mock).unwrap();
    std::fs::write(root.path().join("berkas.txt"), "x").unwrap();
    let mock_s = mock.to_str().unwrap();
    let missing = root.path().join("tidak-ada");
    let file = root.path().join("berkas.txt");
    let state = home.path().join(".mandor");

    let replies = connect_roundtrip(
        home.path(),
        &[
            hello(&token),
            rpc(2, "design_sources.test", &json!({"path": mock_s})),
            rpc(
                3,
                "design_sources.test",
                &json!({"path": missing.to_str().unwrap()}),
            ),
            rpc(
                4,
                "design_sources.test",
                &json!({"path": file.to_str().unwrap()}),
            ),
            rpc(
                5,
                "design_sources.test",
                &json!({"path": state.to_str().unwrap()}),
            ),
            rpc(
                6,
                "design_sources.add",
                &json!({"name": "kelola-mock", "path": mock_s}),
            ),
            rpc(
                7,
                "design_sources.add",
                &json!({"name": "hilang", "path": missing.to_str().unwrap()}),
            ),
            rpc(
                8,
                "design_sources.add",
                &json!({"name": "kelola-mock", "path": mock_s}),
            ),
            rpc(9, "design_sources.list", &Value::Null),
        ],
    );
    assert_eq!(
        replies[1]["result"],
        json!({"ready": true, "status": "Ready"})
    );
    assert_eq!(replies[2]["result"]["status"], "Path missing");
    assert_eq!(replies[3]["result"]["status"], "Not a folder");
    assert_eq!(replies[4]["result"]["ready"], false);
    let added = &replies[5]["result"];
    assert_eq!(added["name"], "kelola-mock");
    assert_eq!(added["kind"], "folder");
    assert_eq!(added["ready"], true);
    assert_eq!(replies[6]["error"]["code"], "rejected");
    assert_eq!(replies[7]["error"]["code"], "conflict");
    let id = added["id"].as_str().unwrap().to_owned();
    assert_eq!(replies[8]["result"][0]["id"], id.as_str());

    // Sumber yang hilang tetap terdaftar dan ditandai; lalu bisa dihapus.
    std::fs::remove_dir_all(&mock).unwrap();
    let replies = connect_roundtrip(
        home.path(),
        &[
            hello(&token),
            rpc(2, "design_sources.list", &Value::Null),
            rpc(
                3,
                "design_sources.update",
                &json!({"id": id, "name": "mock", "path": root.path().to_str().unwrap()}),
            ),
            rpc(4, "design_sources.remove", &json!({"id": id})),
            rpc(5, "design_sources.list", &Value::Null),
        ],
    );
    assert_eq!(replies[1]["result"][0]["status"], "Path missing");
    assert_eq!(replies[1]["result"][0]["ready"], false);
    assert_eq!(replies[2]["result"]["name"], "mock");
    assert_eq!(replies[4]["result"], json!([]));
    stop(home.path());
    let _ = daemon.wait();
}

#[test]
fn status_melaporkan_dikelola_systemd_bila_invocation_id_ada() {
    let home = tempfile::tempdir().unwrap();
    let mut daemon = Command::new(BIN)
        .arg("serve")
        .env("MANDOR_HOME", home.path())
        .env("INVOCATION_ID", "0123456789abcdef")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let token = wait_for_info(home.path())["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let replies = connect_roundtrip(
        home.path(),
        &[hello(&token), rpc(2, "status", &Value::Null)],
    );
    assert_eq!(replies[1]["result"]["managed_by"], "systemd");
    assert_eq!(replies[1]["result"]["busy"], false);
    stop(home.path());
    let _ = daemon.wait();
}

#[test]
fn version_mencetak_satu_baris_polos() {
    let out = Command::new(BIN).arg("--version").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text, format!("mandord {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn drain_dan_undrain_tanpa_sesi_lark() {
    let home = tempfile::tempdir().unwrap();
    let mut daemon = serve(home.path());
    let token = wait_for_info(home.path())["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let replies = connect_roundtrip(
        home.path(),
        &[
            hello(&token),
            rpc(2, "daemon.drain", &Value::Null),
            rpc(3, "daemon.drain", &Value::Null),
            rpc(4, "daemon.undrain", &Value::Null),
        ],
    );
    assert_eq!(replies[1]["result"]["state"], "idle");
    assert_eq!(replies[2]["result"]["state"], "idle");
    assert_eq!(replies[3]["result"], json!({}));
    stop(home.path());
    let _ = daemon.wait();
}
