# Design prompt: Mandor desktop app (all screens)

Paste everything below the line into your design tool (Figma AI, v0, Claude, or a human designer brief).

---

## 1. Product

**Mandor** ("foreman" in Indonesian) is a desktop app for software developers who run several Claude Code AI agents at the same time. Each agent works on one task from the team's **Lark Base "Sprint Tasks"** table, inside its own git worktree and branch. Mandor is the foreman's view: it shows which agent is doing what, which one needs the developer's attention, and keeps the Lark task board in sync automatically.

The pain it removes: today the team opens many terminals, one Claude Code session each, and forgets which terminal is working on which task and which agent is waiting for an answer.

Core loop:
1. The developer sees their Lark tasks on a kanban board.
2. They press **Start** on a ready task. Mandor claims the task, creates a worktree, starts an agent with the task title, description, and acceptance criteria, and moves the Lark status to *In Progress*.
3. They chat with the agent, review the diff, and press **Push & create PR**. Mandor commits, pushes, opens the PR, and moves Lark to *Waiting Review*.
4. When a reviewer moves the task to *Waiting Fix* and leaves comments in Lark, Mandor waits for a short quiet period, forwards trusted reviewer comments to the agent, fixes, pushes, and moves the task back to *Waiting Review*, without the developer having to step in.

## 2. Users and context

- Software developers on a small team (later: sold publicly). Comfortable with git, PRs, and terminals.
- Used all day, side by side with an IDE. Often glanced at, not stared at: status must be readable at a glance.
- Platforms: Windows (UI on Windows, engine inside WSL), macOS, Linux. Desktop window, minimum size **1024 × 680**, typical 1440 × 900. Must also work with the window snapped to half a 1920-wide screen (960 wide): define a compact layout for that.
- Interface language: **English**.
- The app minimizes to the system tray; agents keep running when the window is closed.

## 3. Design principles

1. **Attention first.** An agent waiting for the developer, a failed run, or a sync problem must be impossible to miss, from the sidebar and the board alike.
2. **Calm while working.** Running agents produce a lot of output. Show progress quietly; let the developer drill down when they choose.
3. **Lark is the source of truth for human fields.** The app never edits title, description, AC, or assignee. When Lark overrides the app, say so plainly.
4. **Safety is visible but not noisy.** Denied agent actions, sensitive-file holds, and claim conflicts are shown as clear, factual states, not alarms.
5. **Dense but legible.** A developer tool, closer to Linear, GitHub Desktop, or Zed than to a consumer app.

## 4. Design system and visual direction

### 4.1 Base system: shadcn (Svelte implementation)

The app will be implemented with **shadcn-svelte** (components built on **Bits UI**), **Tailwind CSS**, and **Lucide** icons. Design with the **shadcn/ui Figma kit** and Lucide so every component in the design maps 1:1 to a component in code.

Rules:
- Use stock shadcn components wherever they fit; restyle them through tokens, not one-off overrides.
- Only create a new component when no shadcn component fits (see 4.4), and build it from shadcn primitives and tokens.
- Keep shadcn component names and variants in layer names (e.g. `Button / variant=ghost / size=sm`) so the handoff is unambiguous.
- Density: this is a developer tool. Use the compact end of shadcn sizing (`sm` buttons and inputs in toolbars, 13–14px body text, tight card padding).

### 4.2 Tokens

Keep the warm "amber" feel of the current app. Map it onto the standard shadcn theme variables, in **both dark (default) and light** themes:

| shadcn variable | Dark (starting point) | Purpose |
|---|---|---|
| `--background` | `#12110e` | app background |
| `--foreground` | `#d4c5a9` | main text |
| `--card` / `--card-foreground` | `#1a1814` / `#d4c5a9` | cards, panels |
| `--popover` | `#1a1611` | menus, dialogs |
| `--primary` / `--primary-foreground` | `#c8a97e` / `#12110e` | primary actions (Start, Push & create PR) |
| `--secondary` | `#2a2520` | secondary buttons |
| `--muted` / `--muted-foreground` | `#1e1b17` / `#8a7e6a` | subtle surfaces, secondary text |
| `--accent` | `#2a2520` | hover/selected rows |
| `--destructive` | `#b5564e` | destructive actions, failures |
| `--border` / `--input` | `#3a3530` | borders, inputs |
| `--ring` | `#c8a97e` | focus ring |
| `--sidebar` (+ sidebar variants) | `#0f0d0a` | agents sidebar |

Add Mandor-specific semantic tokens (both themes), used by badges, dots, and banners:

| Token | Used for |
|---|---|
| `--status-working` | running, wf_running, preparing, publishing |
| `--status-attention` | awaiting_dev (strongest emphasis) |
| `--status-waiting` | wf_awaiting_comment, wf_quiet, in_review_loop |
| `--status-failed` | failed, sync_failed, deleted |
| `--status-warning` | overwritten, unassigned, orphaned |
| `--status-pr` | PR links and PR state |
| `--diff-add` / `--diff-add-bg`, `--diff-del` / `--diff-del-bg` | diff viewer |

Starting values: ok `#7e9e6b`, PR `#7e8ec8`, fail `#b5564e`, error text `#ee8888` on `#3a1a1a`, diff add `#7e9e6b` on `#1a2a1a`, diff delete `#c87e7e` on `#2a1a1a`. Adjust for AA contrast; never rely on color alone (pair with icon or label).

Typography: Space Grotesk for UI; a monospace font (e.g. JetBrains Mono) for code, diffs, branch names, task keys (`PGS-1388`), and tool calls. Radius: shadcn default reduced slightly (about 6px) for a tool-like feel.

### 4.3 shadcn components to use

| Need in Mandor | shadcn component |
|---|---|
| Actions | Button (default, secondary, ghost, outline, destructive), Button Group |
| Status chips, flags, readiness | Badge |
| Task cards, settings panels | Card |
| Start, Start fix, Take over | Dialog |
| Remove repo, confirm sensitive push, discard | Alert Dialog |
| Workspace tabs (Chat, Changes, Task, Review comments) | Tabs |
| ⌘/Ctrl+K palette | Command |
| Sprint filter, repo picker | Select / Combobox (Popover + Command) |
| Forms | Input, Textarea, Label, Checkbox, Switch, Form validation messages |
| Disabled-Start reasons, truncated text | Tooltip (reason must also be visible inline) |
| Run actions overflow, card menu | Dropdown Menu |
| Toasts | Sonner |
| Global banners (session expired, runs interrupted) | Alert |
| Agents sidebar | Sidebar (collapsible to icons) |
| Long lists, chat, diff | Scroll Area |
| Task details side panel | Resizable or Sheet |
| Tool call groups, Done column | Collapsible / Accordion |
| Loading | Skeleton, Spinner |
| Owners, reviewers | Avatar |
| Notification center | Popover + list |
| Onboarding wizard | Card + stepper built from Badge/Separator |

### 4.4 Mandor-specific components (build from shadcn primitives)

- **RunStatusBadge**: dot + label + icon for every run state (section 6).
- **CardFlagBadge**: the four sync flags, with optional action (Retry).
- **TaskCard**: kanban card with all variants (section 7.3).
- **AgentRow**: sidebar row with status dot, key, title, state; attention variant.
- **ChatMessage**: developer, agent (markdown), system, turn finished/failed, forwarded reviewer comment.
- **ToolCallGroup**: collapsed summary + expandable calls with result, error, and denied states.
- **DiffViewer**: file list + unified diff with line numbers and hunk headers.
- **Composer**: multiline input with Send/Queue, Send now, Stop, and queued-message chips.
- **StepProgress**: inline steps for Start (claiming → worktree → agent → done) with per-step error.
- **CommentItem**: Lark review comment with author, time, trusted badge, sent-to-agent state.

## 5. App shell

- **Title bar**: app name, daemon connection indicator (connected / reconnecting / unreachable), Lark session indicator (logged in as …, or "session expired" warning), Settings button, and a global "Refresh board" action.
- **Left sidebar: Active agents** (always visible, collapsible to icons in the compact layout). One row per active run: status dot, task key, title (truncated), state label. Runs waiting for the developer are pinned to the top and highlighted. Selecting a row opens that task's workspace.
- **Main area**: the board, a task workspace, or settings.
- **Global banners** (top of the main area, dismissible where safe): Lark session expired → "Log in again"; daemon restarted → "N runs were interrupted, resume them from their workspace".
- **Toasts** for short confirmations ("Lark settings saved", "Repo added", "PR created").
- Keyboard: ⌘/Ctrl+K command palette (open task by key, start task, go to settings), ⌘/Ctrl+Enter to send a message.

## 6. Domain states the design must cover

**Lark Task Status → board column**
| Lark status | Column |
|---|---|
| To Do | Todo |
| In Progress, Waiting Fix | In Progress |
| Waiting Review, In Review | Review |
| Waiting Deploy, Delivered | Done |
| anything else | Unknown (column only shown when it has cards) |

**Run (agent) states**, each needs a label, color, and icon:
| State | Label | Meaning |
|---|---|---|
| preparing | Preparing | creating worktree, starting agent |
| running | Working | agent is processing a turn |
| awaiting_dev | Waiting for you | turn finished, developer's move (most important attention state) |
| publishing | Creating PR | commit, push, PR in progress |
| in_review_loop | Done, waiting for review | PR open, nothing to do |
| wf_awaiting_comment | Waiting for reviewer comments | task is in Waiting Fix but no trusted comment yet |
| wf_quiet | Quiet period | trusted comment received, waiting ~2 min for more |
| wf_running | Fixing | agent is applying review feedback automatically |
| failed | Failed | agent crashed or daemon restarted; can Resume |
| orphaned | Parked: claim lost | another developer took over or the task was reassigned |

**Card flags** (from sync):
| Flag | Label | Action |
|---|---|---|
| overwritten | Overwritten by Lark | none; show the reason (e.g. "status in Lark is now In Review, not In Progress") |
| sync_failed | Sync failed | **Retry** |
| deleted | Deleted in Lark | Close run |
| unassigned | No longer assigned | Close run |

**Ownership**: one owner per task. A card can be *mine*, *owned by another developer (active)* → read-only, or *owned by another developer (inactive 30+ min)* → **Take over** available.

**Start gate**: Start is disabled with a visible reason when Development Readiness is not "Ready To Dev" or "Commit To Sprint", the status is not To Do, another developer actively owns it, or a run already exists. Example reason: `task is not ready: Development Readiness "Review AC By PM"`.

## 7. Screens

For every screen, design: default, loading, empty, and error states, plus the dark and light themes.

### 7.1 Connection states (full window)
- Connecting to the daemon (first launch can take a few seconds on Windows/WSL).
- Daemon unreachable, auto-retrying with backoff; show what to check ("Is mandord installed in WSL? See setup guide").
- Version mismatch (fatal): app and daemon versions differ; tell the user to update both.

### 7.2 First run / onboarding (wizard)
1. Welcome: what Mandor does, in three short points.
2. Prerequisites checklist: Claude Code logged in (subscription), GitHub CLI `gh` logged in, Lark Custom App created. Each item has a "How to" link. (The daemon can report these later; design the layout now.)
3. **Connect Lark app**: App ID, App Secret (password field; when already saved show "saved" placeholder and never reveal it), Base `app_token`, Sprint Tasks table ID, Acceptance Criteria table ID, callback port (default 53682, advanced). Inline validation for required fields. Helper text on where to find each value in a Lark URL.
4. **Log in with Lark**: button opens the browser. States: waiting for browser (with cancel), success ("Logged in as …"), and errors: callback port already in use, state mismatch (login cancelled), user denied access, timed out.
5. **Add a repo**: path on the daemon machine, optional gh profile. Show detected default branch on success; error when the path is not a git repo or `origin/HEAD` is missing (with the fix command).
6. Done → board.

### 7.3 Board (kanban)
- Header: sprint filter (All + sprint list), search by key/title, "Mine / All assigned" toggle, refresh, last synced time ("Synced 12s ago"), sync error indicator.
- Columns: Todo, In Progress, Review, Done, and Unknown when needed. Column header shows count. Done column can collapse.
- **Card anatomy**: task key (mono), title (2 lines max), readiness chip, run state (dot + label) when a run exists, flags and badges, owner avatar/name when not mine, PR link icon, branch name on hover. Primary action by context: **Start** (Todo), **Open** (has run), **Take over** (inactive owner), **Retry** (sync failed). Disabled Start shows the reason inline (not only in a tooltip).
- Card variants to draw: ready to start; blocked by readiness; owned by another dev (read-only); take-over available; working; waiting for you; creating PR; waiting for review with PR; waiting for reviewer comments; quiet period; fixing; failed; parked (claim lost); overwritten by Lark; sync failed; deleted in Lark; no longer assigned; unknown status.
- Developers cannot drag cards between columns (moves only happen through actions or in Lark); make this obvious, e.g. no drag handles, and a hint on attempted drag.
- Empty states: not logged in to Lark; no tasks assigned in this sprint; filter hides everything.

### 7.4 Start task dialog
- Task key, title, readiness.
- Repo picker (preselect the task's last repo); warning when no repo exists, with a link to add one.
- Optional note to the agent ("extra instructions").
- Confirm → shows progress steps inline: claiming task → creating worktree → starting agent → done. Errors per step: "lost the claim: another developer started this task at the same time", git failures.

### 7.5 Task workspace
Layout: header + action bar + tabs, with an optional right side panel for task details.

- **Header**: back to board, task key + title, Lark status chip, run state, branch (copy button), PR link.
- **Action bar** (by state): Push & create PR, Resume (failed), Start fix (Waiting Fix states), Close run (parked / done / deleted), Stop (while working). Show the run's note or last error as an inline callout:
  - "Fix finished without changes; status left unchanged"
  - "Changes touch CI, agent config, or git hooks; push needs developer confirmation" (with Confirm push / Discard)
  - "Fix Notes are from an untrusted author; confirm via Start fix"
  - "the daemon restarted while this run was active; resume to continue the session"
- **Tabs**:
  1. **Chat**: virtualized stream. Message types:
     - Developer message (right or highlighted).
     - Agent text (rendered markdown with code blocks, safe links).
     - Tool call group: consecutive tool calls collapsed into one block ("Read 3 files, ran 2 commands"); each call shows tool name + short preview (command, file path, pattern) and expands to its result. Errored and **denied** calls are marked (denial reason, e.g. "`git push` is run only by the daemon").
     - System message (commit created, turn stopped by developer, run interrupted).
     - Turn finished / turn failed markers, with token usage in a subtle footer.
     - Reviewer comments forwarded to the agent (from Lark, with author and trust badge).
   Composer: multiline input, Send (becomes **Queue** while the agent is working), **Send now** (stops the current turn, then sends), Stop. Show queued messages above the composer with remove buttons. Drafts persist per task.
  2. **Changes**: file list (status: added, modified, deleted, renamed, untracked; +/− counts) and a unified diff with line numbers and hunk headers. Empty state: "No changes against the base branch".
  3. **Task**: read-only Lark data: description (markdown), acceptance criteria list (AC id + text; dropped ACs greyed), readiness, assignees, reporter, sprint, Lark status, link "Open in Lark".
  4. **Review comments** (Waiting Fix): list of Lark record comments, newest first, each with author, time, and **trusted / not trusted** badge; which ones were already sent to the agent; quiet-period countdown when active.

### 7.6 Start fix dialog
- Instruction field (required).
- Comments list: trusted comments are pre-included (checked, locked) with a "will be sent" note; untrusted comments are opt-in checkboxes.
- Explain that the Lark status will move to In Progress.

### 7.7 Take over dialog
- Explain whose claim is inactive and since when.
- Repo picker. Warn that the previous developer's local work is not visible here; the branch is restored from the remote.

### 7.8 Settings
- **Lark**: same fields as onboarding step 3, login status, Log in / Log out, session-expired state.
- **Repos**: list of repos (id, path, default branch, gh profile). Detail/edit per repo:
  - gh profile
  - read-only companion paths (list editor)
  - trusted reviewers / QA (Lark open_id list; later a people picker)
  - extra agent allowlist (tool patterns such as `Bash(just *)`, with a short explanation of what an allowlist does)
  - Remove repo (confirmation dialog)
- **General**: theme (system / dark / light), desktop notifications on/off per kind, quiet period length (seconds), "Keep running in tray when closed".
- **About**: app and daemon versions, data folder location, open logs.

### 7.9 Notifications
- Desktop notification copy and layout for: Agent finished and is waiting for you; Run failed (with reason); The reviewer requested fixes (Waiting Fix); PR created; Needs attention (claim lost, sensitive changes held).
- In-app notification center (bell in the title bar): list of the same events, unread state, click → opens the task.

### 7.10 System tray
- Menu: Open Mandor, a short list of runs waiting for you (click to open), Quit. Tray icon variant when something needs attention.

### 7.11 Browser OAuth callback page
- Small standalone HTML page shown in the browser after Lark login: success ("Logged in to Lark. You can return to Mandor.") and failure (reason). Plain, branded, no external assets.

## 8. Future features (reserve space, do not design in detail)

- **AI Lembur (overnight agents)**: an "overtime queue" badge on cards, a queue view, and a morning report screen (tasks done, stage reached, parked tasks with the agent's questions, skipped tasks with reasons, token usage).
- **AC review** before PR: per-acceptance-criterion pass/fail report that can be sent back to the agent.

Make sure the sidebar, card badges, and workspace tabs can absorb these later.

## 9. Deliverables

1. Theme: shadcn variables and Mandor status tokens for dark and light, as Figma variables named exactly as in 4.2.
2. Component sheet: the shadcn components from 4.3 restyled with the theme, plus every Mandor component from 4.4 with all variants.
3. Frames for every screen and state in section 7, in dark and light, at 1440 × 900 and the 960-wide compact layout.
4. A clickable prototype of the core loop: board → Start → workspace chat → Push & create PR → card in Review → Waiting Fix → quiet period → fixing → back to Review.
5. Short notes on spacing, motion (subtle; no animation on streaming text other than appearing), and accessibility (contrast AA, focus states, keyboard navigation, status never conveyed by color alone).
