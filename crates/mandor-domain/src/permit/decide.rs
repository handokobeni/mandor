//! Keputusan izin bersama hook dan daemon (KTD-A8 plan 2026-11-06-001):
//! tolak, izinkan eksplisit, atau tanpa keputusan (Claude Code memutuskan
//! sendiri, lalu meminta persetujuan lewat kartu).
//!
//! Fakta filesystem datang dari pemanggil lewat [`PathFacts`]; domain hanya
//! menyusun path dan memutuskan.

use serde_json::Value;

use super::BashAllowlist;
use super::args::dangerous;
use super::shell::{SimpleCommand, parse};
use crate::hook::{
    DesignRoot, ToolCall, Verdict, check_design_path, classify_planning, classify_vm,
    classify_with_mcp, in_protected_dir, is_credential_path, is_write_tool, program_name,
};
use crate::hook_review::{
    LabGuard, ReviewState, classify_lab, classify_lab_setup, classify_review,
};

/// Jenis giliran agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TurnKind {
    Implement,
    Plan,
    Review,
    /// Reviewer lab (plan 2026-11-10-001, KTD-L10).
    Lab,
    /// Pengusul perlengkapan lab (KTD-L14): hanya membaca worktree utama repo
    /// dan memanggil `lab_propose`.
    LabSetup,
}

impl TurnKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Implement => "implement",
            Self::Plan => "plan",
            Self::Review => "review",
            Self::Lab => "lab",
            Self::LabSetup => "lab_setup",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "implement" => Some(Self::Implement),
            "plan" => Some(Self::Plan),
            "review" => Some(Self::Review),
            "lab" => Some(Self::Lab),
            "lab_setup" => Some(Self::LabSetup),
            _ => None,
        }
    }
}

/// Hasil keputusan; setiap varian membawa alasan untuk Chat kecuali izin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Deny(String),
    /// Izinkan eksplisit (`permissionDecision: "allow"`).
    Allow,
    /// Tanpa keputusan: Claude Code memakai aturannya sendiri.
    Undecided(String),
}

/// Isi berkas izin satu giliran (KTD-A4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub turn: TurnKind,
    pub allowlist: BashAllowlist,
    pub mcp: Vec<String>,
    pub design_roots: Vec<String>,
    /// Akar run (induk folder worktree anggota).
    pub run_root: String,
    /// Folder worktree anggota run.
    pub members: Vec<String>,
    /// Direktori run daemon VM; `None` di laptop.
    pub vm_run_dir: Option<String>,
    /// Folder milik daemon yang tidak boleh dibaca atau ditulis agent (mis.
    /// folder berkas izin, KTD-A4).
    pub private: Vec<String>,
    /// Folder staging berkas uji giliran lab (KTD-L11).
    pub fixtures: Option<String>,
}

/// Fakta filesystem dari pemanggil (hook atau daemon).
pub trait PathFacts {
    /// `realpath` dari path yang ada; `None` bila tidak ada atau gagal.
    fn canonical(&self, path: &str) -> Option<String>;
    /// Folder terdekat (dirinya atau induknya) yang memuat `.git`.
    fn repo_root(&self, dir: &str) -> Option<String>;
    /// Entri path adalah symlink (`lstat`), termasuk yang menggantung.
    fn is_symlink(&self, path: &str) -> bool;
}

/// Program yang membuat atau memindah path; digabung dengan pindah folder
/// dalam satu baris, fakta path yang diselesaikan sebelum perintah jalan
/// tidak lagi berlaku (KTD-A6).
const CREATORS: &[&str] = &[
    "mkdir", "ln", "mv", "cp", "rsync", "tar", "unzip", "patch", "rm", "rmdir", "install", "touch",
    "dd", "truncate", "chmod", "tee",
];
/// Sub-perintah git yang membuat repo atau mengubah isi worktree (termasuk
/// symlink yang tercatat di repo).
const GIT_CREATORS: &[&str] = &[
    "init",
    "clone",
    "worktree",
    "submodule",
    "checkout",
    "switch",
    "restore",
    "reset",
    "stash",
    "apply",
    "am",
    "merge",
    "pull",
    "rebase",
    "cherry-pick",
    "revert",
    "mv",
    "rm",
];
const PACKAGE_MANAGERS: &[&str] = &["npm", "pnpm", "yarn", "bun", "pip", "pip3", "cargo"];
const INSTALL_WORDS: &[&str] = &["install", "i", "add", "ci"];

const PRIVATE_DENIED: &str = "mandor: the daemon's permission files are not available to the agent";

/// Memutuskan satu tool call (urutan KTD-A8). `review` adalah state pagar
/// reviewer; wajib di giliran reviewer.
#[must_use]
pub fn decide(
    scope: &Scope,
    call: &ToolCall,
    cwd: &str,
    review: Option<&ReviewState>,
    facts: &impl PathFacts,
) -> Decision {
    if let Some(reason) = fences(scope, call, cwd, facts) {
        return Decision::Deny(reason);
    }
    let verdict = match scope.turn {
        TurnKind::Review => review.map_or_else(
            || Verdict::Deny("mandor: review state is missing; tool call denied".to_owned()),
            |state| classify_review(call, state, &scope.mcp),
        ),
        TurnKind::Lab => review.map_or_else(
            || Verdict::Deny("mandor: review state is missing; tool call denied".to_owned()),
            |state| {
                let guard = LabGuard {
                    fixtures: scope.fixtures.as_deref(),
                };
                classify_lab(call, state, &scope.mcp, guard)
            },
        ),
        TurnKind::LabSetup => classify_lab_setup(call, &scope.mcp),
        TurnKind::Plan => classify_planning(call, &scope.mcp),
        TurnKind::Implement => classify_with_mcp(call, &scope.mcp),
    };
    if let Verdict::Deny(reason) = verdict {
        return Decision::Deny(reason);
    }
    if matches!(scope.turn, TurnKind::Lab | TurnKind::LabSetup)
        && let Some(reason) = lab_paths(scope, call, cwd, facts)
    {
        return Decision::Deny(reason);
    }
    if is_write_tool(&call.tool_name) {
        if let Some(reason) =
            resolved_write_paths(call, cwd, &canonical_or(facts, &scope.run_root), facts)
        {
            return Decision::Deny(reason);
        }
    }
    if call.tool_name != "Bash" {
        return Decision::Undecided(format!("`{}` needs approval", call.tool_name));
    }
    let Some(command) = call.command.as_deref() else {
        return Decision::Undecided("Bash call without a command".to_owned());
    };
    let commands = match parse(command) {
        Ok(c) => c,
        Err(e) => return Decision::Undecided(format!("command cannot be fully checked: {}", e.0)),
    };
    if let Some(reason) = commands.iter().find_map(dangerous) {
        return Decision::Undecided(reason);
    }
    if let Some(decision) = PathCheck::new(scope, facts).run(&commands, cwd) {
        return decision;
    }
    if scope.allowlist.allows_all(&commands) {
        Decision::Allow
    } else {
        Decision::Undecided("not in this turn's allowlist".to_owned())
    }
}

/// Path giliran lab setelah diselesaikan (KTD-L10, KTD-L11): baca hanya di
/// akar run atau staging berkas uji, upload hanya berkas di staging yang bukan
/// symlink.
fn lab_paths(scope: &Scope, call: &ToolCall, cwd: &str, facts: &impl PathFacts) -> Option<String> {
    let root = canonical_or(facts, &scope.run_root);
    let staging = scope.fixtures.as_deref().map(|f| canonical_or(facts, f));
    let upload = call.tool_name.ends_with("__browser_file_upload");
    call.paths.iter().find_map(|raw| {
        if upload && facts.is_symlink(raw) {
            return Some(format!(
                "mandor: `{raw}` is a link; upload a regular test file"
            ));
        }
        let Some(path) = resolve(facts, cwd, raw) else {
            return Some(format!("mandor: path `{raw}` cannot be resolved"));
        };
        if let Some(reason) = crate::hook_review::lab_read_violation(&path) {
            return Some(reason);
        }
        let in_staging = staging.as_deref().is_some_and(|s| under(&path, s));
        let ok = if upload {
            in_staging
        } else {
            in_staging || under(&path, &root)
        };
        (!ok).then(|| format!("mandor: `{raw}` is outside this lab's files"))
    })
}

/// Folder milik daemon, pagar VM, dan akar sumber desain, dengan path yang
/// diselesaikan.
fn fences(scope: &Scope, call: &ToolCall, cwd: &str, facts: &impl PathFacts) -> Option<String> {
    let lenient = |p: &str| resolve(facts, cwd, p).unwrap_or_else(|| p.to_owned());
    for dir in &scope.private {
        let real = canonical_or(facts, dir);
        let named = |text: &str| text.contains(dir.trim_end_matches('/')) || text.contains(&real);
        let touched = call
            .paths
            .iter()
            .any(|p| named(p) || under(&lenient(p), &real))
            || call.command.as_deref().is_some_and(named)
            || call.url.as_deref().is_some_and(named);
        if touched {
            return Some(PRIVATE_DENIED.to_owned());
        }
    }
    if let Some(dir) = &scope.vm_run_dir {
        let mut resolved = call.clone();
        resolved.paths = call
            .paths
            .iter()
            .flat_map(|p| [p.clone(), lenient(p)])
            .collect();
        let reason = classify_vm(&resolved, dir)
            .or_else(|| classify_vm(&resolved, &canonical_or(facts, dir)));
        if reason.is_some() {
            return reason;
        }
    }
    let roots: Vec<DesignRoot> = scope
        .design_roots
        .iter()
        .map(|r| DesignRoot {
            given: r.trim_end_matches('/').to_owned(),
            canonical: canonical_or(facts, r),
        })
        .collect();
    if roots.is_empty() {
        return None;
    }
    call.paths
        .iter()
        .find_map(|p| check_design_path(&call.tool_name, p, &lenient(p), &roots))
}

/// Tool penulis berkas: path yang diselesaikan (symlink) ke berkas kredensial
/// atau direktori terlindung ditolak (R11, R16).
fn resolved_write_paths(
    call: &ToolCall,
    cwd: &str,
    run_root: &str,
    facts: &impl PathFacts,
) -> Option<String> {
    call.paths.iter().find_map(|raw| {
        let Some(path) = resolve(facts, cwd, raw) else {
            return Some(format!("mandor: path `{raw}` cannot be resolved"));
        };
        forbidden(&path, run_root).map(|f| f.reason(raw))
    })
}

/// Pelanggaran path terlarang.
enum Forbidden {
    Credential,
    Protected,
}

impl Forbidden {
    fn reason(&self, raw: &str) -> String {
        match self {
            Self::Credential => format!("mandor: access to credential files is denied ({raw})"),
            Self::Protected => {
                format!("mandor: writing agent config or git hooks is denied ({raw})")
            }
        }
    }
}

fn forbidden(path: &str, run_root: &str) -> Option<Forbidden> {
    // Folder kredensial itu sendiri (`~/.ssh`), bukan hanya isinya.
    if is_credential_path(path) || is_credential_path(&format!("{path}/")) {
        return Some(Forbidden::Credential);
    }
    // Segmen di atas akar run (mis. `~/.local/share`) bukan bagian worktree.
    let inside = path
        .strip_prefix(run_root.trim_end_matches('/'))
        .filter(|rest| rest.is_empty() || rest.starts_with('/'))
        .unwrap_or(path);
    in_protected_dir(inside).then_some(Forbidden::Protected)
}

/// Pemeriksaan fakta path untuk satu baris perintah (KTD-A6, KTD-A7).
struct PathCheck<'a, F> {
    facts: &'a F,
    run_root: String,
    members: Vec<String>,
    private: Vec<String>,
}

impl<'a, F: PathFacts> PathCheck<'a, F> {
    fn new(scope: &Scope, facts: &'a F) -> Self {
        Self {
            facts,
            run_root: canonical_or(facts, &scope.run_root),
            members: scope
                .members
                .iter()
                .map(|m| canonical_or(facts, m))
                .collect(),
            private: scope
                .private
                .iter()
                .map(|d| canonical_or(facts, d))
                .collect(),
        }
    }

    /// `Some` bila fakta path memutuskan tolak atau tanpa keputusan.
    fn run(&self, commands: &[SimpleCommand], cwd: &str) -> Option<Decision> {
        // Fakta path diselesaikan sebelum perintah jalan; pembuat path di
        // baris gabungan bisa mengubahnya untuk perintah berikutnya.
        if commands.len() > 1 && commands.iter().any(creates) {
            return Some(Decision::Undecided(
                "creates or moves paths and runs more commands in one line".to_owned(),
            ));
        }
        let Some(mut cur) = self.facts.canonical(cwd) else {
            return Some(Decision::Deny(format!(
                "mandor: working folder `{cwd}` cannot be resolved"
            )));
        };
        if let Some(reason) = self.place(&cur) {
            return Some(Decision::Undecided(reason));
        }
        for command in commands {
            if is_cd(command) {
                let [_, target] = command.words.as_slice() else {
                    return Some(Decision::Undecided(
                        "this `cd` form is not checked".to_owned(),
                    ));
                };
                if target.starts_with('-') {
                    return Some(Decision::Undecided(
                        "this `cd` form is not checked".to_owned(),
                    ));
                }
                match self.folder(&cur, target) {
                    Ok(dir) => cur = dir,
                    Err(decision) => return Some(decision),
                }
                continue;
            }
            let mut base = cur.clone();
            for target in dir_targets(command) {
                match self.folder(&cur, target) {
                    Ok(dir) => base = dir,
                    Err(decision) => return Some(decision),
                }
            }
            if let Some(decision) = self.targets(command, &base) {
                return Some(decision);
            }
        }
        None
    }

    /// Folder tujuan `cd`/`-C`/`--dir`: harus ada, di dalam run, bukan repo bersarang.
    fn folder(&self, cur: &str, target: &str) -> Result<String, Decision> {
        let Some(dir) = self.facts.canonical(&join(cur, target)) else {
            return Err(Decision::Deny(format!(
                "mandor: folder `{target}` cannot be resolved"
            )));
        };
        match self.place(&dir) {
            Some(reason) => Err(Decision::Undecided(reason)),
            None => Ok(dir),
        }
    }

    /// Alasan folder tidak boleh menjadi folder kerja izin otomatis.
    fn place(&self, dir: &str) -> Option<String> {
        if !under(dir, &self.run_root) {
            return Some(format!("`{dir}` is outside this run's worktree"));
        }
        match self.facts.repo_root(dir) {
            Some(repo) if under(&repo, &self.run_root) && !self.members.contains(&repo) => {
                Some(format!("`{repo}` is a nested git repository"))
            }
            _ => None,
        }
    }

    /// Target redirect dan kandidat path (R16).
    fn targets(&self, command: &SimpleCommand, base: &str) -> Option<Decision> {
        for redirect in &command.redirects {
            let Some(path) = resolve(self.facts, base, &redirect.target) else {
                return Some(Decision::Deny(format!(
                    "mandor: redirect target `{}` cannot be resolved",
                    redirect.target
                )));
            };
            if self.private.iter().any(|d| under(&path, d)) {
                return Some(Decision::Deny(PRIVATE_DENIED.to_owned()));
            }
            match forbidden(&path, &self.run_root) {
                Some(Forbidden::Credential) => {
                    return Some(Decision::Deny(
                        Forbidden::Credential.reason(&redirect.target),
                    ));
                }
                Some(Forbidden::Protected) if redirect.write => {
                    return Some(Decision::Deny(
                        Forbidden::Protected.reason(&redirect.target),
                    ));
                }
                Some(Forbidden::Protected) => {
                    return Some(Decision::Undecided(format!(
                        "`{}` is inside agent config or git hooks",
                        redirect.target
                    )));
                }
                None => {}
            }
            // Izin otomatis hanya menulis di dalam worktree run; folder
            // pendamping dan sumber desain di luar run tetap lewat kartu.
            if redirect.write && path != "/dev/null" && !under(&path, &self.run_root) {
                return Some(Decision::Undecided(format!(
                    "`{}` is outside this run's worktree",
                    redirect.target
                )));
            }
        }
        // Gagal-tertutup: selain program baca-saja, setiap kandidat path bisa
        // menjadi target tulis.
        let writes = !read_only(command);
        for raw in path_candidates(command) {
            let Some(path) = resolve(self.facts, base, raw) else {
                return Some(Decision::Undecided(format!(
                    "path `{raw}` cannot be resolved"
                )));
            };
            if self.private.iter().any(|d| under(&path, d)) {
                return Some(Decision::Deny(PRIVATE_DENIED.to_owned()));
            }
            match forbidden(&path, &self.run_root) {
                Some(Forbidden::Credential) => {
                    return Some(Decision::Deny(Forbidden::Credential.reason(raw)));
                }
                Some(Forbidden::Protected) => {
                    return Some(Decision::Undecided(format!(
                        "`{raw}` is inside agent config or git hooks"
                    )));
                }
                None => {}
            }
            if writes && !under(&path, &self.run_root) {
                return Some(Decision::Undecided(format!(
                    "`{raw}` is outside this run's worktree"
                )));
            }
        }
        None
    }
}

fn is_cd(command: &SimpleCommand) -> bool {
    command.words.first().is_some_and(|w| w == "cd")
}

/// Target `git -C <dir>`, `make -C <dir>`, `--dir <dir>`, `--dir=<dir>`.
fn dir_targets(command: &SimpleCommand) -> Vec<&str> {
    let Some((first, args)) = command.words.split_first() else {
        return Vec::new();
    };
    let prog = program_name(first);
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(a) = args.get(i) {
        if a == "--dir" || (a == "-C" && matches!(prog, "git" | "make")) {
            out.extend(args.get(i + 1).map(String::as_str));
            i += 2;
            continue;
        }
        if let Some(v) = a.strip_prefix("--dir=") {
            out.push(v);
        }
        i += 1;
    }
    out
}

/// Program yang hanya membaca path argumennya (argumen berbahaya sudah
/// disaring [`dangerous`]). Program yang bisa menulis ke argumen posisional
/// (mis. `uniq in out`, `file -C`) tidak termasuk.
const READ_ONLY: &[&str] = &[
    "cat", "head", "tail", "wc", "grep", "rg", "ls", "find", "diff", "jq", "sort", "cut", "tr",
    "echo", "printf", "pwd", "test", "which", "basename", "dirname", "awk", "sed", "stat", "du",
    "realpath", "readlink", "true",
];
const GIT_READ_ONLY: &[&str] = &[
    "status",
    "diff",
    "log",
    "show",
    "blame",
    "ls-files",
    "rev-parse",
    "grep",
    "branch",
];

fn read_only(command: &SimpleCommand) -> bool {
    let Some((first, args)) = command.words.split_first() else {
        return true;
    };
    let prog = program_name(first);
    if prog == "git" {
        let mut skip = false;
        let sub = args.iter().find(|a| {
            let take = !skip && !a.starts_with('-');
            skip = matches!(a.as_str(), "-C" | "--git-dir" | "--work-tree");
            take
        });
        return sub.is_some_and(|s| GIT_READ_ONLY.contains(&s.as_str()));
    }
    READ_ONLY.contains(&prog)
}

fn creates(command: &SimpleCommand) -> bool {
    let Some((first, args)) = command.words.split_first() else {
        return false;
    };
    let prog = program_name(first);
    let first_arg = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(String::as_str);
    CREATORS.contains(&prog)
        || (prog == "git" && first_arg.is_some_and(|a| GIT_CREATORS.contains(&a)))
        || (PACKAGE_MANAGERS.contains(&prog)
            && first_arg.is_some_and(|a| INSTALL_WORDS.contains(&a)))
}

/// Kandidat path (KTD-A7): argumen bukan flag, nilai `--flag=value`, dan
/// program yang disebut dengan path.
fn path_candidates(command: &SimpleCommand) -> Vec<&str> {
    let Some((first, args)) = command.words.split_first() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if first.contains('/') {
        out.push(first.as_str());
    }
    for a in args {
        if let Some(flag) = a.strip_prefix("--") {
            if let Some((_, value)) = flag.split_once('=') {
                if !value.is_empty() {
                    out.push(value);
                }
            }
        } else if !a.starts_with('-') && !a.is_empty() {
            out.push(a);
        }
    }
    out
}

/// `path` sama dengan `root` atau di bawahnya, per komponen.
fn under(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn join(base: &str, raw: &str) -> String {
    if raw.starts_with('/') {
        raw.to_owned()
    } else {
        format!("{}/{raw}", base.trim_end_matches('/'))
    }
}

fn canonical_or(facts: &impl PathFacts, path: &str) -> String {
    facts
        .canonical(path)
        .unwrap_or_else(|| path.trim_end_matches('/').to_owned())
}

/// Path sebenarnya, juga untuk berkas yang belum ada: leluhur terdalam yang
/// ada diselesaikan, sisanya ditempel. `..` setelah komponen yang belum ada
/// tidak bisa diselesaikan.
fn resolve(facts: &impl PathFacts, base: &str, raw: &str) -> Option<String> {
    let joined = join(base, raw);
    if let Some(c) = facts.canonical(&joined) {
        return Some(c);
    }
    let parts: Vec<&str> = joined.split('/').collect();
    for i in (1..parts.len()).rev() {
        let prefix = parts[..i].join("/");
        let prefix = if prefix.is_empty() {
            "/".to_owned()
        } else {
            prefix
        };
        let Some(existing) = facts.canonical(&prefix) else {
            continue;
        };
        let rest = &parts[i..];
        // Komponen pertama yang tidak terselesaikan bisa berupa symlink
        // menggantung yang menunjuk ke mana saja.
        let first = format!("{}/{}", existing.trim_end_matches('/'), rest[0]);
        if rest.contains(&"..") || facts.is_symlink(&first) {
            return None;
        }
        let rest: Vec<&str> = rest
            .iter()
            .copied()
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();
        return Some(if rest.is_empty() {
            existing
        } else {
            format!("{}/{}", existing.trim_end_matches('/'), rest.join("/"))
        });
    }
    None
}

/// String kunci "permintaan persis sama" (KTD-A12): nama tool, folder kerja,
/// jenis giliran (`None` untuk kunci penolakan run), dan input kanonis tanpa
/// `description`. Adapter store meng-hash-nya.
#[must_use]
pub fn request_key(tool: &str, input: &Value, cwd: &str, turn: Option<TurnKind>) -> String {
    let mut out = format!(
        "mandor-permit-v1\n{tool}\n{cwd}\n{}\n",
        turn.map_or("*", TurnKind::as_str)
    );
    match input {
        Value::Object(map) => {
            let trimmed: serde_json::Map<String, Value> = map
                .iter()
                .filter(|(k, _)| *k != "description")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            canonical_json(&Value::Object(trimmed), &mut out);
        }
        other => canonical_json(other, &mut out),
    }
    out
}

fn canonical_json(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(k.clone()).to_string());
                out.push(':');
                canonical_json(&map[k], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical_json(v, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use std::collections::{BTreeMap, BTreeSet};

    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    /// Filesystem palsu: path yang ada (dengan hasil `realpath`) dan folder
    /// yang memuat `.git`.
    struct Fs {
        canonical: BTreeMap<String, String>,
        repos: BTreeSet<String>,
        dangling: BTreeSet<String>,
    }

    impl PathFacts for Fs {
        fn canonical(&self, path: &str) -> Option<String> {
            // Seperti `realpath`: setiap komponen harus ada sebelum `..`.
            let mut cur = String::new();
            for p in path.split('/') {
                match p {
                    "" | "." => {}
                    ".." => {
                        cur = cur
                            .rsplit_once('/')
                            .map(|(a, _)| a.to_owned())
                            .unwrap_or_default();
                    }
                    p => cur = self.canonical.get(&format!("{cur}/{p}"))?.clone(),
                }
            }
            if cur.is_empty() {
                Some("/".to_owned())
            } else {
                Some(cur)
            }
        }

        fn is_symlink(&self, path: &str) -> bool {
            self.dangling.contains(path)
        }

        fn repo_root(&self, dir: &str) -> Option<String> {
            let mut cur = dir.to_owned();
            loop {
                if self.repos.contains(&cur) {
                    return Some(cur);
                }
                let (parent, _) = cur.rsplit_once('/')?;
                if parent.is_empty() {
                    return None;
                }
                cur = parent.to_owned();
            }
        }
    }

    fn fs() -> Fs {
        let plain = [
            "/run",
            "/run/kelola-api",
            "/run/kelola-api/src",
            "/run/kelola-api/src/a.ts",
            "/run/kelola-api/fixtures",
            "/run/kelola-api/fixtures/repo",
            "/run/kelola-api/.githooks",
            "/run/kelola-api/.githooks/pre-push",
            "/run/kelola-api/.claude",
            "/run/kelola-api/.claude/settings.json",
            "/run/kelola-api/package.json",
            "/run/kelola-web",
            "/outside",
            "/outside/docs",
            "/home",
            "/home/u",
            "/home/u/.ssh",
            "/home/u/.ssh/id_rsa",
            "/home/u/permit",
            "/dev",
            "/dev/null",
        ];
        let mut canonical: BTreeMap<String, String> = plain
            .iter()
            .map(|p| ((*p).to_owned(), (*p).to_owned()))
            .collect();
        for (link, target) in [
            ("/run/kelola-api/docs", "/outside/docs"),
            ("/run/kelola-api/hooks", "/run/kelola-api/.githooks"),
            ("/run/kelola-api/keys", "/home/u/.ssh"),
            ("/run/kelola-api/pl", "/home/u/permit"),
        ] {
            canonical.insert(link.to_owned(), target.to_owned());
        }
        Fs {
            canonical,
            repos: [
                "/run/kelola-api",
                "/run/kelola-web",
                "/run/kelola-api/fixtures/repo",
            ]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
            dangling: std::iter::once("/run/kelola-api/k".to_owned()).collect(),
        }
    }

    fn scope(turn: TurnKind) -> Scope {
        Scope {
            turn,
            allowlist: BashAllowlist::new(&[
                "Read",
                "Bash(git log*)",
                "Bash(git status*)",
                "Bash(git diff*)",
                "Bash(cd *)",
                "Bash(echo *)",
                "Bash(cat *)",
                "Bash(sed *)",
                "Bash(ls *)",
                "Bash(awk *)",
                "Bash(find *)",
                "Bash(sort *)",
                "Bash(mkdir *)",
                "Bash(ln *)",
                "Bash(tail *)",
                "Bash(npx *)",
                "Bash(curl *)",
            ]),
            mcp: Vec::new(),
            design_roots: Vec::new(),
            run_root: "/run".to_owned(),
            members: vec!["/run/kelola-api".to_owned(), "/run/kelola-web".to_owned()],
            vm_run_dir: None,
            private: vec!["/home/u/permit".to_owned()],
            fixtures: None,
        }
    }

    fn bash(cmd: &str) -> ToolCall {
        ToolCall {
            tool_name: "Bash".to_owned(),
            command: Some(cmd.to_owned()),
            ..ToolCall::default()
        }
    }

    fn run(turn: TurnKind, cwd: &str, cmd: &str) -> Decision {
        let review = ReviewState::default();
        decide(&scope(turn), &bash(cmd), cwd, Some(&review), &fs())
    }

    fn api(cmd: &str) -> Decision {
        run(TurnKind::Implement, "/run/kelola-api", cmd)
    }

    fn denied(d: &Decision) -> bool {
        matches!(d, Decision::Deny(_))
    }

    fn undecided(d: &Decision) -> bool {
        matches!(d, Decision::Undecided(_))
    }

    #[test]
    fn ae1_plan_cd_lalu_git_log_diizinkan_eksplisit() {
        assert_eq!(
            run(
                TurnKind::Plan,
                "/run",
                "cd kelola-api && git log --oneline -5"
            ),
            Decision::Allow
        );
        assert_eq!(api("git log HEAD~3 --stat"), Decision::Allow);
        assert_eq!(api("echo hi > /dev/null 2>&1"), Decision::Allow);
    }

    #[test]
    fn ae2_dan_r11_larangan_keras_ditolak() {
        for cmd in [
            "cd /run/kelola-api && git push origin HEAD",
            "git commit -m x",
            "git remote add o https://x",
            "gh pr list",
            "cat /home/u/.config/gh/hosts.yml",
            "echo x > .git/config",
            "curl -X PUT https://open.larksuite.com/open-apis/bitable/v1/x",
        ] {
            assert!(denied(&api(cmd)), "{cmd}");
        }
        for path in [
            ".git/config",
            ".husky/pre-commit",
            ".githooks/pre-push",
            ".claude/settings.json",
        ] {
            let call = ToolCall {
                tool_name: "Write".to_owned(),
                paths: vec![path.to_owned()],
                ..ToolCall::default()
            };
            let d = decide(
                &scope(TurnKind::Implement),
                &call,
                "/run/kelola-api",
                None,
                &fs(),
            );
            assert!(denied(&d), "{path}");
        }
    }

    #[test]
    fn ae9_ae15_repo_bersarang_dan_symlink_keluar_tanpa_keputusan() {
        assert!(undecided(&api("cd fixtures/repo && git status")));
        assert!(undecided(&api("cd docs && git log")));
        assert!(undecided(&api("git -C fixtures/repo status")));
        assert!(undecided(&run(
            TurnKind::Implement,
            "/run/kelola-api/fixtures/repo",
            "git status"
        )));
        assert!(undecided(&run(
            TurnKind::Implement,
            "/outside",
            "git status"
        )));
    }

    #[test]
    fn folder_tidak_terselesaikan_ditolak() {
        assert!(denied(&api("cd belum-ada && git status")));
        assert!(denied(&api("git -C belum-ada status")));
        assert!(denied(&run(TurnKind::Implement, "/hilang", "git status")));
    }

    #[test]
    fn pindah_folder_bertahap_dan_pembuat_path() {
        assert!(undecided(&api("cd src && cd ../../../outside && ls")));
        assert!(undecided(&api("mkdir x && cd x && git status")));
        assert!(undecided(&api("ln -s /etc l && cd l")));
        assert_eq!(api("mkdir -p src/new"), Decision::Allow);
        assert!(
            undecided(&api("mkdir -p /outside/new")),
            "tulis di luar run"
        );
        assert!(undecided(&api("echo x > /outside/docs/a.md")));
        assert!(undecided(&api("echo x > ../../outside/a")));
        assert_eq!(api("echo x > src/new.txt"), Decision::Allow);
        assert!(
            undecided(&api("npx x --out=/outside/a")),
            "tulis lewat opsi"
        );
        assert!(undecided(&api("cp -t /outside src/a.ts")));
        assert_eq!(
            api("cat /outside/docs/x.md"),
            Decision::Allow,
            "baca di luar run"
        );
    }

    #[test]
    fn ae10_r16_target_terlindung_dan_kredensial() {
        for cmd in [
            "echo x > .githooks/pre-push",
            "sed -i s/a/b/ .claude/settings.json",
            "git diff --output=.githooks/x",
            "echo x > hooks/pre-push",
            "cat keys/id_rsa",
            "echo x > keys/new",
        ] {
            assert!(denied(&api(cmd)), "{cmd}");
        }
        assert!(
            undecided(&api("ls hooks")),
            "baca folder hook lewat symlink"
        );
        assert!(denied(&api("ls /home/u/.ssh")), "folder kredensial");
        assert!(denied(&api("echo x > k")), "symlink menggantung");
        assert!(undecided(&api("cat k")));
        assert!(undecided(&api("ln -s /home/u/.ssh x && cat x/id_rsa")));
        assert!(undecided(&api("mkdir -p a; echo x > a/b")));
        assert!(undecided(&api("git checkout x && cat l/id_rsa")));
        assert!(undecided(&api("git apply p.diff; cat l/id_rsa")));
        assert!(denied(&api("echo x > nope/../x")));
    }

    #[test]
    fn argumen_berbahaya_tanpa_keputusan() {
        for cmd in [
            r#"awk 'BEGIN{system("x")}' f"#,
            "sed 'e ls' f",
            "sed -i s/a/b/ src/a.ts",
            "find . -exec rm {} ;",
            "find . -delete",
            "git -c core.pager=x log",
            "git diff --ext-diff",
            "ls | tee out",
            "sort -o out f",
            "echo $(docker ps)",
            "docker ps",
        ] {
            assert!(undecided(&api(cmd)), "{cmd}");
        }
    }

    #[test]
    fn reviewer_hanya_baca() {
        for cmd in ["echo x > src/a.ts", "sed -i s/a/b/ src/a.ts"] {
            assert!(
                denied(&run(TurnKind::Review, "/run/kelola-api", cmd)),
                "{cmd}"
            );
        }
        assert_eq!(
            run(TurnKind::Review, "/run/kelola-api", "git diff --stat"),
            Decision::Allow
        );
        let d = decide(
            &scope(TurnKind::Review),
            &bash("git status"),
            "/run/kelola-api",
            None,
            &fs(),
        );
        assert!(denied(&d), "tanpa state reviewer");
    }

    #[test]
    fn folder_berkas_izin_terlarang_bagi_agent() {
        assert!(denied(&api("echo x > /home/u/permit/x")));
        assert!(denied(&api("cat /home/u/permit/a.json")));
        assert!(denied(&api("cat pl/a.json")), "lewat symlink");
        let read = ToolCall {
            tool_name: "Read".to_owned(),
            paths: vec!["pl/a.json".to_owned()],
            ..ToolCall::default()
        };
        let d = decide(
            &scope(TurnKind::Implement),
            &read,
            "/run/kelola-api",
            None,
            &fs(),
        );
        assert!(denied(&d));
    }

    #[test]
    fn tool_selain_bash_tanpa_keputusan() {
        let call = ToolCall {
            tool_name: "WebFetch".to_owned(),
            url: Some("https://docs.example.com".to_owned()),
            ..ToolCall::default()
        };
        let d = decide(
            &scope(TurnKind::Implement),
            &call,
            "/run/kelola-api",
            None,
            &fs(),
        );
        assert!(undecided(&d));
    }

    #[test]
    fn pagar_vm_dan_akar_desain_tetap_berlaku() {
        let mut vm = scope(TurnKind::Implement);
        vm.vm_run_dir = Some("/outside".to_owned());
        let call = ToolCall {
            tool_name: "Read".to_owned(),
            paths: vec!["docs/x".to_owned()],
            ..ToolCall::default()
        };
        assert!(denied(&decide(&vm, &call, "/run/kelola-api", None, &fs())));
        let mut design = scope(TurnKind::Implement);
        design.design_roots = vec!["/outside/docs".to_owned()];
        let write = ToolCall {
            tool_name: "Write".to_owned(),
            paths: vec!["docs/y.md".to_owned()],
            ..ToolCall::default()
        };
        assert!(denied(&decide(
            &design,
            &write,
            "/run/kelola-api",
            None,
            &fs()
        )));
    }

    #[test]
    fn kunci_permintaan() {
        let a = json!({"command": "jq . a.json", "description": "lihat"});
        let b = json!({"description": "lain", "command": "jq . a.json"});
        let key = |v: &Value, cwd: &str, t: Option<TurnKind>| request_key("Bash", v, cwd, t);
        assert_eq!(key(&a, "/r", None), key(&b, "/r", None));
        assert_ne!(key(&a, "/r", None), key(&a, "/r/x", None));
        assert_ne!(
            key(&a, "/r", Some(TurnKind::Implement)),
            key(&a, "/r", Some(TurnKind::Review))
        );
        assert_ne!(
            key(&a, "/r", None),
            key(&json!({"command": "jq .name a.json"}), "/r", None)
        );
        assert_eq!(
            request_key("X", &json!({"b": {"y": 1, "x": [2]}, "a": 0}), "/", None),
            request_key("X", &json!({"a": 0, "b": {"x": [2], "y": 1}}), "/", None)
        );
    }

    /// Plan 2026-11-10-001, KTD-L10/KTD-L11: giliran lab membaca hanya di akar
    /// run atau staging; upload hanya dari staging dan bukan symlink.
    #[test]
    fn giliran_lab_membatasi_baca_dan_upload() {
        let mut lab = scope(TurnKind::Lab);
        lab.fixtures = Some("/outside".to_owned());
        lab.mcp = vec!["mandor".to_owned()];
        let state = ReviewState {
            origins: vec!["http://localhost:5173".to_owned()],
            ..ReviewState::default()
        };
        let read = |p: &str| ToolCall {
            tool_name: "Read".to_owned(),
            paths: vec![p.to_owned()],
            ..ToolCall::default()
        };
        let decide_lab = |c: &ToolCall| decide(&lab, c, "/run/kelola-api", Some(&state), &fs());
        assert!(!matches!(decide_lab(&read("src/a.ts")), Decision::Deny(_)));
        assert!(!matches!(
            decide_lab(&read("/outside/docs")),
            Decision::Deny(_)
        ));
        // Symlink keluar dari akar run ditolak setelah diselesaikan.
        assert!(matches!(
            decide_lab(&read("keys/id_rsa")),
            Decision::Deny(_)
        ));
        assert!(matches!(
            decide_lab(&read("/home/u/permit")),
            Decision::Deny(_)
        ));
        let bash = ToolCall {
            tool_name: "Bash".to_owned(),
            command: Some("git status".to_owned()),
            ..ToolCall::default()
        };
        assert!(matches!(decide_lab(&bash), Decision::Deny(_)));
        // Tanpa state review, giliran lab ditolak.
        assert!(matches!(
            decide(&lab, &read("src/a.ts"), "/run/kelola-api", None, &fs()),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn upload_lab_dari_symlink_ditolak() {
        let mut lab = scope(TurnKind::Lab);
        lab.fixtures = Some("/run/kelola-api".to_owned());
        let mut state = ReviewState {
            origins: vec!["http://localhost:5173".to_owned()],
            ..ReviewState::default()
        };
        state.pages.insert(
            "playwright_admin".to_owned(),
            crate::hook_review::TabState {
                url: Some("http://localhost:5173/".to_owned()),
                settled: true,
            },
        );
        let upload = |p: &str| ToolCall {
            tool_name: "mcp__playwright_admin__browser_file_upload".to_owned(),
            paths: vec![p.to_owned()],
            ..ToolCall::default()
        };
        let d = |c: &ToolCall| decide(&lab, c, "/run/kelola-api", Some(&state), &fs());
        assert!(!matches!(
            d(&upload("/run/kelola-api/package.json")),
            Decision::Deny(_)
        ));
        assert!(
            matches!(d(&upload("/run/kelola-api/k")), Decision::Deny(_)),
            "symlink"
        );
        assert!(
            matches!(d(&upload("/run/kelola-api/keys")), Decision::Deny(_)),
            "keluar staging setelah diselesaikan"
        );
    }

    #[test]
    fn jenis_giliran_lab_bolak_balik() {
        assert_eq!(TurnKind::parse("lab"), Some(TurnKind::Lab));
        assert_eq!(TurnKind::Lab.as_str(), "lab");
        assert_eq!(TurnKind::parse("lab_setup"), Some(TurnKind::LabSetup));
        assert_eq!(TurnKind::LabSetup.as_str(), "lab_setup");
    }

    /// KTD-L14: pengusul perlengkapan hanya membaca worktree utama repo tanpa
    /// berkas env, dan hanya memanggil `lab_propose` di server ledger.
    #[test]
    fn giliran_pengusul_lab_hanya_baca() {
        let mut setup = scope(TurnKind::LabSetup);
        setup.mcp = vec!["mandor".to_owned()];
        let call = |tool: &str, path: Option<&str>| ToolCall {
            tool_name: tool.to_owned(),
            paths: path.map(|p| vec![p.to_owned()]).unwrap_or_default(),
            ..ToolCall::default()
        };
        let d = |c: &ToolCall| decide(&setup, c, "/run/kelola-api", None, &fs());
        let denied = |c: &ToolCall| matches!(d(c), Decision::Deny(_));
        assert!(!denied(&call("Read", Some("README.md"))));
        assert!(!denied(&call("Read", Some(".env.example"))));
        assert!(!denied(&call("mcp__mandor__lab_propose", None)));
        // Grep tanpa pola berkas bisa membaca dotfile.
        assert!(denied(&call("Grep", None)));
        assert!(denied(&call("Read", Some(".env"))));
        assert!(denied(&call("Read", Some("config/.env.local"))));
        assert!(denied(&call(
            "Read",
            Some("/home/u/.mandor/lab/kelola/private/secrets.json")
        )));
        assert!(
            denied(&call("Read", Some("keys/id_rsa"))),
            "symlink keluar worktree"
        );
        assert!(denied(&call("Write", Some("lab.json"))));
        assert!(denied(&call("WebFetch", None)));
        assert!(denied(&call("mcp__playwright__browser_navigate", None)));
        assert!(denied(&call("mcp__claude-in-chrome__navigate", None)));
        assert!(denied(&call("mcp__lain__alat", None)));
        let search = |tool: &str, glob: &str| ToolCall {
            tool_name: tool.to_owned(),
            globs: vec![glob.to_owned()],
            ..ToolCall::default()
        };
        for g in [
            "*.ts",
            "**/*.tsx",
            "src/**/*.sql",
            "**/.env.example",
            "README.md",
            "db/seed*.ts",
            "**/*.test.ts",
        ] {
            assert!(!denied(&search("Grep", g)), "Grep {g}");
            assert!(!denied(&search("Glob", g)), "Glob {g}");
        }
        for g in [
            "**/.env",
            ".env*",
            "**/*env*",
            "config/.env.local",
            "**/id_rsa",
            "*.pem",
            "**/secrets.json",
            ".[e]nv",
            "?env",
            "**/.e*v",
            "*",
            "**/*",
            "{.env,a}",
            "!*.ts",
            ".git/config",
            "../x/*.ts",
            "**/*.key",
            "*.env",
            "**/*.local",
            "**/*.production",
            "*.ts,.env",
            "*.ts .env",
            "**/*.rust",
        ] {
            assert!(denied(&search("Grep", g)), "Grep {g}");
            assert!(denied(&search("Glob", g)), "Glob {g}");
        }
        let bash = ToolCall {
            tool_name: "Bash".to_owned(),
            command: Some("cat README.md".to_owned()),
            ..ToolCall::default()
        };
        assert!(denied(&bash));
    }
}
