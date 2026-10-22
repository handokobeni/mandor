# Mandor design notes: spacing, motion, accessibility

Companion to `brand-spec.md` (tokens) and `components.html` (component sheet). Every value below is what the prototype in this folder actually uses, so the shadcn-svelte build can copy it 1:1.

## 1. Spacing and sizing

**Scale.** 2px steps up to 16, then Tailwind steps. Use only these values for padding, margin, and gap:

| px | 2 | 4 | 6 | 8 | 10 | 12 | 14 | 16 | 20 | 24 | 28 | 32 | 40 | 48 | 64 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Tailwind | 0.5 | 1 | 1.5 | 2 | 2.5 | 3 | 3.5 | 4 | 5 | 6 | 7 | 8 | 10 | 12 | 16 |

One deliberate exception: expanded tool call content is indented 34px so it lines up with the text in the summary row (10px padding + 16px icon + 8px gap).

**Control heights.**

| Element | Height | Notes |
|---|---|---|
| Button default / sm / xs | 32 / 28 / 24 | Toolbars and cards use `sm` and `xs` |
| Input default / sm | 32 / 28 | |
| Badge | 20 | Radius 4px |
| Tabs trigger | 36 | |
| Menu item | 30 | |
| Command item | 34 | |
| Title bar | 40 | Draggable region; buttons opt out |

**Radius.** `--radius` 6px for buttons, inputs, and menus. Cards and columns use 8px. Dialogs use 10px. Badges use 4px. Avatars and switches are round.

**Layout regions.**

| Region | 1440 × 900 | 960 (compact, ≤ 1100px) |
|---|---|---|
| Sidebar | 256px, full rows | 56px, status dot + key number, tooltip on hover |
| Board column | flexes, min 272px | fixed 264px, board scrolls sideways |
| Workspace details panel | 300px, inline | 300px overlay (Sheet) from the right |
| Changes file list | 280px | 220px |
| Settings nav | 200px | 160px; form grids collapse to one column |
| Chat column | max 820px, centered | same |

**Density rules.** Card padding 10px. Toolbar padding 12px 16px. Body text 13px, secondary text 12px. Max two lines for card titles; truncate long single lines with an ellipsis and keep the full text in `title`.

Window minimum: **960 × 680** (decided 2026-10-19). This replaces the 1024 × 680 in the brief so Mandor works snapped to half of a 1920 screen. At 960 the compact layout applies (it starts at ≤ 1100px). Set the Tauri window `minWidth: 960, minHeight: 680`.

## 2. Motion

Motion is quiet and short. Streaming agent text has **no animation**: new messages appear, nothing types, fades, or slides.

| Use | Duration | Easing |
|---|---|---|
| Hover, pressed, focus, switch thumb, tab underline | 140ms (`--fast`) | `cubic-bezier(0.2, 0, 0, 1)` (`--ease`) |
| Dialog and toast enter | 140ms; 4px rise + 0.99 scale | `--ease` |
| Toast exit | 200ms fade, after 3.6s | linear |
| Button press | 1px down, 80ms | |
| Working spinner (`loader-circle`) | 1.6s per turn | linear, continuous |
| Attention dot pulse (Waiting for you) | 2.4s, **3 cycles only**, then still | `--ease` |
| Skeleton shimmer | 1.4s | linear |
| Quiet period progress bar | 1s steps | linear |

`prefers-reduced-motion: reduce` turns off the spinner, pulse, shimmer, and dialog/toast entrances, and sets all transitions to 0ms. State is still clear because every state also has an icon and a label.

## 3. Accessibility

**Contrast (WCAG AA).** Text pairs were computed against `--card`, which is the darker of the two surfaces in dark mode. All pass 4.5:1:

| Pair | Dark | Light |
|---|---|---|
| foreground on card | 10.4 | 14.0 (on background) |
| muted-foreground on card | 5.3 | 6.2 |
| primary-foreground on primary | 8.5 | 5.9 |
| status-failed text | 6.9 | 5.9 |
| status-working / waiting / warning / ok / pr | 7.6 / 7.8 / 6.9 / 7.3 / 6.6 | 6.4 / 6.4 / 5.4 / 6.4 / 7.4 |
| attention chip text on fill | 9.7 | 7.7 |

Two values from the brief were raised to pass: `--muted-foreground` (`#8a7e6a` → `#988b74`) and the failed/destructive text (`#b5564e` → `#e08a80`). See `brand-spec.md`.

**Status is never color only.** Every run state is icon + label (RunStatusBadge). Sync flags are icon + label + reason. File status letters (A, M, D, R, U) are outlined and carry an `aria-label`. Diff lines also carry `+` / `−` signs.

**Disabled actions explain themselves inline.** Disabled Start shows the reason on the card (`Can't start: not ready, Development Readiness Review AC By PM`), linked with `aria-describedby`. A disabled composer shows the reason as its placeholder. Tooltips repeat, they never carry the only copy.

**Focus.** `:focus-visible` gets a 2px `--ring` outline with a 2px offset. Inputs show the ring color on the border plus a 3px soft halo. Mouse clicks do not show the ring.

**Keyboard map.**

| Keys | Action |
|---|---|
| ⌘/Ctrl + K | Command palette (open task by key, start task, settings) |
| ↑ / ↓, Enter | Move and choose in the command palette |
| ⌘/Ctrl + Enter | Send (or Queue while the agent works) |
| ← / → | Move between workspace tabs |
| Esc | Close dialog, menu, or palette |
| Enter | Open the focused repo row in Settings › Repos |
| Tab / Shift+Tab | Normal order: title bar, sidebar, toolbar, columns left to right, cards top to bottom |

**Semantics to keep in the Svelte build.**

- Dialogs and Alert Dialogs use the native modal behaviour (Bits UI Dialog does the same): focus moves in, is trapped, and returns to the trigger on close. Alert Dialogs use `role="alertdialog"`.
- Tabs use `role="tablist"`, `tab`, `tabpanel` with `aria-selected` and `aria-controls`.
- Toggle groups use `aria-pressed`. Switches use `role="switch"`.
- Live regions: the toaster and the sync indicator are `aria-live="polite"`; the chat stream is polite too, so a screen reader announces new messages without stealing focus.
- Loading views set `aria-busy="true"` on the main region and include a screen-reader-only "Loading…" line.
- Kanban cards are `draggable="false"` with no drag handle. Trying to drag shows a toast: cards move through actions or in Lark.
- Icon-only buttons always have `aria-label`. Decorative icons are `aria-hidden`.

## 4. Layer naming for handoff

Markup carries `data-ui="Component / prop=value"`, matching shadcn-svelte names, for example `Button / variant=ghost / size=sm`, `Badge / variant=secondary`, `AlertDialog`, `Sidebar / collapsible=icon`. Mandor components follow the same pattern: `TaskCard`, `RunStatusBadge / state=awaiting_dev`, `CardFlagBadge / flag=sync_failed`, `AgentRow / variant=attention`, `ChatMessage / from=agent`, `ToolCallGroup`, `DiffViewer`, `Composer`, `StepProgress`, `CommentItem`. The renderers for these live in `js/components.js` and `js/shell.js`; the component sheet uses the same code.

## 5. Reserved space for later releases

Shown in `components.html` › Slots for later releases. Placement only, not designed yet:

- **AI Lembur (Release 3):** an Overtime queue badge in the TaskCard meta row; sidebar nav items "Overtime queue" and "Morning report"; a sidebar group "Overtime tonight".
- **AC review (Release 2):** now designed, see section 7.

## 6. Notifications and tray

One event list (`MANDOR.notificationsRaw` in the prototype) feeds desktop notifications, the in-app notification center, and the tray attention dot. Full copy in `notifications.html`, menu and icon in `tray.html`.

**Events and copy.**

| Kind (Settings toggle) | Title | Click opens | Default |
|---|---|---|---|
| waiting | `PGS-1391 is waiting for you` | task workspace | on |
| attention (sensitive push held) | `PGS-1391: push held for your confirmation` | task workspace | on |
| attention (claim lost) | `PGS-1376 is parked: claim lost` | task workspace | on |
| fix | `Reviewer requested fixes on PGS-1371` | task workspace | on |
| failed | `Run failed: PGS-1379` | task workspace | on |
| pr | `PR #412 created for PGS-1380` | task workspace | off |
| grouped | `3 agents are waiting for you` | board | follows "waiting" |
| tray close notice | `Mandor is still running in the tray` | window | once |

**Rules.**

- One desktop notification per event; never per tool call or streamed message.
- No desktop notification when the window is focused on that task's workspace; the event still lands in the notification center.
- Three or more "waiting" events within 10 seconds are grouped into one notification.
- No action buttons on desktop notifications (behaviour differs across Windows, macOS, and Linux); click opens Mandor.
- These rules were proposed during design and accepted on 2026-10-19.

**Notification center.** Bell in the title bar, unread count in an attention-colored chip (max two digits, then `99+`). Popover 380px wide, list max 420px tall, newest first. Unread rows: tinted background, bold title, dot. Click opens the task and marks the item read; "Mark all as read" in the header. Esc closes and returns focus to the bell.

**Tray.** Monochrome mark (macOS template image) with an attention dot when anything is waiting, failed, or needs attention. Menu: Open Mandor · up to 5 waiting runs, oldest wait first (6th and beyond collapse into "Show all on the board") · Quit Mandor. Quit asks for confirmation only while runs are active. Tooltip: `Mandor · 2 waiting for you` or `Mandor · all quiet`.

**OAuth callback page.** Served by mandord on `127.0.0.1:<callback port>`. Fully self-contained (inline CSS, inline SVG, system font fallback), follows the OS light or dark setting. Reasons on failure: denied, state mismatch, expired, code exchange.


## 7. Release 2: AC review tab (2026-10-21, first pass)

Example run: PGS-1426 in workspace `kelola-v2` (`kelola-api` + `kelola-web`), AC labelled `AC-1` to `AC-8`. Open `workspace.html?task=PGS-1426&tab=ac`; every state has an `?ac=` key listed in `index.html` › Release 2.

**Rules the UI follows**

- Removed rows (`AC-8`, "Removed in Lark") sit muted below the active rows and are left out of the tab count, the summary strip, and the gate.
- No design source: "No design source" appears once, in the Design column header and in the summary strip. Rows show no Design value.
- Follows design rows are excluded from the gate and cannot be selected for Send to agent. Only Fail rows have a checkbox.
- Warn mode never yields Blocked or Needs confirmation.
- Push & create PR in *Review needed* starts a review and stops there. When the review finishes, the button shows the new state and a toast says nothing was pushed; the developer presses it again.
- Evidence is plain text (escaped). File links name the repo and open the file in Changes. No page data or screenshots.
- Copy says "Reviewer found", never "Mandor confirmed". The PR preview says verdicts are findings, not sign-off.

**Keyboard**

| Key | In the AC ledger |
|---|---|
| Tab | Enters the ledger on one row (roving tabindex), then leaves it |
| ↑ / ↓, Home / End | Move between rows |
| Space | Select or clear a Fail row for Send to agent; does nothing on other rows |
| Enter | Expand or collapse the row's evidence |
| Esc | Closes the Push blocked popover and returns focus to the button |

Rows are `role="row"` inside `role="grid"`, with `aria-selected` only on selectable rows and `aria-expanded` on every row.

**960-wide layout**

- The ledger drops to four columns: select, label, text, expand. The Design and Verdict chips stack under the AC text, Design first. Empty Design (no source) takes no space.
- The column header hides Design and Verdict; "No design source" stays visible in the summary strip.
- Evidence switches from two columns (Verdict | Design) to one.
- The gate button hides its tag ("Review needed", "Needs confirmation"); the action bar text still states it.

**Second pass (2026-10-21):** plan approval (4.2), Run app (4.4), Start dialog additions (4.5), Settings (4.6), and board, sidebar, notifications (4.7) are in. The whole loop starts from `board.html?start=PGS-1426` (PGS-1426 is now a To Do task); preview links with `?ac=`, `?app=`, `?gate=`, or `?phase=` show it mid-run and keep their changes in memory.

**Run app rules**

- One run's app at a time. The title bar shows the owner (App · PGS-1426, with state). Starting another run's app asks first and names the run whose app stops; that run then shows "Stopped: … started its app".
- The confirmation lists changed files per repo (since the base branch on the first run), flags files that run at start (package.json, lockfiles, Cargo files, scripts, config), and shows the `.env` keys that will be passed as checkboxes. Values are never shown.
- After the agent changes code, a running app becomes *Running older code*. The readiness dialog then offers Restart app, which goes through the same confirmation and returns to the checklist.
- Database and login are the developer's confirmations; they are remembered while the dialog is reopened in the same session.

**Plan approval rules**

- Missing and Partly rows are informational: the agent still builds them. Follow design is offered on every gap row, with Undo until approval.
- Approval writes the gaps to the Lark task; the Task tab shows the field content. Follow design rows become *Follows design* in the AC ledger and leave the gate.

**Prototype limits**

- Brainstorm and plan are simulated only on the kelola-v2 task (PGS-1426). Other tasks start implementing directly whatever Before coding says.
- The Start dialog's design source choice does not change the PGS-1426 ledger; use `?ac=nodesign` to see a run without a source.
- PGS-1429, PGS-1431, and PGS-1433 carry fixture AC summaries for the board and sidebar; their workspaces do not have an AC review tab.
