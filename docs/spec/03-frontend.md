# Korlap Frontend — Reconstruction Spec

Scope: `src/` (SvelteKit SPA, Svelte 5 runes) plus tests and config. Paths are relative to `/home/handokobeni/Work/korlap-clone/korlap`.

## 0. Build / runtime shell
- **`package.json`**
  - Stack: Svelte 5, SvelteKit 2, Vite 6, adapter-static.
  - Dependencies: CodeMirror 6 (13 language packs), @xterm/xterm 6 + addon-fit, marked 17 + marked-highlight, highlight.js 11 (core + 20 languages), dompurify, lucide-svelte, devicon, @tauri-apps/api v2, plugin-dialog, plugin-opener.
  - `zod` and the MCP SDK are listed but unused in `src/`.
- **`svelte.config.js`**: `adapter-static({fallback:"index.html"})`.
- **`vite.config.js`**: port 1420 with strictPort, HMR on 1421 when `TAURI_DEV_HOST` is set, `src-tauri` ignored by the watcher.
- **`+layout.ts:5`**: `ssr=false`.
- **`+layout.svelte`**: imports `app.css`, calls `initTheme()`, and suppresses the benign "ResizeObserver loop" error (`:16-24`).
- **`app.html`**: `color-scheme: light dark`, title "korlap".
- **`playwright.config.ts`**: tests on `localhost:1420`, starts the dev server with `bun run dev`, chromium only.

## 1. Components

### 1.1 Tree
```
+layout → +page (ALL state)
 ├ [no activeRepo] Home screen (+page:2329-2531)
 ├ .app
 │  ├ TitleBar (Dropdown repo switcher)
 │  ├ ReviewAlertBar
 │  ├ .mode-stack: two absolute layers (visibility + inert, never unmounted)
 │  │  ├ work: Sidebar(+ResizeHandle) | WorkspacePanel
 │  │  │     ├ action bar
 │  │  │     ├ DiffViewer / FileBrowser(→CodeEditor, SearchModal)   [display toggled]
 │  │  │     ├ terminal pane → TerminalView × (workspace × tab)       [all mounted]
 │  │  │     ├ ReviewPill (draggable)
 │  │  │     └ ChatPanel per workspace (active = floating overlay)
 │  │  │          → VirtualScroller, MentionInput, MentionAutocomplete,
 │  │  │            AskUserQuestion, EditDiffBlock, TodoListBlock
 │  │  └ plan: sub-layers kanban (KanbanBoard) | files (FileBrowser, repo scope)
 │  │        | terminal (tab strip + repo TerminalViews)
 │  ├ DependencyGraph, SearchModal, RepoSettings (fixed full-screen, z-index 100)
 │  └ status bar with LSP pills
 └ Toasts
```
KanbanBoard contains: KanbanColumn ×4 → KanbanCard, AutopilotPill (fixed bottom-right), TaskPopover, ManualCheckoutPopover, PrCheckoutPopover, ComboPrCheckoutPopover, JiraImportPopover, CardDetailOverlay, and the Done ⋯ and More ⋯ menus.

Dead code: `VirtualList.svelte`, `AgentActivityBar.svelte`, `splitTextWithMentions` (`chat-utils.ts:64`).

### 1.2 Component reference
**TitleBar** (`TitleBar.svelte:11-32`)
- Props: repos, activeRepo, inWorkspace, workspaceTitle, onGoToPlan, onSelectRepo, onSettings, onGoHome, highlightedRepoIndex, onDropdownClose, autopilotEnabled, onAutopilotToggle, autopilotStatus, onShowDepGraph, planView, onPlanViewChange, repoSettings.
- Layout: 40px tall, 78px left padding for the macOS traffic lights; purple background in dev mode.
- Window: drag via `startDragging` (`:102-115`), double-click maximizes.
- Left side:
  - Home button.
  - Repo dropdown: entries numbered 1-9 plus "Add repository".
  - Settings button.
  - Breadcrumb: Board + task title in work mode, Kanban/Files/Terminal buttons in plan mode.
  - Repo-level run-script split button with output popover (`:138-264`).
- Right side:
  - Autopilot status text.
  - Branch warning when not on the default branch ("Switch to main" → `checkoutDefaultBranch`).
  - "Sync (N behind)" → `syncMain`.
  - Autopilot toggle and dependency-graph button.
- HEAD branch and behind-count are re-checked by an effect in plan mode (`:47-65`).
- Exports: `toggleRepoDropdown`, `isRepoDropdownOpen`, `closeRepoDropdown`, `triggerRun`.

**Dropdown**
- Props: trigger and children snippets, onclose, tooltipOpts.
- Closes on an outside mousedown. Exports `close`, `toggle`, `isOpen`.

**Sidebar** (`:7-23`)
- Width resizable 140–400px (default 220).
- Groups: In Progress, Review, Done (Done collapsed by default).
- Status dots: creating, running, waiting, pr-open, pr-fail, pr-merge. Eye icon while reviewing, spinner while running.
- Double-click renames (calls `renameBranch`); ⋯ menu offers Remove.
- Staging entry pinned at the bottom.

**WorkspacePanel** (`:20-120`)
- About 45 props. Bindable: activeTab, fileNavigatePath, fileNavigateLine, terminalPaneVisible.
- Action bar with Diff/Files tabs; content panes.
- Terminal pane 200–800px (default 400).
- Floating ReviewPill.
- Chat overlay:
  - Draggable by its header, resizable on all 8 edges, minimum 380×280 (`:698-756`).
  - Collapses to a pill showing the live agent task.
- Owns the per-workspace script runner (`:171-387`) and terminal tabs (`:236-306`).
- `PanelTab = "diff" | "files"`.

**ChatPanel** (`:27-50`)
- Props: workspaceId, creating, planMode, thinkingMode, model, queue, contextWarning, providerInfo, onSend, onSendImmediate, onStop, onSendNow, onRemoveFromQueue, onPlanModeChange, onThinkingModeChange, onModelChange, onExecutePlan, onMentionClick, onReady, onProviderSwitch.
- Reads messages directly from the global store.
- Exposes `ChatPanelApi {addMention, insertText, refreshSuggestions, focusInput}` via `onReady`.

**VirtualScroller**
- Props: count, estimatedHeight (60), gap, overscan (5), children(index), stickToBottom, onscrolledUp.
- Heights measured by ResizeObserver; prefix offsets with binary search.
- Scroll anchoring (`:128-152`); auto-scroll only when the count grows while at the bottom.

**MentionInput**
- contenteditable div with chip spans (`data-mention-type`, `data-mention-path`).
- Detects `@` preceded by whitespace (`:273-328`).
- API: insertMention, appendMention, insertText, restoreContent, focus, submit, getValue.
- Enter submits unless an @-trigger is active or `multiline` is set.
- Backspace deletes a whole chip. Paste is plain text; images are passed to the parent.

**MentionAutocomplete**: fixed popup above the anchor element; up/down wrap around; selects on mousedown.

**AskUserQuestion**
- Parses `chunk.input` as a JSON array of `{header, question, options[{label, description}], multiSelect}`.
- Single-select records the answer on click; multi-select toggles, then "Submit (n)". "Other" opens free text.
- One question auto-submits; several submit as "1. …\n2. …".
- Answered state shows a summary with a "Change" button.
- State is held in parent-owned SvelteMaps.

**EditDiffBlock**: renders old lines as − and new lines as + (not a real diff). Expanded by default.

**TodoListBlock**: TodoWrite items `{content, status, priority}`; progress bar; the latest block is always open.

**DiffViewer, FileBrowser, CodeEditor, Terminal**: see §6.

**ReviewPill**
- `ReviewState {status: running|complete, currentTask, resultMarkdown}`.
- Results classify as:
  - clean: starts with "[CLEAN]" or contains "no issues found";
  - failed: starts with "**Review failed:**";
  - issues: markdown card with a "Send to Chat" button.

**KanbanBoard** (`:31-99`)
- Four columns, keyboard focus navigation, owns all popovers.
- Exports `openNewTask`.

**KanbanCard**
- Todo card:
  - Title, description, Thinking/Plan/model badges, image thumbnails (`convertFileSrc`).
  - Buttons: Ready, Start, Edit, ✕.
- Workspace card:
  - Status dot, title, minutes elapsed since `created_at` (updated every 30s), trash button.
  - Description, branch, +/− counts, "reviewing" badge.

**TaskPopover**: props include initial* fields, providerInfo, submitLabel, onSubmit, onSubmitAndStart, onCancel. Emits `TaskData {title, description, newImages, existingPaths, mentions, planMode, thinkingMode, model, provider}`.

**Other popovers and overlays**
- ManualCheckoutPopover: `{branchName, description}`.
- PrCheckoutPopover: search list of open PRs.
- ComboPrCheckoutPopover: pick 2 or more PRs.
- JiraImportPopover: imports as `[KEY] summary`.
- CardDetailOverlay: workspace details.
- AutopilotPill: collapsed shows the last event plus status; expanded shows an event log and a "Direct the orchestrator…" input.
- DependencyGraph:
  - Kahn-rank SVG layout: nodes 180×44, horizontal gap 80, vertical gap 24.
  - Statuses: ready, blocked (dashed), in-progress (pulse), review, done.
- SearchModal: props workspaceId or repoId, onAddToContext, onOpenInFiles, enterLabel.
- ReviewAlertBar: "● branch — Opus finished review. +N more [Review now]".
- Toasts: newest first, each with Copy and Dismiss.
- ResizeHandle: pointer-captured, reports deltas.

### 1.3 Home screen (`+page:2329-2531`)
```
┌ max 440px ─────────────────────────────────────────┐
│ GITHUB  [● alice][bob][+]                            │
│   or "gh not installed — brew install gh [Retry]"    │
│   or [Connect GitHub Account] (shows device code)    │
│ REPOSITORIES [Search local or alice's GitHub repos]  │
│  my-app  alice  ×   /path                            │
│  ── from GitHub ── (400ms debounce, click = clone)   │
│ [Create "foo" on GitHub] → name / desc /             │
│      Private|Public / ☑README / Create & Open        │
│ [Add local repo as alice] (folder picker + access)   │
└──────────────────────────────────────────────────────┘
```

### 1.4 Plan mode
```
┌TitleBar: ●●●[‹][repo▾][⚙][▦|📁|>_][▶Run main|▾]   2/4 agents·3 queued [⟳Sync(3 behind)][⚡Autopilot][⧉]┐
├ReviewAlertBar (optional) ────────────────────────────────────────────────────────────────┤
│┌Todo 3───────┐┌In Progress 2┐┌Review 1 (accent)┐┌Done 4 (dimmed) ⋯┐                    │
││Title        ││● Title 5m 🗑 ││● Title 🗑        ││● Title 🗑        │                    │
││desc         ││ description ││ feat/b +3 −1    ││                 │                    │
││[💡][📖][Opus]││ feat/a +12−4││ [reviewing]     ││                 │                    │
││[img]        │└─────────────┘└─────────────────┘└─────────────────┘                    │
││○Ready ▶Start ✎ ✕│                                                                       │
││[+ New task][⋯]│                         ┌⚡ last event · 2 agents·3 queued ⌃┐ (fixed)   │
├status bar: [◌ rust][● typescript] ───────────────────────────────────────────────────────┤
```
- Todo ⋯ menu: Manual checkout, Review PR, Combo PRs, Import from Jira. Done ⋯ menu: Remove all.
- The Files view is a full-page FileBrowser with repo scope.
- The Terminal view is a tab strip `[Terminal 1 ×][+]` over stacked repo terminals (`+page:2707-2742`).

### 1.5 Work mode
```
┌TitleBar [‹][repo▾][⚙][▦ | Task title]                                         [⚡]┐
├Sidebar──┬[Diff +12−4][Files] feat/a › main [▶Run|▾][>_] [↓Update 3][↗][👁Review][⬆Push & create PR]┤
│▾In Prog │ files 240px │ unified diff        [↗]  ║ [Term1][Term2][+]          │
│ ● Task A│ ~ src/a.ts  │ 12 13 + code              ║ xterm (400px, resizable)  │
│ ● Task B│             │ [💬 Quote in Chat]        ║                           │
│▾Review  │      ┌ReviewPill (draggable)┐           ║                           │
│▸Done    │      ┌Chat overlay (drag/resize)───────────────┐                    │
│ ⧉Staging│      │ 💬 Chat               [⟲][—]            │                    │
│         │      │ user bubble · tool group · markdown      │                    │
│         │      │ [∞ a.ts] [Yes][No]  ⏱3.4s · 12k tokens   │                    │
│         │      │ Queued(1) [Send now] ×                   │                    │
│         │      │ [Ask to make changes, @mention files]    │                    │
│         │      │ [💡Thinking][📖Plan][Default▾][⟳Claude](↑)│                    │
├─────────┴──────┴──────────────────────────────────────────┴────────────────────┤
```
- The chat overlay sits bottom-right: 380px wide, 55% of the height (`WorkspacePanel:1534`). When collapsed it becomes a pill.
- The staging workspace shows only the Files tab, with a banner.

### 1.6 Modals
TaskPopover:
```
┌──────────────────────────────────────────────────────────────┐
│ [Task title]                                                 │
│ ┌ multiline MentionInput: @mention files, paste images ─────┐ │
│ └────────────────────────────────────────────────────────────┘ │
│ [📄auth.ts ×][img ×]                                           │
│ [💡Thinking][📖Plan][Model▾][Claude▾]  [🔍][Cancel][Add & Start][Add] │
│ ⌘Enter to add · ⌘⇧Enter to start · @mention · Paste images     │
└──────────────────────────────────────────────────────────────┘
```
- Thinking is hidden for Codex.
- Changing the provider resets the model.
- 🔍 opens SearchModal (repo scope); its "add to context" appends a chip.

CardDetailOverlay:
```
┌──────────────────────────────────────────────┐
│ [● Running 5m]                           [×] │
│ Task title                                   │
│ markdown description                         │
│ Branch   feat/a                              │
│ Changes  +12 −4                              │
│ PR #42   [Open] [✓ Passing]                  │
│ Link     github.com/…                        │
│ Tip: ⌘+click card      [Go to workspace →]   │
└──────────────────────────────────────────────┘
```

JiraImportPopover:
```
┌ Import from Jira ──────────────── ☑ All ┐
│ [Filter by name…] [Status▾]             │
│ ☐ KEY  type  [status]                   │
│   summary                               │
│ n selected            [Cancel][Import]  │
└─────────────────────────────────────────┘
```

SearchModal:
```
┌─────────────────────────────────────────────────────────┐
│ 🔍 [Search file contents…] [.*][Aa]                      │
├──────────────────────┬──────────────────────────────────┤
│ dir/ file.ts :42     │ file preview, line 42            │
│   matched line       │ highlighted and centred          │
├──────────────────────┴──────────────────────────────────┤
│ ↑↓ navigate · ⏎ add to context · ⌘⏎ open in files · esc │
└─────────────────────────────────────────────────────────┘
```
- Search is debounced 150ms; at most 100 results (more shows "truncated"). Double-click acts like Enter.

RepoSettings (fixed full-screen):
- Left nav: "← Back to app", then the repo group (Scripts, Agent, Language Servers, MCP Servers, Knowledge), then General (Appearance).
- Right side: the section content with a "Saving…/Saved" indicator.
- Dragging empty space moves the window.

## 2. State architecture

### 2.1 `+page.svelte` state (`:110-143`)
- `repos`.
- `workspaces`: active repo only. Includes the staging workspace and `creating-<uuid>` placeholders.
- `activeRepo`: null means the Home screen.
- `selectedWsId`.
- `activeTab` ("diff").
- `chatExpanded`, `terminalPaneVisible`.
- `lspStatusMap`: a Map replaced on each change.
- `diffRefreshTrigger`.
- `showSettings`, `showSearchModal`, `showDepGraph`.
- `creatingWsId`: a single-flight lock on workspace creation.
- `repoSettings`, `contextBuildStatus`.
- SvelteMaps keyed by workspace id:
  - PR and git: `prStatusMap`, `changeCounts`, `gitOpInProgress`, `baseBehindMap`, `updatingBranchMap`.
  - Chat modes: `planModeByWorkspace`, `thinkingModeByWorkspace`, `modelByWorkspace`.
  - Chat and agent: `chatPanelApis`, `reviewByWorkspace`, `agentTaskByWorkspace`, `providerInfoByWorkspace`.
- Component refs: `titleBarRef`, `wsPanelRef`, `kanbanRef`.
- `repoDropdownIndex`.
- `appMode` ("plan" by default).
- `planView`: kanban | files | terminal.
- `planTerminalTabs`, `planActiveTerminalTab`, `planTerminalCounter`. The first tab is created lazily by an effect (`:519-523`).
- `todos`: a TodoItem[] mutated in place.
- `queueByWorkspace`: SvelteMap of QueuedMessage[], replaced with copies.
- `pendingDrain`: SvelteMap.

Autopilot state (`:145-243`):
- enabled, evaluating, prioritizedIds, prioritizing, maxConcurrentAgents (3).
- `MAX_AUTO_REVIEW_CYCLES` = 5.
- `autoReviewCount`, `autopilotPrPending`, `autopilotErrorWs`.
- `autopilotEvents`: capped at 200.
- Staging state: stagingWsId, stagingError, rebuildingStaging, merged and conflicting branch lists.
- Saved and restored per repo through the plain Map `autopilotByRepo`.

Home state: ghStatus, selectedProfile, ghRepos, repoSearch, loading/cloning flags, gh auth flags and device code, and the create-repo form.

Inline types:
- `TodoItem {id, repo_id, title, description, imagePaths?, mentionPaths?, planMode?, thinkingMode?, model?, provider?, ready?, depends_on?, created_at}`.
- `QueuedMessage {id, prompt, fullPrompt, images, imageDataUrls?, mentions, msgMentions?, planMode, thinkingMode, model, actionLabel?, hidden?}`.

Derived values:
- `filteredRepos`.
- `filteredGhRepos`: GitHub results minus repos already added locally (matched by name).
- `selectedWs`.
- `reviewingWsIds`.
- `activeWorkspaces`: excludes staging, sorted by `created_at`.
- `todoItems`.
- Kanban columns:
  - `inProgressWs`: no PR, or PR state "none".
  - `reviewWs`: PR state "open".
  - `doneWs`: PR state "merged".
- `completedReviewWs`, `reviewAlertWs`, `reviewAlertMore`.
- `activeAgentCount`: workspaces that are sending, reviewing, or being created.

### 2.2 Stores
**`messages.svelte.ts`**
- `Message {id, role: user|assistant|action, chunks, done, actionLabel?, imageDataUrls?, mentions?, planMode?, hidden?}`.
- `MessageChunk`: text | thinking | tool `{name, input, filePath?, oldString?, newString?}`.
- `messagesByWorkspace`: `SvelteMap<wsId, SvelteMap<msgId, Message>>`. Insertion order is display order.
- `sendingByWorkspace`.
- Token accounting:
  - `completedTokens`: plain Map, fed by cumulative usage events.
  - `turnTokens`: SvelteMap of live usage for the current turn.
  - `tokensByWorkspace` = completed + turn.
- API: addUserMessage, addActionMessage, addAssistantMessage (chunk order: thinking, text, tools), getMessages, loadPersistedMessages, flushPersist (never called), clearWorkspaceData.
- Persistence: a per-workspace 500ms timer, then `saveMessages` with the full array, fire-and-forget (`:178-194`). Token counts are not persisted.

**`toasts.svelte.ts`**: SvelteMap of toasts. Auto-dismiss: error 6s, info 4s, success 3s; 0 means sticky.

**`theme.svelte.ts`**
- localStorage keys `korlap-theme` and `korlap-color-mode`.
- `applyCssVars` writes `--token` and `--ansi-*` inline on `<html>`.
- Changes dispatch the window event `korlap-theme-change`.

**`pr-status.ts`**
- localStorage cache `korlap:pr-status-cache`, hydrated when a repo is selected.
- `refreshPrStatus` only writes when a field actually changes, and returns `changed`. The same change guard is used for change counts and base-behind.

### 2.3 Persistence
| Data | Save behaviour |
|---|---|
| Messages | 500ms debounce |
| Todos | `save_todos` immediately on every mutation |
| Repo settings | 600ms debounce |
| Knowledge-base scope | 600ms debounce |
| PR cache, theme | localStorage |
| Modes, queue, autopilot, terminal tabs, panel sizes and positions | memory only (lost on reload) |

### 2.4 Timers
| What | Interval |
|---|---|
| PR poll, open PRs only | 5s; re-runs autopilot if anything changed |
| Base-behind check, selected workspace | 60s |
| Thinking timer | 50ms |
| Kanban card elapsed time | 30s |

## 3. `ipc.ts`

### 3.1 Types
```ts
RepoInfo {id,path,gh_profile}
RepoDetail {id,path,gh_profile,display_name,default_branch}
AgentProvider = "claude"|"codex"
WorkspaceInfo {id,name,branch,worktree_path,repo_id,gh_profile,status:"running"|"waiting",
  created_at(unix s),task_title?,task_description?,source_todo_id?,custom_branch?,
  provider_override?,source_pr?,source_prs?,base_branch?}
SourcePr {number,branch,base_branch,url,title}
ProviderInfo {provider,supports_thinking,supports_plan_mode,models}
ToolUseInfo {name,input_preview?,file_path?,old_string?,new_string?}
AgentEvent = {type:"assistant_message",text,tool_uses,thinking?}
           | {type:"usage",input_tokens,output_tokens,cumulative}
           | {type:"done"} | {type:"error",message}      // no tool_result event
AgentStatusEvent {workspace_id,status}
JiraIssue {key,summary,description,status,issue_type}
GhProfile {login,active}   GhCliStatus {installed,authenticated,profiles}
GhRepoEntry {full_name,description,is_fork,clone_url,updated_at}
CreateRepoOptions {name,private,description,add_readme}
FileSearchResult {path,name,kind,score}
GrepMatch {path,line_number,column,line_content}   GrepResult {matches,truncated}
ModelOption {value,label}   FileEntry {name,path,is_dir,size}
ChangedFile {path,status,additions,deletions}
BaseUpdateStatus {behind_by}   CommitResult {hash,message}
PrStatus {state:none|open|merged|closed,url,number,title,checks:pending|passing|failing|none,
  mergeable:mergeable|conflicting|unknown,additions,deletions,ahead_by}
RepoPrEntry {number,title,branch,base_branch,author,url,updated_at,additions,deletions}
NamedScript {name,command}
LspServerConfig {command,args,extensions,detect_files,language_id,install_hint,project_roots}
McpServerConfig {server_type:stdio|sse,command,args,env,url,headers}
RepoSettings {setup_script,run_scripts,remove_script,pr_message,review_message,default_thinking,
  default_plan,system_prompt,lsp_servers,mcp_servers,agent_provider}
LspLocation {file_path,line,character}
LspDiagnostic {line,character,end_line,end_character,severity,message,source}
LspHoverResult {kind,text}   LspRenameResult {files_changed,edits_applied,details}
ScriptEvent = {type:"output",data} | {type:"exit",code}
StagingResult {workspace,merged_branches,conflicting_branches}
SystemResources {cpu_cores,memory_gb,available_memory_gb}
AutopilotAction {response,action_type,todo_ids,reorder}
ContextBuildStatus = not_built|building|built|failed
ContextMeta {include_globs,exclude_globs,build_status,last_built_at,invariant_count,fact_count,
  context_entry_count,contradiction_count,precheck_model,built_at_commit}
InvariantViolation {invariant_id,file,line,description}   InvariantCheckResult {passed,violations}
```

### 3.2 Commands (camelCase arguments; optional ones are sent as null)
**Repos**
- `add_repo(path)` → RepoDetail; `remove_repo`; `list_repos`.
- `list_gh_profiles` (unused); `set_repo_profile(repoId, profile)`.
- `check_gh_cli`; `gh_auth_login`; `cancel_gh_auth_login`.
- `list_gh_repos(profile, search?)`; `clone_repo(cloneUrl, repoName, destPath?, profile)`.
- `create_gh_repo(options, profile)`; `check_repo_gh_access(path, profiles)` → login or null.
- `get_jira_issues`.
- `get_repo_head`; `checkout_default_branch`.

**Workspaces**
- `create_workspace(repoId, taskTitle?, taskDescription?, sourceTodoId?, customBranch?)`.
- `remove_workspace`; `list_workspaces`.
- `create_workspace_from_pr(repoId, prNumber)`; `create_combo_workspace(repoId, prNumbers)`.
- `rename_branch(workspaceId, newName)` → WorkspaceInfo.

**Files and search**
- `save_image(workspaceId, data_base64, extension)` → path. Todo images use the namespace `todo-<repoId>`.
- Search: `search_workspace_files` / `search_repo_files`; `grep_workspace` / `grep_repo(id, pattern, isRegex, caseSensitive)`.
- Read: `read_workspace_file(wsId, filePath)`; `read_file` / `write_file` / `list_directory(wsId, relativePath)`.
- Repo scope: `read_repo_file` / `write_repo_file` / `list_repo_directory`.

**Agent and models**
- `list_models(repoId?)`: cached; falls back to Default.
- `get_provider_info`; `get_workspace_provider_info`; `switch_workspace_provider`.
- `send_message(workspaceId, prompt, onEvent: Channel<AgentEvent>, planMode, thinkingMode, model|null)`.
- `stop_agent`.
- `generate_commit_message`; `suggest_replies(text)`.

**Git and PRs**
- `get_changed_files`; `get_diff(workspaceId, filePath?)`.
- `check_base_updates`; `update_from_base`.
- `git_commit(wsId, message)`; `git_push`.
- `check_main_behind`; `sync_main`.
- `gh_pr_merge(wsId, prNumber)`.
- `get_pr_status`; `get_pr_template(repoId)`; `list_repo_prs`.

**Terminals**: `open_terminal(wsId, terminalId, onData: Channel<number[]>)`, plus write/resize(rows, cols)/close. The same set exists as `*_repo_terminal`.

**Persistence**: `save_messages` / `load_messages`; `save_todos` / `load_todos`.

**Settings, MCP, LSP**
- `get_repo_settings` / `save_repo_settings`.
- `test_mcp_server(config)`; `mcp_oauth_start(url)`.
- `lsp_start_server(wsId)`; `lsp_stop_server(repoId, serverId)`; `lsp_restart_server(repoId, serverId, wsId)`; `lsp_get_status`.
- `lsp_hover` / `lsp_goto_definition` / `lsp_diagnostics` / `lsp_rename`, with 1-based line and character.

**Scripts**: `run_script(wsId, command, Channel<ScriptEvent>)`; `stop_script`; `run_repo_script`; `stop_repo_script`.

**Autopilot and staging**
- `create_staging_workspace(repoId, branchNames)`; `remove_staging_workspace`.
- `get_system_resources`.
- `prioritize_todos(todoJson)` → ordered ids.
- `determine_dependencies(todoJson)` → a JSON string, parsed to `Record<id, id[]>`.
- `interpret_autopilot_command(command, contextJson)`.

**Knowledge base**
- `regenerate_hot`; `get_context_meta`; `save_context_scope(repoId, includeGlobs, excludeGlobs, precheckModel)`.
- `build_knowledge_base`, `update_knowledge_base_incremental`, `update_context_after_merge(repoId, wsId)`: each streams a `Channel<AgentEvent>`.
- `stop_context_build`; `check_invariants(wsId)`.
- `read_context_file` / `write_context_file`: invariants.md, facts.md, context.md, contradictions.md.
- `draft_contradiction_resolution(repoId, id, resolution, userNote)` and `resolve_contradiction(repoId, id, resolution, invariantText?)`. Resolution is exception, update_invariant or tech_debt.

**Plugins**: dialog `open` / `confirm`; opener `openUrl`; window `startDragging` / `toggleMaximize`; `convertFileSrc`.

### 3.3 Events
**`agent-status`** (`+page:607-623`)
- Updates `ws.status`, except "running" is suppressed while the workspace is being reviewed.
- On `waiting`: `setSending(false)`, drain the queue if `pendingDrain` is set, then `refreshBaseUpdates`.

**`workspace-updated`**: replaces the matching workspace entry.

**`todos-changed {repo_id}`**: reloads todos when that repo is active.

**`lsp-status {repo_id?, server_id, status, message}`**
- Updates the status bar.
- Toasts: "starting" shows a 60s toast that is replaced on ready, error or stopped.
- CodeEditor refreshes diagnostics on "ready".

**`gh-auth-code`**: the device code during login.

## 4. Kanban lifecycle

### 4.1 Columns
| Column | Source | Extras |
|---|---|---|
| Todo | persisted todos | footer: New task (⌘N) and ⋯ |
| In Progress | PR is none or missing | |
| Review | PR open | accent count |
| Done | PR merged | dimmed; ⋯ → Remove all (with confirm) |

Workspaces whose PR is **closed** appear in no column.

### 4.2 No drag-and-drop
Moves happen like this:
1. **Todo → In Progress**: the Start button, "Add & Start" (⌘⇧Enter), or autopilot.
   - `handleSpawnFromTodo` (`+page:1522-1626`):
     1. Creates a placeholder `creating-<uuid>` with `source_todo_id`.
     2. Removes the todo optimistically (restored on failure).
     3. Calls `createWorkspace(repo, title, desc, todoId)`.
     4. Applies plan mode, model and provider (`switchWorkspaceProvider`).
     5. Sends the prompt as a hidden message: `title\n\ndesc` + "Referenced files:\n@path" + image instructions ("I've attached an image. Read it using the Read tool:\n<paths>").
   - It stays on the board (no mode switch).
2. **In Progress → Review**: happens when the PR becomes open (created via ⌘M or "Push & create PR").
3. **Review → Done**: happens when the PR is merged (⌘M, or picked up by the 5s poll).
4. **Card clicks**: a click opens CardDetailOverlay; ⌘-click goes straight to the workspace (`handleKanbanCardClick` `:1628`).

Other creation paths:
- Manual checkout (`:976`), Review PR (`:1014`), Combo PRs (`:1052`). These three switch to work mode.
- ⌘N in work mode creates a blank workspace (`:940`).
- All of them use a placeholder plus the `creatingWsId` lock.

### 4.3 PR action (`triggerPrAction` `:1978-2123`, guarded by `gitOpInProgress`)
**When a PR is open:**
1. Conflicting → prompt the agent to fetch, merge, resolve, commit and push.
2. Checks failing → prompt the agent to use `gh pr checks` and fix.
3. Local changes differ from the PR's additions/deletions → `generate_commit_message` → `git_commit` (tolerates "Nothing to commit") → `git_push`.
4. `ahead_by > 0` → push only.
5. Otherwise, unless `skipMerge`: `gh_pr_merge`, then `regenerate_hot`, then `update_context_after_merge` if the knowledge base is built.

Any git error is sent back to the agent as "This git operation failed … Diagnose".

**When there is no PR:**
1. Commit and push (or push only when there are no changes).
2. Prompt the agent to run `gh pr create --base <default>`, using the custom `pr_message` (variables `{{branch}}`, `{{base_branch}}`, `{{file_count}}`, `{{pr_template}}`) or the default prompt plus the PR template.

**Workspace action-bar buttons** (`WorkspacePanel:527-571`):
- When behind the base branch: `Update N` (⌘U).
- PR open: ↗ link, Review, then one of Resolve Conflicts / Fix issues / Checks pending / Push / Commit & push / Merge.
- PR merged: "Done".
- Changes but no PR: Review and "Push & create PR".
- All disabled while busy.

### 4.4 Review (`triggerReview` `:2170-2270`)
1. Add the action message "Reviewing code" and mark the review as running.
2. If the knowledge base is built, run `check_invariants`. With autopilot on, violations become a fix prompt and the review is aborted. Otherwise a toast is shown and the review continues. This step fails open.
3. Build the prompt: `DEFAULT_REVIEW_PROMPT` (`review-prompt.ts`: multi-agent review with `[CLEAN]`/`[ISSUES]` output and `gh api` inline comments), plus `review_message`, with `{{branch}}`, `{{base_branch}}`, `{{pr_number}}`, `{{pr_title}}` substituted.
4. Stream on the same session. Only `currentTask` and `resultMarkdown` are updated; nothing is added to the chat.
5. In the ReviewPill: "Send to Chat" sends "Address all issues…"; Cancel stops the agent.

`handleSend` is blocked while a review exists for the workspace.

### 4.5 Autopilot (`:1267-1404`)
**Activation** (TitleBar button or ⌘4):
1. `maxConcurrentAgents = max(2, min(cpu/2, memGB/4))`.
2. `determineDependencies` → `prioritizeTodos` → `rebuildStaging` → evaluate.

**Each evaluation** is single-flight, takes one action and returns. Steps are tried in order:
1. **Auto-answer**:
   - A workspace in plan mode gets "Execute the plan above…".
   - Otherwise, if the last assistant text has a line ending in "?", send `suggestReplies()[0]` (fallback "Yes, proceed.").
2. **Completed reviews**:
   - clean or failed → `triggerPrAction(skipMerge)` and mark the PR as pending;
   - issues → "Fix them all".
3. **Spawn**: if below the concurrency limit, spawn the next ready, unblocked todo in priority order.
4. **Auto-review**: idle In Progress workspaces with a diff get a review, up to 5 cycles; after that, create the PR.
5. **Open PRs**: conflicting or failing checks → `triggerPrAction(skipMerge)`.

It **never merges**.

**Triggers for evaluation**: PR poll changes, agent done or error, review done, a todo marked ready, a spawn, an orchestrator command.

**Dependencies** (`:1217-1230`):
- Resolved when the dependency's todo is gone and its workspace's PR is merged (or no workspace exists).
- Blocked todos are skipped.

**Staging** (`:1406-1454`):
- Merges all Review-column branches into a staging worktree.
- Rebuilt on every PR status change, **even with autopilot off**. Removed when the Review column is empty.

**Orchestrator input** (`:1472-1520`):
- Sends a context JSON to `interpret_autopilot_command`.
- Possible actions: pause, resume, skip_todo, prioritize.

**Leftover hack**: `AUTOPILOT_BLACKLISTED_BRANCHES = {"feat/autopilot-mode"}` (`:147`).

## 5. Chat

### 5.1 Send pipeline
**`handleSend`** (`:1878-1946`)
- Saves images via `save_image` (workspace directory).
- Expands mentions:
  - file → `<file path lines [focus_line] source="mention">content</file>`;
  - folder → `<folder path/>`.
- `fullPrompt` = image instructions + context blocks + prompt.
- The raw prompt, the image data URLs and the mentions are kept for display.

**`routeMessage`** (`:1842`)
- Agent busy → enqueue.
- Agent idle with a queue → enqueue, then drain.
- Otherwise → send directly.

**`sendDirect`** (`:1710-1801`): sets sending, adds a user message (respecting hidden) or an action message, then calls `sendMessage` with a Channel. Event handling:

| Event | Handling |
|---|---|
| `assistant_message` | Add the message; bump `diffRefreshTrigger` on tool use. Set `agentTaskByWorkspace` via `formatToolTask` (Read→"Reading", Grep→"Searching", Glob→"Finding files", Bash→"Running command", Edit→"Editing", Write→"Writing", Agent→"Delegating", else "Using X"). Text without tools → "Thinking...". |
| `usage` | finalize (cumulative) or accumulate the turn. |
| `done` | Stop sending; refresh counts and PR status; `refreshSuggestions`; merge `listWorkspaces`; set `pendingDrain`; run autopilot. |
| `error` | Toast and cleanup. |

**Queue drain is two-phase**: the Channel `done` event sets `pendingDrain`, and the `agent-status: waiting` event actually drains it.

Variants:
- `handleSendImmediate`: used by suggestions and AskUserQuestion; bypasses the queue.
- `handleSendQueuedNow`: stops the agent, then sends.
- `sendPrompt`: internal programmatic sends; never in plan mode.

### 5.2 Rendering (`buildVisualBlocks` `chat-utils:107-176`; ChatPanel `:422-585`)
Hidden messages are skipped.

| Block | Rendering |
|---|---|
| user | Image thumbnails, "Plan" badge, `renderUserMarkdown` with `@name` turned into a `.msg-mention-chip[data-mention-path]`; clicking a chip opens the Files tab. |
| action | Centred label. |
| assistant-label | Renders nothing. |
| thinking | `<details>`, collapsed by default; state in the `expandedThinking` set. |
| text | `renderMarkdown` (marked + hljs + DOMPurify, cached), with `externalLinks` and `copyCodeBlocks`. |
| tool-group | Consecutive normal tools across messages. Header shows a spinner if this is the last block while sending (else a gear), the last tool's icon and input, and "N actions". Expanding shows a pill per tool. Icon map: Read, Edit, Write, Bash, Glob, Grep, Agent, Web*, Skill, ToolSearch, AskUserQuestion, TodoWrite, and the codex tools. |
| special-tool | AskUserQuestion, or EditDiffBlock when both old and new strings are present. |
| todo-list | TodoWrite; the last one is the latest. |
| footer | Recent file pills; Execute plan / Revise; suggestions; while sending "⏱ s · N tokens", afterwards a token summary. |

Tool results are never shown, and there is no cost display. Images appear only on user messages, as data URLs that are also persisted.

### 5.3 Input
- `@query` triggers `searchWorkspaceFiles` (100ms debounce).
- ChatPanel intercepts ↑ ↓ Enter Esc while autocomplete is open.
- Pasted images go into a preview strip.
- Toolbar:
  - Thinking (hidden if `supports_thinking` is false), Plan, Model (fixed-position dropdown).
  - Provider toggle, behind a destructive-warning confirm dialog (`:829-848`).
  - Stop or Send.
- In plan mode the placeholder becomes "Describe what to analyze…".
- A "Knowledge base not built" warning shows when the chat is empty.

### 5.4 Suggested replies (`:244-298`)
1. Runs on `done`. Takes the last assistant text and finds the last line ending in "?".
2. If that line matches `CONFIRM_RE` ("want me to", "proceed", …), show "Yes"/"No" instantly.
3. Always call `suggest_replies` and swap the results in; stale responses are dropped by request id.

### 5.5 Plan mode
- **Execute plan** appears when the last non-assistant message is a plan-mode user message and an assistant has replied. It sends "Execute the plan above. Do not ask for confirmation" and turns plan mode off.
- **Revise** hides the buttons until new messages arrive.

### 5.6 Streaming
Events arrive as whole `assistant_message` events; there is no token-level streaming.

## 6. Diff viewer, files, editor, terminal, scripts, PR UI

### 6.1 DiffViewer
- Loading is triggered by an effect on `workspaceId` and `refreshTrigger`.
- The spinner shows only on the first load; later refreshes are silent and only write when the content changed (`:24-99`).
- Selects the first file automatically.
- `parseDiff` (`:108-138`): tracks old and new line numbers from hunks and hides header lines. There is **no syntax highlighting**.
- Quoting (`:171-237`):
  - Click a line number to select it; shift-click extends the selection.
  - "Quote in Chat" inserts `` `path` Lx-y ```diff…``` `` via `insertText`.
- Per-line ↗ opens the file at that line; per-file "open in Files".
- Status glyphs: A "+", D "−", others "~".
- File sidebar width 140–500px.

### 6.2 FileBrowser
- Lazy tree; directories expand on demand.
- Devicon icons; binary files disabled; sizes in B/KB/MB.
- Changing scope reloads the root and calls `lspStartServer`.
- Shortcuts, only when the browser is visible (`:183-204`):
  - ⌘F: fuzzy file finder (150ms debounce).
  - ⌘⇧F: grep SearchModal ("open file").
- Deep links (`:432-514`): expand each directory segment, select the file, then `goToLine`.
- View mode is a read-only CodeEditor with an Edit button; Edit mode shows Save/Cancel (`write_file`).
- LSP features only in workspace scope.

### 6.3 CodeEditor
- Extensions: lineNumbers, active-line highlight, foldGutter, drawSelection, rectangularSelection, bracketMatching, selection matches, indentOnInput, history; keymaps default, history, fold, search, indentWithTab.
- Compartments: theme, lsp, lang, readonly, editable.
- Lifecycle: the view is created once; separate effects sync content (with an echo guard), language, readonly and LSP.
- Languages:
  - JS/TS family (ts, tsx, js, jsx, mjs, cjs); svelte and html via html().
  - rs, py, css/scss, json, md, java/kt, c/cpp, go, php, xml/svg, yaml.
- Theme:
  - Built from EditorColors with a custom HighlightStyle.
  - Font `--font-mono` at 0.78rem, line-height 1.6.
  - Reconfigured on `korlap-theme-change`.
- LSP:
  - Hover tooltip (400ms, markdown).
  - ⌘-click go-to-definition, ⌘-hover underline.
  - Diagnostics as error/warning/info marks, refreshed only on the `lsp-status: ready` event.

### 6.4 Terminal
- xterm settings: scrollback 10000, 'SF Mono'/'Fira Code'/'Menlo' at 13px, FitAddon.
- Lazy init once the container has a non-zero height, via the visible effect plus rAF, or the ResizeObserver.
- Data flow:
  - Input: TextEncoder → `number[]` → `write_terminal`.
  - Output: Channel `number[]` → `term.write(Uint8Array)`.
- Fit is debounced 100ms, then `resize_terminal`.
- Never unmounted. Every workspace and tab sits in a `.ws-terminal-layer` (`display:none`, `inert`). Plan-mode repo terminals use the same pattern.
- `onDestroy` does not call `close_terminal`.
- ⌘3 focuses the xterm helper textarea.
- One terminal tab is created automatically per workspace; × closes a tab (minimum one).

### 6.5 Scripts
- Two copies of the same logic: WorkspacePanel `:171-514` and TitleBar `:138-436`.
- Split button "▶ Run <branch>" runs the first script; the ▾ dropdown lists all scripts plus "Show last output".
- Output:
  - `ScriptEvent` channel into a 500-line ring buffer.
  - `ansiToHtml` handles SGR 0-4, 30-37 and 90-97 colours as `var(--ansi-*)`.
  - Shown in a popover with auto-scroll and the exit status.
- After finishing, the button flashes success or error for 2s.
- ⌘⇧R toggles run/stop.

### 6.6 PR UI
- Workspace action bar (see §4.3).
- Sidebar dots (`Sidebar:138-146`, `:402-429`): pr-open blue, pr-fail red, pr-merge green pulse, running accent pulse, waiting green.
- CardDetailOverlay and ReviewAlertBar.

## 7. Keyboard shortcuts
The global handler is on window (`+page.svelte:680-827`); ⌘ means Meta or Ctrl.

| Key | Context | Action |
|---|---|---|
| ⌘, | repo active | toggle Settings (RepoSettings also closes on ⌘,) |
| ⌘E | any | toggle repo dropdown; while open: ↑ ↓ Enter, 1-9, Esc |
| ⌘N | plan + kanban | New Task; elsewhere a blank workspace |
| ⌘W | workspace selected, any mode | remove workspace (confirm) |
| ⌘⇧F | work mode + workspace | SearchModal |
| ⌘1 / ⌘2 / ⌘3 | any | plan mode: Kanban / Files / Terminal (focuses the terminal) |
| ⌘4 | any | toggle Autopilot |
| ⌘⇧R | not in an input | run/stop script (workspace or repo) |
| ⌘R | work, not in an input | review |
| ⌘M | work, not in an input | PR action |
| ⌘U | work, not in an input | update branch from base |
| ⌘` | work | toggle terminal |
| ⌘J | work | toggle chat (focus input when opening) |

Component-level keys:
- **Kanban** (`KanbanBoard:152-231`): arrows move focus; Enter opens edit (todo) or details (workspace); ⌘Enter goes to the workspace; Esc clears focus.
- **Chat**: Enter sends, Shift+Enter adds a newline; autocomplete uses ↑ ↓ Enter Esc; Backspace removes a whole chip.
- **TaskPopover**: Enter in the title moves to the description; ⌘Enter adds; ⌘⇧Enter adds and starts; Esc closes in stages (model menu, then autocomplete, then the popover).
- **ManualCheckoutPopover**: ⌘Enter creates; Enter moves to the next field; Esc cancels.
- **Other overlays**: the PR, Combo and Jira popovers, CardDetail and DependencyGraph all close on Esc.
- **SearchModal**: ↑ ↓; Enter adds to context; ⌘Enter opens in Files; Esc closes.
- **FileBrowser**: ⌘F / ⌘⇧F; the finder uses ↑ ↓ Enter Esc.
- **Sidebar rename**: Enter commits, Esc cancels.
- **AutopilotPill input**: Enter sends, Esc collapses.
- **CodeMirror**: ⌘-click to go to definition, plus its default keymaps.
- **Double-click**: TitleBar maximizes; a Sidebar item renames.

Advertised but **not implemented**:
- Esc to stop the agent (tooltip `ChatPanel:764`).
- ⌘⇧F in TaskPopover (`:471`), blocked by the global handler's work-mode guard.

## 8. Theme, tooltip, typography

**Themes** (`themes.ts`): amber (default), indigo, peach, slate. Each has `css {dark, light}`, `terminal {dark, light}` and `editor {dark, light}`.

| Theme | bg-base | bg-card | text-primary | accent |
|---|---|---|---|---|
| amber | #12110e | #1a1814 | #d4c5a9 | #c8a97e |
| indigo | #141318 | #19181d | #d8d2e4 | #A3C7D6 |
| peach | #141010 | #1a1614 | #d8c0b6 | #e09880 |
| slate | #121214 | #18181c | #d4d4dc | #7eaacc |

**Tokens** (`app.css` fallback plus a light-mode media block; overwritten inline at runtime):
- Backgrounds: bg-sidebar, bg-base, bg-titlebar, bg-card, bg-hover, bg-active.
- Borders: border, border-light.
- Text: text-muted, text-dim, text-secondary, text-primary, text-bright.
- Accent and status: accent, status-ok, status-pr-open, status-fail.
- Diff and error: diff-add, diff-add-bg, diff-del, diff-del-bg, error, error-bg.
- Overlays and controls: toast-bg, toast-border, overlay-bg, input-inset-bg, input-inset-focus, btn-subtle-bg, btn-subtle-hover, pill-btn-hover, code-inline-bg, code-block-bg, img-remove-bg, img-remove-hover.
- Dev mode: bg-dev, border-dev.
- Fonts: `--font-sans`, `--font-mono`.
- Terminal colours: `--ansi-*` and `--ansi-bright-*` are injected at runtime.

**Palette shapes**
- TerminalColors: background, foreground, cursor, cursorAccent, selection, plus 16 ANSI colours.
- EditorColors: keyword, operator, string, number, comment, docComment, variable, property, link, text, textSecondary, textMuted, bg, bgCard, accentRgba, borderColor.

**highlight.js theme** (`hljs-korlap.css`) maps onto the tokens.

**Colour mode** is dark, light or system; changes dispatch `korlap-theme-change`.

**Tooltip action** (`actions.ts:19-129`): `use:tooltip={{text, shortcut?, placement?}}`.
- Removes the native title attribute.
- Appears after a 100ms delay; the element is appended to `document.body`.
- Centred and clamped 8px from the edges, with a 6px gap; flips placement when there is no room.
- Hidden on pointerleave and pointerdown.
- CSS in `app.css:126-178`: z-index 99999, 0.7rem text, kbd badge.

**Other actions in `actions.ts`**
- `draggable`: clamps within the parent and re-clamps on resize.
- `resizable`: 8 handles; reports `posDelta` so a bottom-right-anchored panel keeps its edge.
- `externalLinks`: opens links with `openUrl`.
- `copyCodeBlocks`.

**Typography**
- Body 14px "Space Grotesk", falling back to system-ui. The font is **not bundled**.
- Monospace: "SF Mono", "Fira Code".
- Common sizes: 0.7rem uppercase section labels; 0.72–0.82rem UI text.
- Bar heights: titlebar 40px, action bar 38px, status bar 22px.

## 9. RepoSettings
Autosave with a 600ms debounce. When it closes, the parent reloads the settings and the repo list.

**Scripts**
- `setup_script`: runs after a workspace is created.
- `run_scripts[]`: name and command; the first is the default.
- `remove_script`: runs before a workspace is removed.
- Environment variables available to scripts: `KORLAP_WORKSPACE_NAME`, `KORLAP_WORKSPACE_PATH`, `KORLAP_ROOT_PATH`, `KORLAP_DEFAULT_BRANCH`.

**Agent**
- `system_prompt`.
- `default_thinking` and `default_plan` toggles.
- `pr_message`: replaces the create-PR prompt; template variables as in §4.3.
- `review_message`: appended to the review prompt.
- `agent_provider` exists in the type but has **no UI**.

**Language Servers**
- Built-ins (`:151-156`): rust-analyzer, typescript-language-server --stdio, gopls serve, pyright-langserver --stdio.
- Overrides and custom servers, each with: command, args, extensions, detect_files, language_id, install_hint, project_roots.
- Badges and live status; Stop, Restart (needs a workspace), Reset, Remove, Add.

**MCP Servers**
- Name validation: lowercase letters, digits and hyphens; "korlap" is reserved.
- Types:
  - stdio: command, args, env as `KEY=VAL` lines;
  - sse: URL, plus "Authenticate with OAuth".
- Test connection and Remove buttons.

**Knowledge**
- Status badge and live build activity; Cancel, Update (incremental), Rebuild.
- Review tabs: Invariants (`- INV-N:`), Facts (`##` sections), Context (`##` sections), Contradictions (`## CONTRA-N`).
  - Invariants, Facts and Context entries can be edited or removed inline, which rewrites the markdown file.
  - Contradictions are resolved via a note → AI draft (exception / update invariant / tech debt) → edit → Apply.
- Scope include/exclude globs.
- Pre-check model, default `claude-haiku-4-5-20251001`.

**Appearance**: theme cards with colour swatches; colour mode Light/Dark/System.

## 10. Tests, performance, gotchas, bugs

### 10.1 Tests
- `tests/app.spec.ts` has 4 Playwright tests using a mocked `__TAURI_INTERNALS__` (`helpers.ts`). The mock `send_message` emits `assistant_message` then `done` after 100ms.
- **All stale**: they assert "Open Repository", "Korlap", `.repo-tab`, "Workspaces", "+ New workspace", `.tab.active` "Chat", an `<input>` placeholder, and a "Send" text button. None of these exist now.
- `mockTauri` handlers cannot be functions, because they are serialised through `addInitScript`.
- No unit tests.

### 10.2 Performance tricks
1. Messages are a SvelteMap per message, so updating one message touches one cell.
2. `VirtualScroller` measures heights and anchors scroll. Expanded/collapsed state lives in parent-owned SvelteSets so it survives row recycling.
3. Markdown output is cached; only 20 highlight.js languages are loaded.
4. PR status, change counts, base-behind and diffs only write when the value changes.
5. PR status is hydrated from a localStorage cache.
6. Terminals are hidden with `display:none` + `inert`; PTY bytes never touch Svelte state.
7. Mode layers use `visibility` + `inert` instead of unmounting.
8. Optimistic placeholders for creating and removing workspaces.
9. CodeMirror changes go through compartments, so the editor is never rebuilt.
10. Debounces: messages 500ms, settings 600ms, search 100–150ms, GitHub search 400ms, terminal fit 100ms.

### 10.3 Gotchas
- **Chat panels are not really kept mounted.** `WorkspacePanel:677-782` uses `{#if isActive}…{:else}` branches, so switching workspaces destroys and recreates the panel. Draft text, pasted images and suggestions are lost. The hidden branch also omits `onProviderSwitch`.
- **The queue drain needs `agent-status: waiting`.** If that event never arrives, the queue stalls.
- **Repo switching leaves stale maps.** `selectRepo` does not clear `prStatusMap` or the mode maps.
- **⌘R/⌘M/⌘U inside inputs** skip the action without calling `preventDefault`. A webview reload on ⌘R is possible.
- **⌘⇧F can open two modals.** FileBrowser's `stopPropagation` does not stop the other window listener (`FileBrowser:183` vs `+page:756`).
- **Kanban arrow keys stay active** under the Settings and DependencyGraph overlays (`KanbanBoard:155`).
- **Duplicated logic**: `ansiToHtml`, the script runner, `TodoItem`, `resultKind` and `isTodoBlocked` exist in several places with subtle differences.

### 10.4 Bugs (file:line)
1. A mention's `lineNumber` is dropped. The chip does not store it (`MentionInput:53-67`) and serialisation rebuilds without it (`:225-237`), so SearchModal's line focus never reaches the prompt (`+page:1904`, `2759`).
2. "Esc to stop" is advertised but not implemented (`ChatPanel:764`).
3. Closed-PR workspaces are missing from Kanban (`+page:571-582`) but listed under In Progress in the Sidebar (`Sidebar:53-62`).
4. Editing a todo loses its provider: no `initialProvider` is passed (`KanbanBoard:450-463`) and `handleEditTodo` never sets it (`+page:1139-1158`).
5. Removing a mention pill in TaskPopover does not stick, because inline mentions are merged back in (`TaskPopover:104-126`, `212`).
6. Jira "All" selects every issue, not just the filtered ones (`JiraImportPopover:71-77`).
7. CodeEditor's dark check uses `matchMedia` only and ignores the colour-mode override (`CodeEditor:498-501` vs `theme.svelte.ts:77-81`).
8. Diagnostics only refresh on an `lsp-status: ready` event (`CodeEditor:635-647`). `diagnosticCount` is never shown.
9. The `lsp` prop is an inline object literal (`FileBrowser:627,635`), so the editor is reconfigured and re-subscribed on every re-render.
10. LSP Args and Extensions fields eat spaces and commas as you type, because the controlled value is re-joined (`RepoSettings` LSP fields, around lines 960-985).
11. The `lsp-status` listener (`unlistenLsp`) is never removed (`+page:642`, `849-856`).
12. Removal cleanup is incomplete: `handleRemove` (`+page:1656-1667`) leaves review, task, API, provider and autoReview maps; RemoveAllDone (`:1686-1704`) also leaves model, baseBehind and updating maps. A failed removal restores the workspace at the end of the list.
13. The PR poll only covers open PRs (`:832-841`), so PRs created or merged externally are missed until the workspace is selected or the agent finishes.
14. `flushPersist` is never called, so up to 500ms of messages can be lost on quit.
15. Base64 image data URLs are persisted inside the messages JSON, and the full array is rewritten on every save.
16. The markdown caches grow without bound (`markdown.ts:86`, `110`).
17. A 50ms timer runs in every sending ChatPanel, including hidden ones (`ChatPanel:101`).
18. History loading on repo select is async and can be reordered with new messages (`+page:885`; `messages.svelte.ts:163-176`).
19. "Elapsed" on Kanban cards measures time since creation, not since the current run (`KanbanCard:61-75`; `CardDetailOverlay:41-45`).
20. The CardDetail PR link is a plain `target=_blank` link without `externalLinks` (`CardDetailOverlay:139`).
21. `ipc.ts` has no try/catch wrappers (contrary to CLAUDE.md), and many call sites swallow errors with `.catch(()=>{})`.
22. Native `title=` attributes remain (`Sidebar:209`; `+page:2422`, `2726`), as do `console.warn`/`console.error` (`pr-status.ts:80,99,113`; `RepoSettings:211,225`).
23. Autopilot only runs for the active repo, but Channel callbacks from other repos' agents trigger evaluation against the current one. Staging is rebuilt even with autopilot off (`+page:2279-2287`).
24. `AUTOPILOT_BLACKLISTED_BRANCHES` dev hack (`+page:146-147`).
25. DiffViewer `selectFile` has no stale-response guard (`DiffViewer:75-85`). FileBrowser drops unsaved edits on navigation without asking (`FileBrowser:289-305`).
26. AskUserQuestion answers are memory-only (`ChatPanel:187-192`).
27. Space Grotesk is never loaded.
28. Dead code: VirtualList, AgentActivityBar, `splitTextWithMentions`, `listGhProfiles`, `lspRename`, `getProviderInfo`, `flushPersist`, `RepoInfo`.

### 10.5 Rebuild recommendations
1. Break `+page.svelte` into stores and services: repo, workspace, todo, sendQueue, pr, review, a pure testable autopilot engine, and one shortcut registry. A single registry fixes the ⌘⇧F double-fire and the missing `preventDefault`.
2. Keep the strong patterns: SvelteMap per message, VirtualScroller, Channel streaming, xterm display toggling, CodeMirror compartments, the tooltip action, and theme tokens with the change event.
3. Keep ChatPanels truly mounted: one `{#each}` with CSS visibility instead of an if/else swap.
4. Add `tool_result` and cost to `AgentEvent` if those are wanted.
5. Rewrite the Playwright tests using function mocks (`page.exposeFunction`).