# Korlap — Rust Core Backend: Reconstruction Spec

Scope: `src-tauri/src/{lib,main,state,traffic}.rs`, `commands/{mod,repo,workspace,git,github,files,terminal,scripts,persistence,staging,system,helpers}.rs`, `git_provider/{mod,github}.rs`, `Cargo.toml`, `tauri.conf.json`, `capabilities/default.json`.
Out of scope (covered elsewhere): agent/agent_backend/context/lsp/jira/oauth/mcp_api internals. They are referenced only where core code touches them.

All paths below are relative to `src-tauri/`. Line numbers are per-file.

---

## 0. Build / config facts

### Cargo.toml
- Package `korlap` 0.1.0, edition 2021. Lib name `korlap_lib`, crate-type `["staticlib","cdylib","rlib"]` (Cargo.toml:10-15).
- Deps: `tauri = 2 (features ["protocol-asset"])`, `tauri-plugin-opener 2`, `tauri-plugin-dialog 2`, `serde`(derive), `serde_json`, `uuid`(v4), `tracing`, `tracing-subscriber 0.3`, `portable-pty 0.8`, `base64 0.22`, `ignore 0.4`, `fuzzy-matcher 0.3`, `grep-regex/grep-searcher/grep-matcher 0.1`, `regex 1`, `sysinfo 0.33`.
- macOS-only deps: `cocoa 0.26`, `objc 0.2`, `rand 0.8` (traffic-light positioner).
- `[lints.rust] unexpected_cfgs` declares `cfg(feature="cargo-clippy")` expected (objc 0.2 macro noise).
- No async runtime dependency besides Tauri's (`tauri::async_runtime`). No HTTP client crate — curl/gh/git are shelled out.

### tauri.conf.json
- `productName` "korlap", `identifier` **`net.ghora.korlap`** → app data dir = `~/Library/Application Support/net.ghora.korlap/` (macOS) / `~/.local/share/net.ghora.korlap/` (Linux).
- Build: `beforeDevCommand: bun run dev`, `devUrl: http://localhost:1420`, `beforeBuildCommand: bun run build`, `frontendDist: ../build` (SvelteKit static adapter).
- Single window (label defaults to `"main"`): title "korlap", 1100×700, min 760×400, `titleBarStyle: "Overlay"`, `hiddenTitle: true`.
- `security.csp: null` (CSP disabled). `assetProtocol.enable: true`, scope `["$APPDATA/images/**"]` — this is how pasted images saved by `save_image` are displayed via `convertFileSrc`.
- Bundle targets "all", icons 32/128/128@2x/icns/ico.

### capabilities/default.json
Window `main`: `core:default`, `core:event:default`, `core:window:allow-start-dragging`, `core:window:allow-toggle-maximize`, `opener:default`, `dialog:default`. (Custom commands are allowed implicitly by `generate_handler!` in Tauri v2 for the app's own commands.)

### main.rs
`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`, `fn main(){ korlap_lib::run() }`.

---

## 1. App bootstrap (lib.rs)

Sequence in `run()` (lib.rs:24-200):
1. `tracing_subscriber::fmt::init()` (lib.rs:25) — default fmt subscriber, RUST_LOG-controlled.
2. `tauri::Builder::default()` + plugins `tauri_plugin_opener::init()` and `tauri_plugin_dialog::init()` (lib.rs:28-29).
3. `.setup(|app| …)`:
   - `data_dir = app.path().app_data_dir()`; `create_dir_all(data_dir)` (lib.rs:32-34).
   - Construct `AppState` with all maps empty, `mcp_api_port: 0` (lib.rs:36-48).
   - `app_state.load()` — failure only `warn!`s (lib.rs:50-52). See §2 for exactly what load does (incl. Running→Waiting reset, archived-migration, context Building→Failed reset, run_script migration, task_title backfill).
   - Wrap: `Arc<Mutex<AppState>>` (lib.rs:54).
   - `lsp_manager = Arc<Mutex<lsp::server::LspServerPool::new()>>` (lib.rs:55).
   - `mcp_api::start_api(app.handle(), state.clone(), lsp_manager.clone())` → binds `127.0.0.1:0` (random port), spawns accept-loop thread, thread-per-connection hand-rolled HTTP/1.1 (mcp_api.rs:15-42). Returned port stored into `state.mcp_api_port` (lib.rs:56-61). Agents later get this port to call back (rename-branch, workspace-info, notify, LSP, todos routes — mcp_api.rs:84-100).
   - Managed state registered (lib.rs:64-66), exactly three types:
     - `Arc<Mutex<AppState>>`
     - `Arc<Mutex<LspServerPool>>`
     - `Arc<GitProviderRegistry>` (type alias `SharedProviderRegistry`, git_provider/mod.rs:252)
   - macOS only (lib.rs:68-77): get window `"main"`, compute y = 22.0 if `sw_vers -productVersion` major ≥ 26 (macOS Tahoe) else 18.0; `traffic::setup_traffic_light_positioner(window, x=8.0, y)`.
4. `invoke_handler(generate_handler![…])` registers 90+ commands (lib.rs:81-197).
5. `.run(generate_context!()).expect(...)`.

Notably NOT done at startup: no worktree validation, no orphan worktree cleanup, no `git worktree prune`, no shell-env pre-warm (shell env is resolved lazily on first `inject_shell_env` call — the first git command after launch pays ~4 login-shell spawns), no window-close handler (agents/PTYs are not explicitly killed on quit).

### macOS traffic lights (traffic.rs)
- `position_traffic_lights(ns_window, x, y)` (traffic.rs:19-45): takes close button's superview (button group) and its superview (title-bar container); sets container height = button_height + y and pins it at the top; positions group at `origin.x = x`, vertically centered in that height.
- `setup_traffic_light_positioner` (traffic.rs:63-342): positions once, then **replaces the NSWindow delegate** with a dynamically-created ObjC class (`cocoa::delegate!`) named `windowDelegate_<label>_<20 random alnum>`; ivars `window`, `app_box` (boxed `WindowState{window}`), `super_delegate` (Tauri/tao's original delegate), `pad_x`, `pad_y`. Every forwarded selector calls through to the super delegate; `windowDidResize:` and `windowDidExitFullScreen:` re-run positioning first. Forwarded selectors: windowShouldClose, windowWillClose, windowDidResize, windowDidMove, windowDidChangeBackingProperties, windowDidBecomeKey/ResignKey, dragging{Entered,Exited}, prepare/perform/concludeDragOperation, window:willUseFullScreenPresentationOptions:, windowDid/WillEnter/ExitFullScreen, windowDidFailToEnterFullScreen, effectiveAppearanceDidChange(+OnMainThread).
- Rebuild note: In a rebuild, prefer Tauri 2's `trafficLightPosition` window config (supported since tauri 2.x `TitleBarStyle::Overlay` + `traffic_light_position`) instead of delegate swizzling.

---

## 2. Data model (state.rs)

### Enums
| Type | Variants | serde |
|---|---|---|
| `WorkspaceStatus` (state.rs:5-10) | `Running`, `Waiting` | `rename_all="lowercase"` → `"running"`/`"waiting"`. Legacy `"archived"` handled only via raw `Value` in `load` |
| `AgentProvider` (state.rs:12-18) | `Claude` (default), `Codex` | lowercase |
| `ContextBuildStatus` (state.rs:162-170) | `NotBuilt` (default), `Building`, `Built`, `Failed` | `snake_case` → `"not_built"`… |

### Structs (serialized)
**`RepoInfo`** (state.rs:20-27) — `id: String` (uuid v4), `path: PathBuf` (canonicalized), `gh_profile: Option<String>` (gh login), `default_branch: Option<String>` `#[serde(default)]` (cached detection).

**`SourcePr`** (state.rs:29-36) — `number: i64`, `branch: String` (headRefName), `base_branch: String` (baseRefName), `url`, `title`.

**`WorkspaceInfo`** (state.rs:38-64):
- `id: String` (uuid v4), `name: String` (display name; random `adj-noun` or custom branch), `branch: String`, `worktree_path: PathBuf`, `repo_id`, `gh_profile: Option<String>` (copied from repo at creation; not used later — commands re-read the repo's profile), `status: WorkspaceStatus`, `created_at: u64` (unix secs)
- `#[serde(default)]`: `task_title: Option<String>`, `task_description: Option<String>`, `source_todo_id: Option<String>`, `custom_branch: bool` (true ⇒ agent must not auto-rename), `provider_override: Option<AgentProvider>`, `source_pr: Option<SourcePr>`, `source_prs: Option<Vec<SourcePr>>` (combo), `base_branch: Option<String>` (overrides repo default for diff/update; set for PR & combo workspaces).
- `effective_provider(ws, settings)` = `ws.provider_override.unwrap_or(settings.agent_provider)` (state.rs:66-68).

**`NamedScript`** (state.rs:74-80) — `name`, `command` (both `#[serde(default)]`).

**`RepoSettings`** (state.rs:82-113), all `#[serde(default)]`, struct derives `Default`:
- `setup_script: String` — run with `zsh -c` after worktree creation.
- `run_script: String` — private, `skip_serializing` (legacy; migrated).
- `run_scripts: Vec<NamedScript>`.
- `remove_script: String` — `alias = "archive_script"` (legacy name).
- `pr_message: String`, `review_message: String` — prompt templates used by frontend (PR is created by the *agent* running `gh pr create`; there is no backend PR-create command).
- `default_thinking: bool`, `default_plan: bool`, `system_prompt: String`.
- `lsp_servers: HashMap<String, lsp::types::LspServerConfig>`.
- `mcp_servers: HashMap<String, McpServerConfig>` (key must not be "korlap").
- `agent_provider: AgentProvider`.
- `migrate()` (state.rs:144-151): if `run_scripts` empty and legacy `run_script` non-empty → push `{name:"Run", command: run_script}`.

**`McpServerConfig`** (state.rs:115-140) — `server_type: String` default `"stdio"` (or `"sse"`), `command`, `args: Vec<String>`, `env: HashMap`, `url`, `headers: HashMap`.

**`ContextMeta`** (state.rs:172-194) — include/exclude globs, build_status, last_built_at, counts, precheck_model, built_at_commit (context agent's domain).

### Non-serialized runtime handles
- `AgentHandle { child: std::process::Child }` (state.rs:70-72).
- `TerminalHandle { writer: Box<dyn Write+Send>, child: Box<dyn portable_pty::Child+Send>, master: Box<dyn MasterPty+Send> }` (state.rs:154-158).

### `AppState` (state.rs:206-219)
```
repos: HashMap<repo_id, RepoInfo>
workspaces: HashMap<ws_id, WorkspaceInfo>
agents: HashMap<ws_id, AgentHandle>
session_ids: HashMap<ws_id, claude_session_id>
repo_settings: HashMap<repo_id, RepoSettings>
data_dir: PathBuf
mcp_api_port: u16
terminals: HashMap<"<ws_id>:<term_id>" | "repo:<repo_id>:<term_id>", TerminalHandle>
context_meta: HashMap<repo_id, ContextMeta>
context_agents: HashMap<repo_id, AgentHandle>
script_pids: HashMap<ws_id | "repo:<repo_id>", u32>
```

### On-disk layout (`<data_dir>` = Tauri app_data_dir)
| Path | Shape | Written | Read |
|---|---|---|---|
| `repos.json` | JSON **array** of `RepoInfo` (pretty) | `save_repos` (state.rs:326-332): add/clone repo, remove_repo, set_repo_profile, list_repos backfill | `load` (state.rs:224-236); repos whose `path` no longer exists are skipped (not deleted from file until next save) |
| `workspaces.json` | JSON **array** of `WorkspaceInfo` (pretty) | `save_workspaces` (state.rs:334-348) on every workspace mutation | `load` (state.rs:239-276) |
| `sessions.json` | JSON **object** `{ws_id: session_id}` | inside `save_workspaces`, **only if non-empty** (state.rs:340-345) | `load` (state.rs:279-288), errors ignored |
| `repo_settings.json` | object `{repo_id: RepoSettings}` | `save_repo_settings` (state.rs:350-356) | `load` (state.rs:291-303), errors ignored, `migrate()` each |
| `context_meta.json` | object `{repo_id: ContextMeta}` | `save_context_meta` (state.rs:378-384) | `load` (state.rs:306-321); `Building`→`Failed` |
| `workspaces/<dir_name>/` | git worktree; dir_name = random workspace name (NOT the id — CLAUDE.md is wrong on this) or `staging-<repo_id[..8]>` | worktree add | — |
| `messages/<ws_id>.json` | arbitrary JSON (frontend's message array), compact | `save_messages` | `load_messages`, `extract_task_from_messages` |
| `todos/<repo_id>.json` | arbitrary JSON, compact | `save_todos` (+ mcp_api todo routes) | `load_todos` |
| `images/<ws_id>/<uuid>.<ext>` | binary | `save_image` | asset protocol (frontend) |
| `context/<repo_id>/…` | knowledge-base files | context.rs | context.rs |

`AppState::load` details (state.rs:222-324):
- `repos.json` / `workspaces.json` parse errors are **fatal to load** (returns Err → logged; remaining files including sessions/settings are then not loaded!). `sessions.json`, `repo_settings.json`, `context_meta.json` failures are silently ignored.
- Workspaces loaded as `Vec<serde_json::Value>`; entries with `status == "archived"` → delete `messages/<id>.json`, remove session id, skip (state.rs:247-256). (Note: sessions are loaded *after* this, so the session removal is a no-op.)
- `Running` → `Waiting` (state.rs:260-262).
- If `task_title` missing: read `messages/<id>.json`, find first `{role:"user", hidden:true}`, take `chunks[0].content`, split at first `"\n\n"` → title / trimmed description; persist if any backfilled (state.rs:264-275, 410-436).
- No check that `worktree_path` still exists; no check workspace's repo was loaded.

Helpers: `worktree_dir()`=`data_dir/workspaces`, `messages_dir()`, `todos_dir()`, `context_dir(repo_id)`=`data_dir/context/<repo_id>` (state.rs:358-376). `delete_workspace_data(id)` removes messages file, `images/<id>` dir, session id (state.rs:388-392). `is_git_repo(path)` = `git rev-parse --git-dir` in path (state.rs:394-406).

`rename_git_branch(worktree, new, fallback)` (state.rs:441-468): `git branch --show-current` (fallback to stored branch), no-op if equal, else `git branch -m <current> <new>`; error `"git branch rename failed: <stderr>"`. Used by `rename_branch` command, mcp_api `/rename-branch`, and agent auto-rename.

---

## 3. Command table (every `#[tauri::command]` in scope)

Conventions: all return `Result<T,String>`. JS invokes with camelCase arg names (Tauri v2 default: `workspaceId`, `repoId`, `prNumber`, `onData`, …). "auth-git" means `provider.git_cmd_with_auth(dir, token)` = `git -c url.https://x-access-token:<TOKEN>@github.com/.insteadOf=git@github.com: -c url.https://x-access-token:<TOKEN>@github.com/.insteadOf=ssh://git@github.com/ <args>` with cwd and full shell env (github.rs:216-239); when token is None, no `-c` flags. "env" = `inject_shell_env` applied.

### repo.rs
| Command | Params | Returns | Behavior / external commands |
|---|---|---|---|
| `add_repo` (repo.rs:17) async | `path: String` | `RepoDetail` | spawn_blocking → `register_repo(path, None)`: canonicalize; `git rev-parse --git-dir` (env); `detect_default_branch` (`git symbolic-ref refs/remotes/origin/HEAD`, then `git rev-parse --verify refs/remotes/origin/{main,master}` — **no env**); dedupe by canonical path (returns existing; updates gh_profile only if Some given); else new uuid, save repos.json (repo.rs:78-127). Fails if repo has no origin default ref |
| `remove_repo` (repo.rs:28) sync | `repo_id` | `()` | `lsp.shutdown_repo(repo_id)`; remove repo; `workspaces.retain(repo_id != …)` in memory; `save_repos()` only (see bugs) |
| `list_repos` (repo.rs:46) sync | — | `Vec<RepoDetail>` | For each repo use cached default_branch or detect & backfill (`unwrap_or_default` → "" on failure) and save; sort by display_name case-insensitive |

`RepoDetail` (repo.rs:8-14) = `#[serde(flatten)] RepoInfo` + `display_name` (path basename) + `default_branch: String` → JSON `{id, path, gh_profile, default_branch, display_name}` (note `default_branch` key appears from both flattened `RepoInfo.default_branch: Option` and the outer String — serde flatten emits both; outer wins in JS object since it's serialized later, but it's a duplicate key).

### workspace.rs
| Command | Params | Returns | Behavior |
|---|---|---|---|
| `create_workspace` (47) async | `repo_id, task_title?: String, task_description?: String, source_todo_id?: String, custom_branch?: String` | `WorkspaceInfo` | §4.2 |
| `create_workspace_from_pr` (258) async | `repo_id, pr_number: i64` | `WorkspaceInfo` | §4.4 |
| `create_combo_workspace` (425) async | `repo_id, pr_numbers: Vec<i64>` | `WorkspaceInfo` | §4.5 |
| `remove_workspace` (861) async | `workspace_id` | `()` | §4.3 |
| `list_workspaces` (944) sync | `repo_id` | `Vec<WorkspaceInfo>` | filter by repo (HashMap order → unordered) |
| `rename_branch` (960) sync | `workspace_id, new_name` | `WorkspaceInfo` | `rename_git_branch` (git branch --show-current; git branch -m) then sets `ws.branch = ws.name = new_name`, save |

### git.rs (`git_cmd()` = `git` + env, git.rs:11-15)
| Command | Params | Returns | Behavior / commands |
|---|---|---|---|
| `get_repo_head` (20) sync | `repo_id` | `String` | `git rev-parse --abbrev-ref HEAD` in repo root |
| `checkout_default_branch` (45) async | `repo_id` | `()` | spawn_blocking `git checkout <cached default or "main">` in repo root |
| `get_changed_files` (89) async | `workspace_id` | `Vec<ChangedFile{path,status,additions,deletions}>` | §4.6 |
| `get_diff` (184) async | `workspace_id, file_path?: String` | `String` (unified diff) | §4.6 |
| `git_commit` (263) async | `workspace_id, message` | `CommitResult{hash,message}` | empty msg → Err; `git add -A`; `git diff --cached --quiet` (exit 0 ⇒ Err "Nothing to commit — working tree is clean"); `git commit -m <msg>`; `git rev-parse --short HEAD` |
| `git_push` (327) async | `workspace_id` | `()` | resolve token (lenient); branch = `git branch --show-current` (fallback ws.branch); auth-git `push -u origin <branch>` in worktree |
| `check_main_behind` (373) async | `repo_id` | `u32` | auth-git `fetch origin <default>` in repo root (fail → "Could not fetch from origin"); `git rev-list --count <default>..origin/<default>` |
| `sync_main` (423) async | `repo_id` | `()` | §4.8 |
| `check_base_updates` (514) async | `workspace_id` | `BaseUpdateStatus{behind_by:i64}` | best-effort auth-git `fetch origin <base>` (stdout/err null, result ignored); `git merge-base HEAD origin/<base>` (fail → 0); `git rev-list --count <mb>..origin/<base>` |
| `update_from_base` (584) async | `workspace_id` | `()` | auth-git `fetch origin <base>` in worktree (fail → Err "git fetch failed: …"); `git merge origin/<base> --no-edit`; on failure `git merge --abort`; if "CONFLICT" in stdout/stderr → "Merge conflicts detected when updating from <base>. The merge has been aborted." |

Base used by workspace-level git commands: `ws.base_branch ?? repo.default_branch ?? "main"` (git.rs:100-102, 196-198, 523-525, 593-595).

### github.rs (commands) — provider in `git_provider/github.rs`
| Command | Params | Returns | Behavior / commands |
|---|---|---|---|
| `list_gh_profiles` (14) sync | — | `Vec<ServiceProfile{login,active}>` | `gh auth status --json hosts` (env); non-zero exit → `[]`; parse `hosts.*[]. {login, active}` |
| `set_repo_profile` (21) sync | `repo_id, profile?: String` | `()` | set + save repos.json |
| `check_gh_cli` (36) sync | — | `CliStatus{installed,authenticated,profiles}` | `gh --version`; then list_profiles; authenticated = profiles non-empty |
| `gh_auth_login` (43) async | (AppHandle) | `()` | spawn_blocking: `gh auth login --hostname github.com --git-protocol https --web --scopes workflow` (env, stdin+stderr piped), store pid; write `"\n"` to stdin; read stderr lines, strip ANSI; for word starting `https://github.com/login` → `open <url>` (macOS); for word matching `XXXX-XXXX` alnum → emit **`gh-auth-code`** (payload: string code); wait; nonzero → "GitHub authentication was cancelled or failed." |
| `cancel_gh_auth_login` (54) sync | — | `()` | swap pid→0, `kill <pid>` |
| `list_gh_repos` (61) async | `profile, search?: String` | `Vec<RepoEntry{full_name,description,is_fork,clone_url,updated_at}>` | `GH_TOKEN=<token of profile> gh repo list <profile> --json nameWithOwner,description,isFork,sshUrl,updatedAt --limit 100 --source`; client-side filter on name/description contains (lowercase); sort by updated_at desc. **clone_url is the SSH url** |
| `clone_repo` (186) async | `clone_url, repo_name, dest_path?: String, profile` | `RepoDetail` | §4.1 |
| `create_gh_repo` (216) async | `options: CreateRepoOptions{name,private,description?,add_readme}, profile` | `RepoDetail` | `GH_TOKEN=… gh repo create <profile>/<name> --private|--public [-d desc] [--add-readme]` → clone_url `git@github.com:<profile>/<name>.git` → clone_and_register (dest `~/Developer/<name>`) |
| `check_repo_gh_access` (245) async | `path, profiles: Vec<String>` | `Option<String>` | canonicalize; nwo from origin URL; for each profile: `gh auth token --user p`, `GH_TOKEN=… gh repo view <nwo> --json name`; first success wins |
| `get_pr_status` (264) async | `workspace_id` | `PrStatus` | branch = `git branch --show-current` (**run on async thread, no env**) ; spawn_blocking `GH_TOKEN=… gh pr view <branch> --json state,url,number,title,statusCheckRollup,mergeable,additions,deletions` in worktree; non-zero → `{state:"none", checks:"none", mergeable:"unknown", zeros}`; checks: any conclusion ∈ {FAILURE,ERROR,TIMED_OUT} → "failing"; all `status=="COMPLETED"` → "passing"; else "pending"; empty → "none". `ahead_by` = `git rev-list --count origin/<b>..<b>` (no env; failure → 0). state/mergeable lowercased (`open/merged/closed`, `mergeable/conflicting/unknown`) |
| `get_pr_template` (307) sync | `repo_id` | `String` | First existing of `.github/pull_request_template.md`, `.github/PULL_REQUEST_TEMPLATE.md`, `docs/pull_request_template.md`, `PULL_REQUEST_TEMPLATE.md` in **repo root** (not worktree); else "" |
| `gh_pr_merge` (338) async | `workspace_id, pr_number: i64` | `()` | `GH_TOKEN=… gh pr merge <n> --squash --delete-branch=false` (cwd worktree) |
| `list_repo_prs` (372) async | `repo_id` | `Vec<PrEntry{number,title,branch,base_branch,author,url,updated_at,additions,deletions}>` | `GH_TOKEN=… gh pr list --repo <nwo> --json number,title,headRefName,baseRefName,author,url,updatedAt,additions,deletions --limit 50` (open PRs only, gh default) |
| `get_pr_detail` (402) async | `repo_id, pr_number` | `PrDetail{number,title,branch,base_branch,url,body}` | `GH_TOKEN=… gh pr view <n> --repo <nwo> --json number,title,headRefName,baseRefName,url,body` |

### files.rs
| Command | Params | Returns | Behavior |
|---|---|---|---|
| `list_directory` (18) async | `workspace_id, relative_path` | `Vec<FileEntry{name,path,is_dir,size}>` | canonicalize target & root, must `starts_with` root; read_dir; skip only `.git`; path relative to canonical root; sort dirs first then name (case-insens.). Not gitignore-aware |
| `read_file` (95) async | `workspace_id, relative_path` | `String` | boundary check; >2 MiB → Err "File too large (N bytes). Max 2MB for preview."; `read_to_string` (non-UTF8 → Err) |
| `write_file` (185) async | `workspace_id, relative_path, content` | `()` | canonicalize **parent** (must exist), boundary check, write `parent/file_name` (no mkdir -p) |
| `list_repo_directory` (230), `read_repo_file` (140), `write_repo_file` (307) async | `repo_id, …` | same | Same logic on repo root ("Path escapes repo boundary") |
| `search_workspace_files` (364) / `search_repo_files` (382) **sync** | `…_id, query` | `Vec<FileSearchResult{path,name,kind:"file"|"folder",score}>` | §4.12 |
| `grep_workspace` (500) / `grep_repo` (533) async | `…_id, pattern, is_regex: bool, case_sensitive: bool` | `GrepResult{matches: Vec<GrepMatch{path,line_number,column,line_content}>, truncated}` | §4.12 |
| `read_workspace_file` (694) sync | `workspace_id, file_path` | `String` | boundary check; full read; if >100 lines → first 100 + `"\n\n... truncated (100 of N lines shown, B bytes total)"` (used for @-mention previews) |

### terminal.rs — see §4.10
`open_terminal(workspace_id, terminal_id, on_data: Channel<Vec<u8>>)`, `write_terminal(workspace_id, terminal_id, data: Vec<u8>)`, `resize_terminal(workspace_id, terminal_id, rows: u16, cols: u16)`, `close_terminal(workspace_id, terminal_id)`; repo variants `open_repo_terminal/write_repo_terminal/resize_repo_terminal/close_repo_terminal(repo_id, terminal_id, …)`. All sync, return `()`.

### scripts.rs — see §4.11
`run_script(workspace_id, command, on_event: Channel<ScriptEvent>)`, `stop_script(workspace_id)`, `run_repo_script(repo_id, command, on_event)`, `stop_repo_script(repo_id)`. All sync, return `()`.

### persistence.rs
| Command | Params | Returns | Behavior |
|---|---|---|---|
| `get_repo_settings` (9) sync | `repo_id` | `RepoSettings` | cloned or `Default` |
| `save_repo_settings` (22) sync | `repo_id, settings: RepoSettings` | `()` | replace + write repo_settings.json (note: incoming legacy `run_script` is deserialized but never migrated here) |
| `save_messages` (36) sync | `workspace_id, messages: Value` | `()` | mkdir messages/; write compact JSON |
| `load_messages` (53) sync | `workspace_id` | `Value` | missing → `[]` |
| `save_todos` (73) / `load_todos` (90) sync | `repo_id, todos` | | same pattern under `todos/` |
| `test_mcp_server` (110) async | `config: McpServerConfig` | `String` | sse: `curl -s -o /dev/null -w %{http_code} --max-time 5 <url>` → "Server reachable (HTTP nnn)" (note: any HTTP code incl. 404/500 counts as reachable; headers not sent). stdio: spawn `command args` (env + config.env), write MCP `initialize` JSON-RPC line (protocolVersion "2024-11-05", clientInfo korlap 0.1.0), wait ≤5 s for first non-empty stdout line; `error.message` → Err "Server error: …"; `result.serverInfo` → "`<name> v<version>`"; timeout → kill, read stderr (first 200 chars) |
| `save_image` (217) sync | `workspace_id, data: base64 String, extension` | `String` abs path | decode STANDARD base64; mkdir `images/<ws>`; write `<uuid>.<ext or png>` |

### staging.rs
`create_staging_workspace(repo_id, branch_names: Vec<String>) -> StagingResult{workspace, merged_branches, conflicting_branches}` (staging.rs:17) and `remove_staging_workspace(repo_id) -> ()` (staging.rs:231). See §4.9.

### system.rs
`get_system_resources() -> SystemResources{cpu_cores:u32, memory_gb:f64, available_memory_gb:f64}` (system.rs:11): `available_parallelism` (default 4), sysinfo total/available memory in GiB rounded 2 dp. Used by frontend to size autopilot concurrency.

### git_provider (not commands, but core)
- Trait `GitServiceProvider` (git_provider/mod.rs:82-200): `name`, `domain`, `matches_remote_url`, `resolve_token_strict`/`resolve_token` (default impl = `.ok().flatten()`), `list_profiles`, `check_cli`, `auth_login(AppHandle)`, `cancel_auth_login`, `inject_git_auth(cmd, token)`, `git_cmd_with_auth(path, token)`, `cli_cmd_with_auth(path, token)`, `build_auth_env_vars(token)`, `extract_repo_id(path)`, `list_repos`, `create_repo`, `check_repo_access`, `get_pr_status`, `merge_pr`, `list_prs`, `get_pr_detail`.
- `GitProviderRegistry { github }` (mod.rs:207-237): `for_repo(path)` reads `git remote get-url origin` (env) and always returns GitHub (fallback too); `github()` direct.
- `build_auth_env_vars` (github.rs:251-264) → `GH_TOKEN=<t>` and `GIT_CONFIG_PARAMETERS='url.https://x-access-token:<t>@github.com/.insteadOf=git@github.com:'` (used by agent spawn, outside scope).
- `extract_repo_id` (github.rs:268-291): strip `git@github.com:` prefix else split on `github.com/`; trim `.git`.

---

## 4. Key flows

### 4.1 Add / clone repo
**Add local** (`add_repo`): path → canonicalize → `git rev-parse --git-dir` → `detect_default_branch` (hard error if no origin refs: "Could not detect default branch from remote. No origin/HEAD, origin/main, or origin/master found. Run `git remote set-head origin --auto` …", helpers.rs:44-49) → dedupe by path → persist. The frontend then typically calls `check_repo_gh_access(path, profiles)` and `set_repo_profile`.

**Clone** (`clone_and_register`, github.rs:77-183):
1. dest = `dest_path` or `$HOME/Developer/<repo_name>`.
2. If dest exists: has `.git` → just `register_repo(dest, Some(profile))`; else Err "Destination already exists: …".
3. mkdir parent; `git -c insteadOf… clone <clone_url> <dest>` (+env). SSH URLs get rewritten to HTTPS-with-token, so no SSH key needed.
4. Empty-repo bootstrap: if `git rev-parse HEAD` fails → write `README.md` (`# <name>\n`), `git add README.md`, `git commit -m "Initial commit"`, get branch via `git branch --show-current` (fallback "main"), auth-git `push -u origin <branch>`.
5. `register_repo(dest, Some(profile))` (persists gh_profile).
Note: after a clone the **remote URL stored in the repo is the SSH URL** — every later auth-git command relies on the insteadOf rewrite; plain `git` (no auth) operations against origin would need SSH keys.

### 4.2 Create workspace (`create_workspace`, workspace.rs:47-255)
1. Read repo path + gh_profile (lock, release).
2. `provider = for_repo(repo_path)`; **strict** token: `gh auth token --user <profile>`; failure → Err "Cannot authenticate as GitHub profile '<p>'. Fix your auth or change the repo's profile.\n<stderr>" (67-78). No profile → token None (OK).
3. `base_branch = detect_default_branch(repo)` (fresh, not cached).
4. auth-git `fetch origin <base>` in repo root. On failure classify stderr (lowercased) into hints (94-113): "repository not found"/"could not read from remote" → profile access hint; "could not resolve host" → network; "permission denied"/"authentication failed" → token expired (`gh auth refresh`); else generic. Err "Could not fetch from origin.\n<hint>\n\n<stderr>".
5. Start point = `origin/<base>` (always remote tip, never local).
6. Naming (130-177):
   - custom_branch given: trim, empty → Err; `git rev-parse --verify <cb>` succeeds → Err "Branch '<cb>' already exists"; dir = random name; branch = display name = cb.
   - else: `random_workspace_name()` = `ADJ-NOUN` from 30 adjectives × 30 nouns chosen by `DefaultHasher(SystemTime nanos)`: adj = `h % 30`, noun = `(h>>16) % 30` (workspace.rs:12-42). Loop up to 10: branch `korlap/<name>`; accept if branch doesn't verify AND `worktree_dir/<name>` doesn't exist; otherwise name = `<random>-<first 4 chars of uuid v4>`; 10th failure → Err.
7. `mkdir -p worktree_dir`; `git worktree add -b <branch> <data_dir/workspaces/<dir>> origin/<base>` in repo root (no env). Err "git worktree add failed: …".
8. Build `WorkspaceInfo{status: Waiting, created_at: now, custom_branch: custom_branch.is_some(), base_branch: None, provider_override: None, …}`.
9. Setup script (if `repo_settings[repo].setup_script.trim()` non-empty): `zsh -c <script>` cwd worktree, env `KORLAP_WORKSPACE_NAME`, `KORLAP_WORKSPACE_PATH`, `KORLAP_ROOT_PATH` (repo root), `KORLAP_DEFAULT_BRANCH`, + shell env. **Synchronous**, output captured and discarded; non-zero exit only logged (creation still succeeds). Spawn failure → Err (worktree left behind, not registered!).
10. Insert + `save_workspaces`. Return ws. (No event emitted; frontend uses return value.)

Branch auto-rename later: `custom_branch == false` workspaces get renamed by the agent pipeline (agent.rs ~1000) or by the agent calling MCP `/rename-branch` → `workspace-updated` event.

### 4.3 Remove workspace (`remove_workspace`, workspace.rs:861-941)
1. Lock: remove & `kill()`+`wait()` agent child; `kill_workspace_terminals` (kills PTY children with prefix `<ws_id>:`, no wait); read paths. Release.
2. `lsp_manager.remove_worktree(repo_id, worktree_path)`.
3. Remove script (if set): `zsh -c <remove_script>` cwd worktree, env NAME/PATH/ROOT_PATH (**no KORLAP_DEFAULT_BRANCH**) + shell env; output ignored; runs while holding the AppState lock (892-907).
4. If worktree exists: `git worktree remove --force <path>` (failure → `git worktree prune`); else `git worktree prune`.
5. `delete_workspace_data` (messages, images, session), remove from map, save.
- Branch is **not deleted** (local `korlap/*` branches accumulate; this also prevents name reuse via the rev-parse check). Script pids not cleaned/killed. Context agent not touched.

### 4.4 PR checkout workspace (`create_workspace_from_pr`, workspace.rs:258-422)
1. Lenient token (`resolve_token`). nwo from origin URL.
2. `gh pr view <n> --repo <nwo> --json number,title,headRefName,baseRefName,url,body`.
3. auth-git `fetch origin pull/<n>/head` in repo root (fork-safe).
4. Name loop identical but branch prefix `korlap/review-<name>`.
5. `git worktree add -b korlap/review-<name> <path> FETCH_HEAD` (repo root).
6. Body truncated to 4000 **bytes** + "…" (355-359).
7. `WorkspaceInfo{ task_title: "Review PR #<n>: <title>", task_description: body or None, custom_branch: true (prevents agent rename), source_pr: Some(SourcePr{…}), base_branch: Some(pr.baseRefName) }`.
8. Setup script with `KORLAP_DEFAULT_BRANCH = detect_default_branch() or "main"`; spawn failure ignored here (inconsistent with 4.2).
9. Persist.
Note: The local branch is `korlap/review-*`, not the PR's head branch, so `git_push` would push a new branch rather than updating the PR (by design for review use).

### 4.5 Combo workspace (`create_combo_workspace`, workspace.rs:425-858)
1. Dedupe PR numbers preserving order; < 2 → Err.
2. Lenient token; nwo; fetch `PrDetail` for each (sequential gh calls).
3. `base_branch = detect_default_branch`.
4. Validation: every PR's `base_branch` must be the default branch or the head branch of another selected PR, else Err "PR #n (title) targets branch 'x' which is not the default branch and not provided by any other selected PR…" (490-498).
5. Kahn topological sort: edge dep→i when `pr_i.base_branch == pr_dep.branch`; leftover nodes ⇒ "Circular dependency detected among PRs: #a (b) → …". Queue seeded in input order, so ties keep selection order.
6. auth-git `fetch origin <base>`; name loop with `korlap/combo-<name>`; `git worktree add -b <branch> <path> origin/<base>`.
7. For each PR in topo order:
   - auth-git `fetch origin pull/<n>/head` (repo root). Failure → cleanup (`git worktree remove --force <path>`, `git branch -D <branch>`) and Err with "Already merged: #…" list.
   - `git rev-parse FETCH_HEAD` **in repo root** (FETCH_HEAD lives in main .git, not the worktree gitdir — comment 675-677) → SHA.
   - `git merge <sha> --no-edit` in worktree (+env). Failure → `git ls-files --unmerged --deduplicate` → conflicted file names (field after `\t`, sorted, deduped), else raw merge output; `git merge --abort`; cleanup worktree+branch; Err "Merge conflict when adding PR #n (title) [i/N].\n\nConflicting files:\n  • f…\nSuccessfully merged before conflict: #…".
8. `task_title = "Combo: #a + #b …"`, `task_description = "Combined PRs for integration testing:\n- PR #n: title (url)…"`, `custom_branch: true`, `source_prs: Some(sorted)`, `base_branch: Some(default)`.
9. Setup script (KORLAP_DEFAULT_BRANCH=base), failures logged. Persist (error message "Combo workspace created successfully but failed to persist metadata").
Merges use the user's git identity (merge commits are created in the worktree).

### 4.6 Diff computation (git.rs:88-252)
- Base = `ws.base_branch ?? repo.default_branch ?? "main"`; `remote_base = origin/<base>`.
- `merge_base = git merge-base origin/<base> HEAD` (fallback: the literal `origin/<base>`). No fetch is done — uses whatever origin ref is locally present (refreshed by check_base_updates / create).
- **Changed files**: `git diff --numstat <merge_base>` → compares merge-base to the **working tree** (includes committed + staged + unstaged changes). Per line `adds\tdels\tpath`; status heuristic: adds>0&&dels>0 → "M", adds>0 → "A", else "D". Then `git ls-files --others --exclude-standard` → untracked files appended with status "A", additions = line count (read_to_string; binary → 0).
- **Diff**: `git diff <merge_base> [-- <file>]`. If empty AND file_path given AND file exists → synthesize a new-file diff: `--- /dev/null\n+++ b/<fp>\n@@ -0,0 +1,<N> @@\n` + `+line\n` each (for untracked files). Whole-workspace diff never includes untracked files.

### 4.7 Commit / push / PR / merge
- Commit: §3 `git_commit` (stages *everything* with `-A`).
- Push: `git_push` → `push -u origin <current branch>` with token rewrite.
- **PR creation is not a backend command.** The frontend (`src/routes/+page.svelte` ~2101-2113) sends the agent a prompt built from `RepoSettings.pr_message` (templated `{{file_count}}`, `{{branch}}`, `{{base_branch}}`) instructing it to commit, push and run `gh pr create --base <base>`. The agent has `GH_TOKEN` injected (agent spawn, out of scope).
- Status polling: `get_pr_status` (§3).
- Merge: `gh_pr_merge` → `gh pr merge <n> --squash --delete-branch=false`. Post-merge context update is `update_context_after_merge` (context.rs, out of scope).

### 4.8 Base branch sync / update
- **Repo default branch ("main") sync** — `check_main_behind` counts `<default>..origin/<default>`; `sync_main` (git.rs:423-504): auth-git fetch (hints for host/auth failures) → `git rev-parse --abbrev-ref HEAD` in repo root; if HEAD == default → `git merge --ff-only origin/<default>`; else → `git update-ref refs/heads/<default> refs/remotes/origin/<default>` (unconditional ref move — see bugs).
- **Workspace update from base** — `check_base_updates` (behind count since merge-base) and `update_from_base` (merge origin/<base> with --no-edit, abort on failure). Uses `ws.base_branch` so PR workspaces track the PR's base.

### 4.9 Staging / autopilot workspace (staging.rs)
`create_staging_workspace(repo_id, branch_names)`:
1. Lenient token; `detect_default_branch`; auth-git `fetch origin <base>` (fail → Err "Failed to fetch origin/<base>: …").
2. If a workspace with `branch == "korlap/staging"` for this repo exists (lock held throughout, staging.rs:46-92): kill agent (kill+wait), kill terminals, `git worktree remove --force <path>`, `git branch -D korlap/staging`, `delete_workspace_data`, remove from map (not saved until the end).
3. Path `worktree_dir/staging-<repo_id[..8]>`. Always: if path exists → `git worktree remove --force`; legacy `worktree_dir/staging` → remove too; `git worktree prune`; `git branch -D korlap/staging` (all errors ignored).
4. `git worktree add -b korlap/staging <path> origin/<base>`.
5. For each branch name: auth-git `fetch origin <branch>` (failure → conflicting list, continue); `git merge origin/<branch> --no-edit` in staging worktree; failure → `git merge --abort`, conflicting list.
6. New `WorkspaceInfo{name:"staging", branch:"korlap/staging", task_title:"Staging", custom_branch:false, base_branch:None, …}`, insert, save. Return merged/conflicting lists.
Note the staging workspace has `custom_branch:false` — if an agent ever runs in it, auto-rename logic could rename `korlap/staging` (then the find-by-branch lookups break).

`remove_staging_workspace(repo_id)`: find by branch; kill agent/terminals; `git worktree remove --force` (fallback prune) / prune; `git branch -D korlap/staging`; delete data; save.

### 4.10 Default branch detection (helpers.rs:14-50)
Tier 1 `git symbolic-ref refs/remotes/origin/HEAD` → strip `refs/remotes/origin/`. Tier 2 `git rev-parse --verify refs/remotes/origin/main` then `…/master`. Else hard error. Never local branches. Runs **without** `inject_shell_env`. Cached in `RepoInfo.default_branch` at registration (and backfilled in list_repos), but `create_workspace`, combo, PR and staging re-detect each time; `get_changed_files`/`git_*`/`check_main_behind`/`sync_main`/`checkout_default_branch` use the cache with "main" fallback. The cache is never refreshed if the remote default changes.

### 4.11 gh profile token injection
- Token: `gh auth token --user <login>` per call (no caching) (github.rs:44-65). Never `gh auth switch`.
- git: `-c url."https://x-access-token:<T>@github.com/".insteadOf=git@github.com:` and `…=ssh://git@github.com/` prepended to git args (github.rs:216-231). HTTPS remotes (`https://github.com/...`) are NOT rewritten → they use whatever credential helper is configured (typically `gh auth git-credential` for the *active* gh account, which may be the wrong profile).
- gh: `GH_TOKEN=<T>` env per process (github.rs:241-249, and manual in list_repos/create_repo/check_repo_access/list_prs/get_pr_detail).
- Profiles come from `gh auth status --json hosts` (requires gh ≥ 2.6x supporting `--json` on auth status).

### 4.12 Shell env injection (helpers.rs:59-262)
- `get_shell_env()` — `OnceLock<ShellEnv>`, resolved on first use:
  - shell = `$SHELL` or `/bin/zsh`.
  - `ssh_auth_sock` = `$SSH_AUTH_SOCK` or `launchctl getenv SSH_AUTH_SOCK`.
  - `home` = `$HOME`.
  - Each probe runs `<shell> -lic "echo __KORLAP_ENV__; <X>; echo __KORLAP_ENV__"` (fish: `--login --interactive -c`), stderr nulled, and takes the text between the first two delimiters (`extract_delimited`): `$PATH`, `command -v claude`, `command -v codex` (results containing "not found" dropped), and `/usr/bin/env` (full env).
  - Env parsing: lines `KEY=VALUE` where KEY matches `[A-Za-z0-9_]+` start a new var; other lines are appended (with `\n`) to the previous value (multi-line values).
  - So 4 sequential interactive login-shell spawns, **no timeout** (a shell rc that waits for input hangs the first git command forever).
- `inject_shell_env(cmd)` — `cmd.envs(all_vars)` + `SSH_AUTH_SOCK` fallback. Note: it *adds* to (doesn't clear) the inherited env, and it's applied **after** e.g. `GH_TOKEN` in `cli_cmd_with_auth`? — no: `cli_cmd_with_auth` calls inject first then sets GH_TOKEN, correct. But in `clone_and_register` the order is `inject_git_auth` then `inject_shell_env` — fine since one is args and one env. If the user's shell exports `GH_TOKEN`, `list_repos`/`create_repo` set it after inject (OK), but commands using bare `Command::new("gh")` + inject only (list_profiles, check_cli, resolve_token) inherit the user's GH_TOKEN, which makes `gh auth token --user X` return the env token rather than X's (gh precedence) — subtle multi-profile bug.
- `strip_ansi` (helpers.rs:228-244): drop ESC and everything up to the next ASCII letter.
- `now_unix`, `repo_display_name` (basename).

### 4.13 Terminal PTY lifecycle (terminal.rs)
- Keys: workspace `"<ws_id>:<terminal_id>"`, repo `"repo:<repo_id>:<terminal_id>"` (terminal.rs:8-15).
- `open_*`: if key exists → `Ok(())` (idempotent; **new Channel is ignored**, the old reader keeps sending to the old channel — re-mounted frontends get no output). Else `spawn_pty(cwd, key, on_data)`:
  - `native_pty_system().openpty(24×80)`.
  - `CommandBuilder::new($SHELL or /bin/zsh)` + `-l` (login, non-interactive flag not needed since it's a tty); cwd; env `TERM=xterm-256color`, `COLORTERM=truecolor`, `SHELL`, `LANG`=`$LANG` or `en_US.UTF-8`, `LC_ALL`=same; then all captured shell env vars + SSH_AUTH_SOCK fallback.
  - spawn on slave, **drop slave**, `take_writer`, `try_clone_reader`, insert `TerminalHandle` (lock).
  - Reader thread: 4096-byte reads → `on_data.send(Vec<u8>)` (Tauri `ipc::Channel<Vec<u8>>`, arrives as number array / ArrayBuffer in JS) until EOF/error; then logs. **No exit event; handle not removed** from the map on EOF (a shell that exits via `exit` leaves a dead handle; `open_terminal` will then refuse to reopen and writes fail silently or error).
- `write_*`: lock, `write_all(data)` to master writer (lock held during write).
- `resize_*`: `master.resize(rows, cols)`.
- `close_*`: remove, `kill()` + `wait()`.
- Bulk kill: `kill_workspace_terminals` (used by remove_workspace/staging). `kill_repo_terminals` exists but is **never called** (remove_repo leaks repo terminals).

### 4.14 Script runner (scripts.rs)
- `zsh -c <command>` (hard-coded zsh, not `$SHELL`), cwd worktree/repo root, stdin null, stdout/stderr piped, env injected, `process_group(0)` (new pgid = pid) (scripts.rs:34-42).
- pid stored in `script_pids[ws_id]` or `["repo:<repo_id>"]`.
- Thread: spawns a stderr-reader sub-thread; both read **line by line** and send `ScriptEvent::Output{data: line+"\n"}`; after join → `child.wait()` → `ScriptEvent::Exit{code: Option<i32>}` (None when killed by signal); then remove pid.
- `ScriptEvent` JSON: `{"type":"output","data":"…"}` / `{"type":"exit","code":0|null}` (`#[serde(tag="type")]`, scripts.rs:9-16).
- `stop_*`: `kill -- -<pid>` (SIGTERM to the process group). No SIGKILL escalation.
- Concurrency: one pid slot per workspace; a second `run_script` overwrites the pid (first becomes unkillable) and the first's completion removes the second's pid.

### 4.15 File listing / search / grep (files.rs)
- Listing: raw `read_dir`, hides only `.git`, does not respect .gitignore (node_modules shown). Sorted dirs-first.
- Fuzzy search (`search_files_in_dir`, 399-481): query trimmed+lowercased (empty → []); `ignore::WalkBuilder` (hidden files included, .gitignore/global/exclude respected, max_depth 12); skip root and `.git/**`; `SkimMatcherV2` score = max(fuzzy(name), fuzzy(rel_path)); score>0 kept; dirs as `path/` with kind "folder"; sort desc; top 20. Runs synchronously over the entire tree on every keystroke.
- Grep (`grep_in_dir`, 565-691): non-regex → `regex::escape`; case-insensitive mode = **smart case** (`case_smart(true)`: insensitive unless pattern has uppercase); `case_sensitive=true` → default (sensitive). Walker same as above + `max_filesize 1 MiB`. Limits: 100 total matches (→ `truncated=true`), 5 matches/file, line truncated to 500 bytes + "…". Column = byte offset of first match in the line. Paths relative (prefix strip + leading `/` trim). Binary detection: grep-searcher default (no `BinaryDetection::quit`), UTF8 sink errors skip the file.

---

## 5. Events emitted to the frontend

From in-scope files:
| Event | Payload | Source |
|---|---|---|
| `gh-auth-code` | string device code, e.g. `"ABCD-1234"` | github.rs:190 (during `gh_auth_login`) |

Channels (not events) from in-scope commands:
| Channel param | Item type | Source |
|---|---|---|
| `on_data` (open_terminal / open_repo_terminal) | `Vec<u8>` raw PTY bytes | terminal.rs:141 |
| `on_event` (run_script / run_repo_script) | `ScriptEvent` `{type:"output",data}` / `{type:"exit",code}` | scripts.rs:75-98, 194-217 |

Emitted elsewhere but relevant to core state (for completeness):
| Event | Payload | Source |
|---|---|---|
| `workspace-updated` | full `WorkspaceInfo` | mcp_api.rs:160 (agent `/rename-branch`), agent.rs:1011 (auto-rename) |
| `agent-status` | `{workspace_id, status:"running"|"waiting"}` | agent.rs:329, 666 |
| `agent-notify` | arbitrary JSON from agent | mcp_api.rs:213 |
| `todos-changed` | `{repo_id}` | mcp_api.rs:799/851/895 |
| `lsp-status` | `{…}` | lsp.rs / mcp_api.rs |
(Agent stream events at agent.rs:501 are covered by the agent analysis.)

No backend command in scope emits on workspace create/remove, repo add/remove, or settings save — the frontend relies on return values.

---

## 6. Gotchas, edge cases & things a rebuild must replicate

1. **Worktrees live in app data**, not in the repo: `<data_dir>/workspaces/<random-name>`; dir name ≠ workspace id. Staging: `staging-<first 8 chars of repo_id>`.
2. **Always branch from `origin/<default>`** after a fresh `git fetch origin <default>`; never from local branch.
3. **FETCH_HEAD resides in the main repo `.git`**: resolve SHA in repo root before merging in a worktree (combo). `create_workspace_from_pr` passes `FETCH_HEAD` to `worktree add` run in the repo root — OK.
4. Branch prefixes: `korlap/<name>`, `korlap/review-<name>`, `korlap/combo-<name>`, `korlap/staging`. Name collision avoidance checks both local branch and worktree folder; retries with `-xxxx` uuid suffix, 10 attempts.
5. `custom_branch: true` is the "do not auto-rename" flag (set for custom names, PR review, combo).
6. Diff base = merge-base with `origin/<ws.base_branch ?? repo.default_branch ?? "main">`, diffed against the working tree; untracked files appended separately; single-file untracked diff synthesized.
7. Setup/remove scripts: `zsh -c`, cwd = worktree, env `KORLAP_WORKSPACE_NAME`, `KORLAP_WORKSPACE_PATH`, `KORLAP_ROOT_PATH`, (`KORLAP_DEFAULT_BRANCH` for setup only). Setup script failure is non-fatal.
8. Token injection is per-process and never global; SSH remotes are silently converted to HTTPS+token by `insteadOf`.
9. Shell env captured once from an interactive login shell, delimited with `__KORLAP_ENV__`, and injected into most (not all) child processes — essential for GUI-launched macOS apps (nvm/fnm/volta PATH, CARGO_TARGET_DIR, etc.).
10. Restart semantics: `Running → Waiting`; context `Building → Failed`; archived migration; repos with missing paths dropped from memory.
11. Errors are human-readable strings with remediation hints (fetch hint classifier in create_workspace and sync_main); the UI displays them verbatim.
12. Mutex pattern: lock → clone needed data → drop → spawn process. (Violated in several places — §7.)
13. Async vs sync: in Tauri v2 **sync commands run on the main thread**; async commands run on the tokio runtime. Heavy work is put in `spawn_blocking` in git.rs/github.rs/files.rs but **not** in workspace.rs/staging.rs (blocking `std::process` calls directly inside async fns, which ties up a tokio worker).
14. Timeouts: none on git/gh subprocesses. Only `test_mcp_server` has 5 s timeouts (curl `--max-time 5`, stdio `recv_timeout(5s)`).
15. Path-escape protection for file commands uses `canonicalize` + `starts_with` (symlinks pointing outside are rejected for reads; for writes only the parent is checked).
16. Images are stored outside worktrees and served via the asset protocol scope `$APPDATA/images/**` — a rebuild must keep that scope in sync with `save_image`'s path.
17. `RepoDetail` flattens `RepoInfo`; JSON contains `default_branch` twice (Option from RepoInfo + String) — frontend sees the latter.
18. JSON writes are non-atomic `std::fs::write` (rebuild should write-temp-then-rename).

---

## 7. Weaknesses, bugs & rule violations (CLAUDE.md rules referenced)

### Panics / unwrap (rule: "never panic, never unwrap in command handlers"; "No unwrap()/expect() outside tests")
- repo.rs:52 and repo.rs:58 — `state.repos.get(id).unwrap()` / `get_mut(id).unwrap()` in `list_repos` (safe by construction but violates rule).
- lib.rs:61 `state.lock().unwrap()`, lib.rs:70 `.expect("main window")`, mcp_api.rs:20-21 `.expect` on bind — startup panics.
- **workspace.rs:356** `&pr_body[..4000]` — byte slice; panics if byte 4000 is inside a multi-byte UTF-8 char (emoji/CJK in PR body ⇒ command panics). Use `chars().take()` or `floor_char_boundary`.
- **files.rs:661-662** `content.truncate(max_line_len)` on byte index — panics (inside spawn_blocking → JoinError → "…" error, and for grep the whole search fails) for lines with multibyte chars at position 500.

### Lock discipline (rule: never hold locks across spawned processes or I/O)
- workspace.rs:965-984 `rename_branch` holds AppState lock while running two git processes.
- workspace.rs:892-907 `remove_workspace` runs the user's **remove script** (`zsh -c`, unbounded duration) while holding the AppState lock — freezes every other command and the MCP API for the script's duration. Also 867-873 kill+wait agent under lock.
- staging.rs:46-92 holds the lock while running `git worktree remove --force` and `git branch -D` and killing/waiting the agent.
- terminal.rs:185-192 / 273-280 `write_*` holds the global AppState lock during PTY `write_all` (can block if the PTY buffer is full, e.g. a paused program) — stalls entire app. Same for `close_*` (`wait()` under lock, 233-236).
- All `save_*` methods do file I/O under the lock (state.rs:326-384 called while locked everywhere).
- github.rs:21-30 etc. minor.

### Blocking on async runtime / main thread
- workspace.rs:47, 258, 425, 861 and staging.rs:17, 231 are `async fn` but run blocking `std::process::Command::output()` (git fetch can take many seconds) directly — no `spawn_blocking`.
- github.rs:287-294 `get_pr_status` runs `git branch --show-current` on the async thread before spawn_blocking.
- persistence.rs:110 `test_mcp_server` async but blocking (curl / recv_timeout up to 5 s).
- Sync commands on main thread doing heavy work: `search_workspace_files`/`search_repo_files` (files.rs:364/382, full tree walk per keystroke), `list_repos` (may run git), `get_repo_head`, `list_gh_profiles`/`check_gh_cli` (spawn gh), `read_workspace_file`, `open_terminal`, `run_script`, `save_image`, `save_messages`.

### Timeouts (rule: "Async filesystem/process ops must have a timeout")
- No timeout on any git fetch/push/clone, gh call, setup/remove script, or the 4 login-shell probes in `get_shell_env` (helpers.rs:115-203). A hung network or interactive rc file blocks forever (and, combined with locks above, freezes the app).

### Correctness bugs
- **git.rs:483-497 `sync_main`**: when the default branch isn't checked out, `git update-ref refs/heads/<b> refs/remotes/origin/<b>` moves the ref **unconditionally** — not a fast-forward check; local unpushed commits on main are silently discarded. Also if `<b>` is checked out in *another* worktree, its index/working tree become inconsistent. Should verify `merge-base --is-ancestor <b> origin/<b>` first (or `git fetch origin <b>:<b>`, which refuses non-FF).
- git.rs:402-416 `check_main_behind` errors if local default branch doesn't exist (fresh clone with different default), and ignores local-ahead state.
- git.rs:137-146 status heuristic: binary files (`-\t-`) → "D"; modified files with only deletions → "D"; files with only additions → "A" even if modified; renames show `old => new` paths (no `-M`/`--name-status`). Real status should come from `git diff --name-status`.
- git.rs:233-247 synthetic diff: if a *tracked* file has no changes, a full "new file" diff is fabricated for it.
- github.rs:499-525 checks rollup: `StatusContext` entries (commit statuses) have `state`, not `status`/`conclusion` → never "COMPLETED" → PRs with legacy statuses stay "pending" forever; `CANCELLED`, `ACTION_REQUIRED`, `STARTUP_FAILURE` not counted as failing.
- github.rs:528-544 `ahead_by` = `origin/<b>..<b>`: if branch never pushed, rev-list fails → 0 (should be "all commits"); no env injected.
- github.rs:179 `open <url>` is macOS-only (Linux needs `xdg-open`; should use tauri-plugin-opener). github.rs:182-192 device-code detector also matches any `XXXX-XXXX` token in output (multiple emits possible).
- github.rs:216-231 insteadOf only covers SSH forms — HTTPS remotes don't use the profile token for git operations; profile selection is effectively ignored for `https://github.com/...` remotes (falls back to the active gh credential helper account).
- github.rs:251-264 `GIT_CONFIG_PARAMETERS` only includes the `git@github.com:` rewrite, not `ssh://`.
- Token exposure: tokens are passed on the git command line (`-c url.https://x-access-token:<T>@…`) → visible in `ps` to other local users; also echoed in git error messages ("fatal: unable to access 'https://x-access-token:ghp_…@github.com/…'") which are returned verbatim to the UI (workspace.rs:115-119, git.rs:359-362).
- helpers.rs:91-213: `GH_TOKEN` exported in the user's shell is captured into `all_vars` and inherited by `gh auth token --user X` calls → gh returns the env token, defeating multi-profile.
- repo.rs:28-43 `remove_repo`: removes workspaces from memory but **doesn't save workspaces.json** (they reappear on restart as orphans whose repo is missing → most commands fail "Repo not found"), doesn't remove their worktrees/branches/messages, doesn't kill their agents/terminals/scripts, doesn't delete repo_settings/todos/context, and never calls `kill_repo_terminals` (dead code, terminal.rs:37).
- state.rs:229-235 repos with missing paths are dropped but their workspaces are still loaded (orphans).
- state.rs:250-256 archived migration removes session ids before sessions are loaded (no-op); sessions.json not rewritten when `session_ids` becomes empty (state.rs:340) → stale sessions revive deleted workspace ids (harmless but leaks).
- state.rs:239-258: a single malformed workspace entry aborts loading of *all* workspaces and everything after (sessions, settings, context meta).
- workspace.rs:228-247 vs 397-414: setup-script spawn failure is fatal in `create_workspace` (after the worktree is already created → orphaned worktree on disk, not tracked) but ignored in PR/combo.
- workspace.rs:184-198 worktree add errors leave nothing to clean, but setup failures / save failures after worktree creation leave orphans; no rollback except in combo merge path.
- workspace.rs:910-931 `remove_workspace` never deletes the local branch.
- workspace.rs:137-144 custom branch existence check is local-only (`rev-parse --verify <cb>`); a remote branch with the same name will collide on push; no `git check-ref-format` validation.
- Concurrency: two concurrent `create_workspace` calls can pick the same random name (check-then-create race); two concurrent PR fetches in the same repo race on FETCH_HEAD (workspace.rs:285-344).
- terminal.rs:162-166 / 250-254 open is check-then-spawn-then-insert without holding the lock → concurrent opens leak a shell; reopen with a new Channel is ignored; reader thread never removes the handle or emits an exit event (CLAUDE.md rule: "PTY reader threads … emit `agent-status` event on exit").
- terminal.rs:31, 49 kill without `wait()` → zombie processes.
- terminal.rs:87 forces `LC_ALL=$LANG`, overriding user locale categories.
- scripts.rs:72-93: reader uses `lines()` which errors on invalid UTF-8 → the reader `break`s, stops draining the pipe, the child blocks on a full pipe and `child.wait()` never returns (deadlock, script "running" forever, Exit never sent). Use byte reads + lossy conversion.
- scripts.rs:53, 102: single pid slot per workspace (overwrites / wrong cleanup race described in §4.14). No SIGKILL fallback; `kill` shells out instead of `libc::killpg`.
- scripts.rs:34, workspace.rs:230 etc.: hard-coded `zsh` (fails on Linux/Windows without zsh; ignores user's `$SHELL`), while terminals and env probes use `$SHELL`. `std::os::unix::process::CommandExt` (scripts.rs:2) makes the crate Unix-only.
- staging.rs:95 `&repo_id[..min(8)]` byte slice — fine for UUIDs but would panic on non-ASCII ids. Staging ws has `custom_branch:false` (auto-rename risk). Fetch failures are reported as "conflicting".
- files.rs:185-225 `write_file` cannot create files in new directories (parent must exist); boundary check doesn't prevent writing through a symlinked *file* that points outside.
- files.rs:18-92 directory listing ignores .gitignore (huge node_modules listings).
- persistence.rs:116-132 SSE test treats any HTTP status as success and ignores configured headers. persistence.rs:139-147 stdio test: if the server never writes, reader thread is leaked blocked on read until kill (ok), but `recv_timeout` blocks the async runtime.
- persistence.rs:22-31 `save_repo_settings` doesn't validate that `mcp_servers` excludes "korlap" (rule enforced elsewhere?).
- persistence.rs:217-245 `save_image` doesn't sanitize `extension` (e.g. `../x`) → path traversal within data dir; `workspace_id` also unsanitized in messages/images/todos paths (`format!("{}.json", workspace_id)` — `../../foo`).
- `detect_default_branch` (helpers.rs:16-41), `git worktree add` (workspace.rs:187, 341, 611), `rev-parse` checks, `ls-files`, combo cleanup, `get_pr_status`'s git calls, `clone_and_register`'s README commit — all run **without** `inject_shell_env` (inconsistent; may pick wrong git from minimal PATH when launched from Finder, and git hooks needing PATH tools — e.g. husky/lefthook post-checkout — fail).
- Tauri `csp: null` — any XSS in rendered agent markdown has full IPC access (write_file, run_script = RCE). Combined with the unauthenticated MCP HTTP API on 127.0.0.1 (any local process/browser page via DNS-rebinding-free POST can call `/rename-branch`, `/todos/*`, `/notify`).
- CLAUDE.md docs drift: says worktrees are `<data_dir>/workspaces/<workspace-id>/` (actually random name dir).

### Rebuild recommendations (brief)
- Single `run_git(args, cwd, auth, timeout)` helper that always injects env, applies timeout, redacts tokens in errors, and returns structured errors. Use `GIT_CONFIG_COUNT/KEY_n/VALUE_n` env vars instead of `-c` argv for tokens, and rewrite `https://github.com/` too.
- Put all subprocess work in `spawn_blocking` (or `tokio::process` with timeouts); make heavy sync commands async.
- Atomic JSON writes; per-entry tolerant workspace loading; persist workspaces on `remove_repo`.
- Validate ids/extensions used in file paths; char-boundary-safe truncation.
- PTY: remove handle + emit exit event on EOF; allow channel re-attach; per-terminal locks instead of global AppState lock.
- Use `git diff --name-status -M` + `--numstat` for accurate file status.
