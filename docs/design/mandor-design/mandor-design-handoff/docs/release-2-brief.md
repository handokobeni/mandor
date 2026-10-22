# Design prompt: Mandor Release 2 (AC review)

Paste everything below the line into your design tool after the Release 1 brief. That brief still defines the product, users, design principles, design system, tokens, app shell, and Release 1 screens. This brief adds what Release 2 needs and records what Release 1 grew into after the first brief, so new screens fit the app as it is today.

Requirements source: `docs/plans/2026-10-21-003-feat-ac-review-plan.md`. Interface language stays **English**.

---

## 1. Release 2 in one paragraph

Tasks keep bouncing back from reviewers because acceptance criteria (AC) in Lark contain behavior that the designer's prototype does not show, and developers build what the prototype shows. Release 2 gives every run an **AC ledger**: one row per AC. Before coding, the agent compares each AC with the team's **design source** and marks what the design is missing. Before a PR, a separate **reviewer agent** checks every AC against the code, the tests, and, for web repos, the running app in the browser. The ledger then gates **Push & create PR**, is summarized in the PR description, and writes the design gaps back to the task in Lark so designers can update the design.

**Source of truth.** The AC in the tracker defines behavior; the design defines appearance where it does not contradict the AC. The gate checks code against AC, never pixel-matching against the design. The one exception is a row the developer marks **Follow design** (the AC is outdated): that row is excluded from the gate and Mandor asks the PM to update the AC.

**Delivery.** Release 2.0 ships everything in this brief except the items marked **(2.1)**: Figma, deployed URL, and image/PDF design sources, and task-specific design links. Design the 2.1 parts too, but keep them visually separable.

## 2. The app today (Release 1 as built, not in the first brief)

Treat these as existing screens and patterns. Restyle freely, but new screens must sit next to them.

- **Before coding (planning).** The Start dialog has a **Before coding** toggle: *Brainstorm, then plan* (default), *Plan only*, *Skip planning*, each stage with a skill picker (*Mandor built-in* or a Claude plugin skill such as `compound-engineering:ce-plan`). Choices are remembered.
- **Planning run states.** Labels *Brainstorming* / *Question for you* (agent asks clarifying questions in chat) and *Planning* / *Plan ready*. Action bar shows **Continue to plan** during brainstorm and **Approve plan & implement** when a plan is ready; **Push & create PR** is unavailable until the plan is approved.
- **Workspace tabs today:** Chat, **Plan** (rendered Markdown document with *Copy Markdown*), Changes, Task, Review comments. Release 2 adds **AC review** as the next tab.
- **Multi-repo workspaces.** Settings › Workspaces groups repos under a parent folder (e.g. `kelola-v2` = `kelola-api` + `kelola-web`). The Start dialog toggles **One repo | Workspace** with repo checkboxes. One run then has several repos, one branch, and one PR per changed repo; cards and the header show several PR links (`kelola-api#12`, `kelola-web#34`). Changes are grouped per repo.
- **Board scope:** **Mine | Team**. Team shows all team cards read-only with each card's PIC (assignee) names; Mine is the default.
- **Chat commands:** typing `/` in the composer lists Claude plugin skills (`/compound-engineering:ce-plan …`).
- **Settings sections:** Lark, Repos (with folder browser and repo discovery), Workspaces, General, About.

## 3. New concepts and their states

### 3.1 AC ledger row

Each row: AC label (e.g. `AC-4`, mono) + AC text, then two result columns.

**Design column** (filled during brainstorm/plan):
| Value | Meaning |
|---|---|
| Not checked yet | a design source was chosen but planning has not reported on this AC |
| In design | the behavior is shown in the design; link to where |
| Partly in design | some of it is shown |
| Missing from design | the AC has behavior the design does not show (informational, the agent still builds it) |
| Conflicts with design | the design shows different behavior for this AC (e.g. the action lives on another page); the developer may mark it Follow design |
| Follow design | the developer marked the AC as outdated; the agent follows the design and the PM is asked to update the AC |

When the run has no design source, the Design column shows **No design source** once (in the column header or summary strip), not *Not checked yet* on every row.

**Verdict column** (filled by the reviewer):
| Value | Gate (block mode) | Notes |
|---|---|---|
| Not reviewed | needs review | new rows, or no review yet |
| Pass | ok | evidence: file and line, test result, or browser steps |
| Fail | blocks Push & create PR | evidence and why; selectable for **Send to agent** |
| Can't verify | needs the developer's confirmation | reason, e.g. "app not running", "browser testing is off for this workspace", "repo has no app" |
| Checked manually | ok | the developer confirmed a *Can't verify* row |
| Skipped | ok | the developer used **Push anyway**; shows the written reason |
| Follows design | excluded | not reviewed against the AC |

Every verdict can become **Stale** when the code changes after the review: keep the previous verdict visible but marked stale, and treat the run as needing a new review.

A review that fails (time or cost limit, crash) leaves unreported rows at *Not reviewed*; only a review that finished normally turns unreported rows into *Can't verify*.

Rows whose AC was dropped or unlinked in Lark become **Removed**: shown muted below the active rows with a "Removed in Lark" marker, left out of the tab count, the summary strip, and the gate.

### 3.2 Review run

States: *Not reviewed*, *Reviewing* (per-row progress: rows fill in as the reviewer finishes them), *Finished*, *Failed* (crashed or timed out, with **Retry**), *Stale*. Each review in the history records when, what triggered it (*Manual*, *Push & create PR*, *Automatic fix*), whether the browser was used, and the result counts.

### 3.3 Local app (Run app)

States: *Off*, *Setting up* (optional setup command such as `bun install`, when the lockfile changed), *Starting* (per repo, e.g. kelola-api then kelola-web), *Waiting for port*, *Running* (with the app URL), *Running older code* (the code changed after the app started; restart before a browser review), *Failed to start* (with logs), *Stopped because another run's app was started*. Only one run's app runs at a time, so the UI must show **which run owns the running app**. The developer starts the database themselves and logs in to the app in their own Chrome at the repo's **app URL** (one address per repo, `http://localhost:<port>` by default, because login cookies are tied to the host). Mandor cannot see that login, so the developer confirms it ("I'm logged in").

### 3.4 Gate

**Block** (default) or **Warn**, set per workspace with an optional per-repo override.

## 4. Screens and changes

For every screen: default, loading, empty, and error states, dark and light themes, and the 960-wide compact layout.

### 4.1 Workspace › AC review tab (new)

- Tab label `AC review` with a compact count, e.g. `5/7` passing or a failure dot.
- **Summary strip:** counts per verdict, the gate state ("Push blocked by 2 AC", "Ready to push", "Review needed"), last review time and trigger, and **Run review** (disabled with a reason while the agent is working).
- **Rows:** label, AC text (two lines, expandable), Design value, Verdict value. Expanding a row shows evidence: file and line links (open in the Changes tab), test output excerpt, browser steps as a numbered list, and for Design the matched design location. Never show page data or screenshots in evidence.
- **Selection:** checkboxes on *Fail* rows → **Send to agent** (sends the selected findings to the implementing agent and marks those rows "Fix in progress" until the next review). Rows marked *Follows design* cannot be sent.
- **History:** previous reviews in a collapsible list, newest first, with their counts; selecting one shows that review's verdicts read-only.
- **States to draw:** task has no AC; run with no design source; only the Design column filled (planning done, no review yet); reviewing with partial rows; review failed or timed out (unreported rows stay *Not reviewed*, with **Retry**); results current; results stale after code changed; a new or changed AC arrived from Lark (row marked *New* / *Changed*, verdict *Not reviewed*); an AC removed in Lark; every AC passing; warn mode with a failing AC.
- **Accessibility:** verdict chips and the tab's failure marker carry text or an icon, never colour alone. Rows support arrow-key navigation, Space to select, and Enter to expand.
- Multi-repo runs: one ledger for the whole task; evidence names the repo for each file.

### 4.2 Approving the plan with design gaps

- When the plan is ready, the Plan tab (or the approve dialog) lists rows whose Design value is *Missing from design*, *Partly in design*, or *Conflicts with design*.
- Each listed row offers **Follow design** with a short explanation: the agent follows the design, the row leaves the gate, and the PM gets a request to update the AC. Undo is possible until the plan is approved.
- After approval, the design gaps are written to the task in Lark; show a quiet confirmation and the field's content in the Task tab.

### 4.3 Push & create PR gate

- **Button states:** *Ready* (all rows ok); *Review needed* (no review, or stale: pressing it starts a review and shows progress; the push does **not** continue by itself); *Reviewing* (disabled, with per-row progress; when the review finishes the button shows the new state and the notification asks the developer to press Push & create PR again); *Blocked* (lists the failing AC in a popover); *Needs confirmation* (some rows are *Can't verify*).
- **Confirm manual check dialog:** lists *Can't verify* rows with their reasons; one checkbox per row "I checked this myself"; the confirmation is recorded in the ledger and PR.
- **Push anyway dialog** (block mode): lists the failing AC; one required reason that covers all of them; explains that the rows become *Skipped* with this reason in the PR description. When *Can't verify* rows also exist, the same dialog lists them with their "I checked this myself" checkboxes, so one confirmation covers both.
- **Warn mode:** the button stays enabled with a warning marker ("Push with 2 warnings"), and the summary strip says "2 AC failed (warn mode)". Pressing it opens one confirmation dialog listing failing and unverified AC; no reason and no per-row confirmation.
- **AC sync warning:** if Mandor could not refresh the AC from Lark right before the push, the dialog says so, and the PR records when the AC were last synced.
- PR description preview (optional disclosure): the per-AC summary that will be added.

### 4.4 Run app

- Control in the action bar: **Run app** / **Stop app**, with state (3.3) and the local URL once running.
- **Confirmation dialog** before starting: the list of changed files per repo since the app last ran (or since the base branch on the first run), linked to Changes, with package manifests, scripts, and start-command files flagged; the `.env` keys that will be passed (checkboxes, chosen per repo in Settings; values never shown); and a reminder that the app runs code written by the agent and that `.env` must hold local or dev credentials only.
- **Readiness dialog** shown whenever a review starts (from Run review or from Push & create PR) for a run with a Web repo and browser testing allowed: database running (developer's confirmation), app running from the latest code, logged in at the app URL ("I'm logged in"), Claude in Chrome extension connected, browser testing allowed for this workspace. Missing items explain what to do (e.g. **Restart app** for older code); **Run review anyway** runs the review with UI AC marked *Can't verify*. Reviews started by an automatic fix skip the dialog.
- **Logs drawer:** per repo output while starting or after a failure.
- **Owner indicator:** title bar or sidebar shows which run's app is running; starting another run's app asks for confirmation and names the run whose app will stop.

### 4.5 Start dialog additions

- **Reviewer** picker: *Mandor built-in* or a Claude plugin skill, remembered like the planning skill pickers.
- **Design sources** multi-select from Settings, remembered. Empty state: "No design sources yet" with a link to add one. A remembered source that is now missing or unreachable shows a warning and is unselected.
- **(2.1) Task-specific design link** field, plus links detected in the task description offered as suggestions. Show the full host before use; nothing detected is used without the developer's confirmation.

### 4.6 Settings

- **Design sources** (new section): list with kind icon, name, location, status. Add or edit per kind:
  - Code repo or folder (2.0): path on the daemon machine (folder browser).
  - **(2.1)** Figma: file link plus a Figma token (password field, saved state never revealed, read-only token advised).
  - **(2.1)** Deployed prototype URL: https only, full host shown.
  - **(2.1)** Folder of images or PDFs.
  - Each kind has a **Test** action and a clear error state (path missing, token rejected, URL unreachable).
- **Repos › repo detail** additions: **Test mode** (*Web* or *No app*); for Web: optional setup command, start command, port, app URL, and `.env` keys to pass (key names only); **Gate override** (*Use workspace* / *Block* / *Warn*); **Browser testing** (used for single-repo runs).
- **Workspaces › workspace detail** additions: **Browser testing** on/off with a notice that page content is read by the AI model; **Gate** *Block* / *Warn*; per-repo override noted where it applies.
- **General** additions: reviewer model (default: same as the implementing agent) and cost limit per review.
- Explain precedence in place: design sources and reviewer settings are global; test mode is per repo; browser testing and the gate are per workspace, with a repo override for the gate (repo override > workspace > default; a multi-repo run uses the strictest member). Single-repo runs use the repo's browser testing setting.
- **About › system check:** add *Claude in Chrome extension connected*.

### 4.7 Board, sidebar, notifications

- **Card meta:** AC summary chip (e.g. `AC 5/7`, `2 failed`, `Review stale`) on cards with a run; design-gap marker on Todo/In Progress cards when the Design column found gaps.
- **Sidebar run labels:** *Reviewing AC*, *Review: 2 failed*, *Needs your check* (unverified AC), plus *Push held* for automatic fixes.
- **Notifications:** review finished (with counts); automatic fix push held because an AC failed or could not be verified; app failed to start.

## 5. Copy guidance

- Name the AC by its label in every message ("AC-4 failed: …").
- Reasons are concrete and short: "Can't verify: the app isn't running", "Can't verify: browser testing is off for kelola-v2".
- Never imply the AI verdict is final; use "Reviewer found", not "Mandor confirmed".

## 6. Out of scope

- AI Lembur (Release 3): keep the slots from the Release 1 brief.
- Testing mobile apps on emulators or devices; mobile repos use *No app*.
- Browsers outside the Chrome family.

## 7. Deliverables

1. Component additions: AC ledger row (all Design and Verdict values, stale, new/changed, selected, fix in progress), verdict chip, review summary strip, gate button states, Run app control and states, readiness checklist item, design source list item per kind.
2. Frames: AC review tab (all states in 4.1), plan approval with design gaps, every gate dialog in 4.3, Run app confirmation, readiness checklist, logs drawer, Start dialog additions, Settings sections in 4.6, board card and sidebar variants, notifications. Dark and light, 1440 × 900 and the 960-wide layout.
3. A clickable prototype of the Release 2 loop: Start with a design source → plan ready with gaps (mark one *Follow design*) → approve → agent done → Run app and log in → Run review → one AC fails → Send to agent → review again → one AC *Can't verify* → confirm manual check → Push & create PR.
4. Notes on how the AC review tab collapses in the 960-wide layout and on keyboard use of the ledger (row navigation, select, expand).

