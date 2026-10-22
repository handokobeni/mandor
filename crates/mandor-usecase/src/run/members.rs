//! Anggota run: repo utama, ditambah repo lain pada run workspace multi-repo.
//!
//! Semua alur yang menyentuh worktree (permintaan agent, publish, tutup run)
//! memakai daftar anggota yang sama, sehingga run satu repo hanyalah kasus
//! dengan satu anggota.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{RunError, RunService};
use crate::ports::Deps;
use mandor_domain::Task;
use mandor_domain::push_gate::pr_summary;

use crate::hash::fnv1a_hex;

use crate::run_ports::{
    AgentPort, AgentRequest, AgentWorkspace, ChatItem, GitPort, LedgerStore, PrContext, RepoInfo,
    Repos, RunPhase, RunRecord, RunStore, SessionRef, WorkChanges,
};

/// Batas diff untuk pembuat pesan commit.
const COMMIT_DIFF_MAX: usize = 50_000;
/// Judul PR: `KEY: judul task`.
fn pr_title(task: &Task) -> String {
    match &task.key {
        Some(k) => format!("{k}: {}", task.title),
        None => task.title.clone(),
    }
}

/// GitHub dapat mengembalikan badan dengan `\r\n`; bandingkan setelah dirapikan.
fn normalize_body(body: &str) -> String {
    body.replace("\r\n", "\n").trim().to_owned()
}

/// Badan template sebelum deskripsi otomatis ada: `Lark task: KEY`, baris kosong,
/// lalu penanda Mandor.
fn is_legacy_body(body: &str) -> bool {
    let lines: Vec<&str> = body.lines().collect();
    matches!(
        lines.as_slice(),
        [task, "", "Created by mandor." | "Created by Mandor."] if task.starts_with("Lark task: ")
    )
}

/// Batas diff branch yang dikirim ke pembuat deskripsi PR.
const PR_DIFF_MAX: usize = 60_000;
/// Batas deskripsi buatan model; GitHub menolak badan PR di atas 65.536 karakter.
const PR_BODY_MAX: usize = 20_000;

/// Satu repo dalam run beserta worktree dan link PR-nya.
#[derive(Debug, Clone)]
pub(crate) struct Member {
    pub repo: RepoInfo,
    pub worktree: PathBuf,
    pub pr_url: Option<String>,
}

/// Nama folder run workspace: id workspace dan branch, aman sebagai satu segmen path.
pub(crate) fn group_name(workspace_id: &str, branch: &str) -> String {
    format!("{workspace_id}-{branch}").replace(['/', '\\'], "-")
}

/// Sesi baru atau lanjutan sesuai riwayat run.
pub(crate) fn session_of(run: &RunRecord) -> SessionRef {
    let id = run.session_id.clone().unwrap_or_default();
    if run.session_started {
        SessionRef::Resume(id)
    } else {
        SessionRef::New(id)
    }
}

fn group_of(run: &RunRecord) -> Option<String> {
    run.workspace
        .as_ref()
        .and_then(|w| w.root.file_name())
        .map(|n| n.to_string_lossy().into_owned())
}

/// Menyimpan link PR anggota ke-`index` (0 = repo utama).
fn set_pr_url(run: &mut RunRecord, index: usize, url: &str) {
    if index == 0 {
        run.pr_url = Some(url.to_owned());
    } else if let Some(m) = run.extra.get_mut(index - 1) {
        m.pr_url = Some(url.to_owned());
    }
}

impl<D: Deps> RunService<D> {
    /// Anggota run, repo utama pertama.
    pub(crate) async fn members(&self, run: &RunRecord) -> Result<Vec<Member>, RunError> {
        let worktree = run
            .worktree
            .clone()
            .ok_or(RunError::InvalidState("no worktree"))?;
        let mut out = vec![Member {
            repo: self.repo_of(run).await?,
            worktree,
            pr_url: run.pr_url.clone(),
        }];
        for m in &run.extra {
            let repo = self
                .repos
                .repo(&m.repo_id)
                .await?
                .ok_or(RunError::NotFound("repo"))?;
            out.push(Member {
                repo,
                worktree: m.worktree.clone(),
                pr_url: m.pr_url.clone(),
            });
        }
        Ok(out)
    }

    /// Membuat ulang worktree yang hilang dari branch task, di folder run yang sama.
    async fn restore_missing(&self, run: &mut RunRecord) -> Result<(), RunError> {
        let branch = run
            .branch
            .clone()
            .ok_or(RunError::InvalidState("no branch"))?;
        let group = group_of(run);
        let primary = match &run.worktree {
            Some(w) if self.git.worktree_exists(w).await => w.clone(),
            _ => {
                let repo = self.repo_of(run).await?;
                self.git
                    .restore_worktree(&repo, &branch, group.as_deref())
                    .await?
            }
        };
        run.worktree = Some(primary);
        for i in 0..run.extra.len() {
            if self.git.worktree_exists(&run.extra[i].worktree).await {
                continue;
            }
            let repo = self
                .repos
                .repo(&run.extra[i].repo_id)
                .await?
                .ok_or(RunError::NotFound("repo"))?;
            run.extra[i].worktree = self
                .git
                .restore_worktree(&repo, &branch, group.as_deref())
                .await?;
        }
        Ok(())
    }

    /// Permintaan giliran agent untuk `run`; worktree yang hilang dibuat ulang
    /// dan disimpan di `run`. Run workspace bekerja di folder run dengan
    /// konteks semua repo dan MCP server pilihan dev.
    pub(crate) async fn agent_request(
        &self,
        run: &mut RunRecord,
        session: SessionRef,
        message: String,
    ) -> Result<AgentRequest, RunError> {
        self.restore_missing(run).await?;
        let branch = run
            .branch
            .clone()
            .ok_or(RunError::InvalidState("no branch"))?;
        let members = self.members(run).await?;
        let primary = members[0].clone();
        let (worktree, workspace) = match &run.workspace {
            None => (primary.worktree, None),
            Some(ws) => {
                // Workspace yang dihapus saat run berjalan: lanjut tanpa MCP;
                // `CLAUDE.md` sudah ada di folder run.
                let (source, mcp_servers) = match self.repos.workspace(&ws.id).await? {
                    Some(info) => (info.root, info.mcp_servers),
                    None => (ws.root.clone(), Vec::new()),
                };
                let ctx = AgentWorkspace {
                    source,
                    repos: members.into_iter().map(|m| (m.repo, m.worktree)).collect(),
                    mcp_servers,
                };
                (ws.root.clone(), Some(ctx))
            }
        };
        let plan_mode = run.phase != RunPhase::Implement;
        // Giliran brainstorm/plan mengisi kolom Design lewat tool ledger bila
        // ada sumber desain; tanpa sumber, Design tidak diisi (U6).
        let ledger = (plan_mode && !run.design.is_empty()).then(|| crate::review::LedgerAccess {
            role: crate::review::CapabilityRole::Design,
            token: self.issue_capability(&run.id, crate::review::CapabilityRole::Design),
        });
        Ok(AgentRequest {
            repo: primary.repo,
            worktree,
            branch,
            session,
            message,
            workspace,
            plan_mode,
            ledger,
            design: run.design.clone(),
            review: None,
        })
    }

    /// Perubahan setiap anggota run.
    pub(crate) async fn member_changes(
        &self,
        run: &RunRecord,
    ) -> Result<Vec<(Member, WorkChanges)>, RunError> {
        let branch = run
            .branch
            .clone()
            .ok_or(RunError::InvalidState("no branch"))?;
        let mut out = Vec::new();
        for m in self.members(run).await? {
            let changes = self.git.changes(&m.repo, &m.worktree, &branch).await?;
            out.push((m, changes));
        }
        Ok(out)
    }

    /// Commit, push, dan PR untuk setiap anggota yang berubah. Link PR disimpan
    /// per anggota segera setelah ada, agar kegagalan sebagian bisa dilanjutkan.
    /// Mengembalikan semua link PR, satu per baris.
    ///
    /// Repo run workspace yang tidak berubah dan tidak punya commit di branch
    /// task dilewati. `NoChanges` bila tidak ada yang di-push dan tidak ada PR baru.
    pub(crate) async fn publish_members(
        self: &Arc<Self>,
        run: &mut RunRecord,
        work: &[(Member, WorkChanges)],
    ) -> Result<String, RunError> {
        let branch = run
            .branch
            .clone()
            .ok_or(RunError::InvalidState("no branch"))?;
        let multi = run.workspace.is_some();
        let mut urls = Vec::new();
        let mut progressed = false;
        for (index, (m, changes)) in work.iter().enumerate() {
            if !changes.is_empty() {
                self.commit_and_push(&run.id, m, changes, &branch).await?;
                progressed = true;
            }
            let existing = match &m.pr_url {
                Some(u) => Some(u.clone()),
                None => self.git.find_pr(&m.repo, &branch).await?.map(|pr| pr.url),
            };
            let url = if let Some(u) = existing {
                if !changes.is_empty() && self.refresh_pr_body(run, m, &u).await {
                    self.runs.save(run, run.generation, self.now()).await?;
                }
                Some(u)
            } else if !multi || !changes.is_empty() || self.has_commits(m).await? {
                Some(self.create_pr(run, m, &branch).await?)
            } else {
                None
            };
            let Some(url) = url else { continue };
            if m.pr_url.as_deref() != Some(url.as_str()) {
                progressed = true;
                set_pr_url(run, index, &url);
                self.runs.save(run, run.generation, self.now()).await?;
            }
            urls.push(url);
        }
        if !progressed || urls.is_empty() {
            return Err(RunError::NoChanges);
        }
        Ok(urls.join("\n"))
    }

    async fn commit_and_push(
        &self,
        run_id: &str,
        m: &Member,
        changes: &WorkChanges,
        branch: &str,
    ) -> Result<(), RunError> {
        if changes.dirty {
            let diff: String = changes.diff.chars().take(COMMIT_DIFF_MAX).collect();
            let message = self.agent.commit_message(&m.worktree, &diff).await?;
            self.git.commit_all(&m.worktree, &message).await?;
            self.runs
                .append(
                    run_id,
                    &ChatItem::System(format!("Commit: {message}")),
                    self.now(),
                )
                .await?;
        }
        self.git.push(&m.repo, &m.worktree, branch).await?;
        Ok(())
    }

    /// Branch task punya commit terhadap base (mis. sudah di-push tapi PR gagal dibuat).
    async fn has_commits(&self, m: &Member) -> Result<bool, RunError> {
        Ok(!self
            .git
            .branch_diff(&m.repo, &m.worktree)
            .await?
            .trim()
            .is_empty())
    }

    async fn create_pr(
        &self,
        run: &mut RunRecord,
        m: &Member,
        branch: &str,
    ) -> Result<String, RunError> {
        let task = self.task_of(&run.record_id).await?;
        let title = pr_title(&task);
        let body = self.pr_body(run, m, &task, &title).await;
        let url = self
            .git
            .create_pr(&m.repo, &m.worktree, branch, &title, &body)
            .await?;
        run.pr_body_hashes
            .insert(url.clone(), fnv1a_hex(&normalize_body(&body)));
        Ok(url)
    }

    /// Menulis ulang badan PR dari diff terbaru setelah push perbaikan, hanya bila
    /// badan saat ini masih tulisan Mandor. Kegagalan hanya dicatat; push dan
    /// status tidak bergantung padanya. `true` bila badan diperbarui (hash baru
    /// perlu disimpan).
    async fn refresh_pr_body(&self, run: &mut RunRecord, m: &Member, url: &str) -> bool {
        if !self.pr_body_owned(run, m, url).await {
            return false;
        }
        let task = match self.task_of(&run.record_id).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(url, error = %e, "task tidak terbaca; badan PR tidak diperbarui");
                return false;
            }
        };
        let body = self.pr_body(run, m, &task, &pr_title(&task)).await;
        if let Err(e) = self.git.set_pr_body(&m.repo, &m.worktree, url, &body).await {
            tracing::warn!(url, error = %e, "badan PR gagal diperbarui");
            return false;
        }
        run.pr_body_hashes
            .insert(url.to_owned(), fnv1a_hex(&normalize_body(&body)));
        true
    }

    /// Badan PR masih tulisan Mandor: hash sama dengan yang terakhir ditulis,
    /// atau masih badan template lama. Badan yang tidak terbaca dianggap bukan.
    async fn pr_body_owned(&self, run: &RunRecord, m: &Member, url: &str) -> bool {
        let current = match self.git.pr_body(&m.repo, url).await {
            Ok(Some(b)) => normalize_body(&b),
            Ok(None) => return false,
            Err(e) => {
                tracing::warn!(url, error = %e, "badan PR tidak terbaca; tidak diperbarui");
                return false;
            }
        };
        let owned =
            run.pr_body_hashes.get(url) == Some(&fnv1a_hex(&current)) || is_legacy_body(&current);
        if !owned {
            tracing::info!(url, "badan PR sudah disunting; tidak ditimpa");
        }
        owned
    }

    /// Badan PR: deskripsi buatan model dari diff branch dan konteks task, diapit
    /// tautan task dan penanda Mandor. Bila model atau bahan gagal, PR tetap
    /// dibuat dengan badan minimal.
    async fn pr_body(&self, run: &RunRecord, m: &Member, task: &Task, title: &str) -> String {
        let key = task.key.as_deref().unwrap_or(&task.record_id);
        let header = format!("Lark task: {key}");
        let footer = "Created by Mandor.";
        let generated = async {
            let diff: String = self
                .git
                .branch_diff(&m.repo, &m.worktree)
                .await?
                .chars()
                .take(PR_DIFF_MAX)
                .collect();
            let acs = self.ac_texts(task).await.unwrap_or_default();
            let ctx = PrContext {
                title,
                description: &task.description,
                acs: &acs,
                plan: run.plan.as_deref(),
                diff: &diff,
            };
            self.agent.pr_description(&m.worktree, &ctx).await
        }
        .await;
        // Ringkasan per AC (R31): label, status, alasan, dan pemutus saja (R33).
        let summary = match self.reviews.ac_rows(&run.id).await {
            Ok(rows) if !rows.is_empty() => {
                let at = self
                    .synced_at(&run.id)
                    .map(crate::review::settings::utc_minutes);
                format!("\n\n{}", pr_summary(&rows, at.as_deref()).trim_end())
            }
            Ok(_) => String::new(),
            Err(e) => {
                tracing::warn!(error = %e, "buku AC tidak terbaca; PR tanpa ringkasan AC");
                String::new()
            }
        };
        match generated {
            Ok(text) if !text.trim().is_empty() => {
                let text: String = text.trim().chars().take(PR_BODY_MAX).collect();
                format!("{header}\n\n{text}{summary}\n\n---\n{footer}")
            }
            Ok(_) => format!("{header}{summary}\n\n{footer}"),
            Err(e) => {
                tracing::warn!(error = %e, "deskripsi PR otomatis gagal; memakai badan minimal");
                format!("{header}{summary}\n\n{footer}")
            }
        }
    }

    /// Menghapus worktree anggota (rollback Start atau tutup run), lalu folder
    /// run bila semuanya terhapus.
    pub(crate) async fn discard_members(
        &self,
        members: &[(RepoInfo, PathBuf)],
        branch: &str,
        root: Option<&Path>,
    ) {
        let mut all = true;
        for (repo, wt) in members {
            if let Err(e) = self.git.discard_worktree(repo, wt, branch).await {
                all = false;
                tracing::warn!(branch, repo = %repo.id, error = %e, "worktree tidak terhapus");
            }
        }
        if let Some(root) = root.filter(|_| all) {
            self.git.remove_run_root(root).await;
        }
    }
}
