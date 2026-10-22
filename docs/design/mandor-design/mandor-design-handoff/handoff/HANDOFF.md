# Mandor design handoff

Start here in the implementation session. This package holds the full Mandor desktop design (Release 1: Phase 1 + Phase 2) as a clickable HTML prototype, plus everything needed to build it in **shadcn-svelte + Tailwind CSS v4 + Lucide** inside the Tauri app.

Status on 2026-10-19: all screens from the design prompt sections 7.1 to 7.11 are designed. The prototype passed automated checks (no JS errors on 44 frames, onboarding click-through) but **has not been reviewed visually yet**. Treat spacing and the 960 layout as "check while building".

## 1. What is in the package

```
mandor-design-handoff/
  handoff/
    HANDOFF.md            this file
    theme.css             paste-ready shadcn-svelte theme (Tailwind v4), dark + light
    icons.txt             every Lucide icon the design uses (105)
  prototype/              open prototype/index.html in a browser; works offline
    index.html            launcher with links to every screen and state
    frames.html           every screen and state as a 1440 or 960 window, dark/light
    components.html       component sheet: shadcn components + Mandor components
    board.html, workspace.html, settings.html, connection.html,
    onboarding.html, notifications.html, tray.html, oauth-callback.html, frame.html
    css/mandor.css        tokens + component styles (source of truth for values)
    js/                   prototype behaviour and fixtures (not production code)
    vendor/               Lucide 0.469.0 + Space Grotesk / JetBrains Mono woff2
  docs/
    brand-spec.md         token table, contrast results, posture rules
    design-notes.md       spacing, sizes, layout regions, motion, a11y, notifications
    mandor-screens-prompt.md          the original design brief
    korlap-lark-lembur-plan.md        product + planning contract (U12, U13 = UI units)
```

The prototype's JS is plain DOM code for demonstration. Port the **markup structure, classes, copy, and states**, not the JS.

## 2. Setup in the Svelte app

1. Copy `handoff/theme.css` into `src/app.css` (it already contains `@import "tailwindcss"`). Values are generated from `prototype/css/mandor.css`, so they match the prototype exactly.
2. Dark is the default: render `<html class="dark">`. Settings > General > Theme switches between system / dark / light.
3. Fonts: `npm i @fontsource-variable/space-grotesk @fontsource-variable/jetbrains-mono` and import both in the root layout. (Offline alternative: `prototype/vendor/fonts/`.)
4. Icons: `@lucide/svelte`. Names in `handoff/icons.txt` are kebab-case; the Svelte components are PascalCase (`git-pull-request-arrow` → `GitPullRequestArrow`). Stroke width 1.75, size 15 (13 in dense rows).
5. Tauri window: `minWidth: 960, minHeight: 680`; default 1440 × 900. Compact layout applies at ≤ 1100px.

## 3. Decisions already made

| Topic | Decision |
|---|---|
| Window minimum | 960 × 680 (replaces 1024 × 680 in the brief) so half of a 1920 screen works |
| Tokens | Brief values, except `--muted-foreground` `#988b74` and failed/destructive `#e08a80` / `#a8463e`, raised for WCAG AA |
| Primary (amber) | One primary action per view + the focus ring. Everything else secondary, outline, or ghost |
| Status | Never color alone: every run state is icon + label; "Waiting for you" is the only filled chip |
| Kanban | No drag and drop. `draggable=false`, no handles; a drag attempt shows a toast |
| Notifications | One per event; none if that task's workspace is focused; 3+ "waiting" in 10s grouped; no action buttons; PR created off by default. Accepted 2026-10-19 |
| Tray | Menu: Open Mandor · up to 5 waiting runs · Quit (confirm only while runs are active) |
| Later releases | AC review (R2) and AI Lembur (R3) only have reserved slots, see components.html › Slots for later releases |

## 4. Screens → prototype → Svelte

Component file names follow the plan (U12, U13). Open any prototype URL from `prototype/index.html`.

| Screen (brief §) | Prototype | Svelte (plan) | shadcn-svelte parts |
|---|---|---|---|
| App shell: title bar, Active agents sidebar | every app page | `lib/components/agents/AgentPanel.svelte` + shell layout | Sidebar (collapsible=icon), Button, Badge, Popover, Command |
| Daemon connection (7.1) | `connection.html`, `?state=unreachable`, `?state=mismatch` | connection gate in root layout | Alert, Button |
| Onboarding (7.2) | `onboarding.html?step=1..6`, `&login=waiting/ok/port/state/denied/timeout` | onboarding route | Card, Input, Label, Checkbox, Select, Alert, Button, Collapsible |
| Board (7.3) | `board.html`, `?lark=expired`, `?lark=loggedout`, `?sync=error` | `kanban/Board.svelte`, `kanban/Card.svelte` | Card, Badge, Button, Select, Input, ToggleGroup, DropdownMenu, Skeleton, Alert |
| Start dialog (7.4) | `board.html?start=PGS-1402`, `?start=PGS-1409` (claim lost) | StartDialog | Dialog, Select, Textarea, Button + StepProgress |
| Workspace (7.5) | `workspace.html?task=PGS-1391` (+ `&tab=changes/task/comments`, `&open=push`) | `workspace/Chat.svelte`, `DiffView.svelte`, `PrBar.svelte` | Tabs, ScrollArea, Alert, AlertDialog, Resizable/Sheet, Collapsible |
| Start fix (7.6) | `workspace.html?task=PGS-1366&open=fix` | `workspace/StartFixDialog.svelte` | Dialog, Textarea, Checkbox, Badge |
| Take over (7.7) | `board.html?takeover=PGS-1390` | TakeOverDialog | Dialog, Select, Alert |
| Settings (7.8) | `settings.html#lark/#repos/#general/#about`, `?settings=error`, `?loading=hold` | `settings/LarkLogin.svelte`, `settings/RepoSettings.svelte` | Card, Input, Table, Switch, RadioGroup, AlertDialog, Dialog |
| Notifications (7.9) | bell in title bar, `board.html?notif=open`, `notifications.html` | NotificationCenter + Tauri notification plugin | Popover, Button, Badge |
| Tray (7.10) | `tray.html` | Tauri tray menu (native) | n/a (native menu) |
| R2 · AC review tab (4.1) | `workspace.html?task=PGS-1426&tab=ac&ac=…` (states listed in `index.html`) | `workspace/AcReviewTab.svelte`, `AcRow.svelte`, `ReviewHistory.svelte` | Tabs, Checkbox, Collapsible, Alert, Badge, Button |
| R2 · Plan tab, design gaps (4.2) | `?task=PGS-1426&phase=plan&tab=plan`, `&phase=implementing&tab=task` | Plan tab + approve dialog | Alert, AlertDialog, Button, Badge |
| R2 · Gate (4.3) | `&gate=blocked/readiness/manual/anyway/warn`, `&acsync=failed` | `workspace/GateDialogs.svelte` | Popover, Dialog, AlertDialog, Checkbox, Textarea, Collapsible |
| R2 · Run app (4.4) | `&app=off/setup/starting/port/running/older/failed/stopped/other`, `&open=runapp`, `&logs=open` | Run app control, confirmation, logs drawer | Button, Dialog, Checkbox, Sheet (bottom), ToggleGroup |
| R2 · Start dialog (4.5) | `board.html?start=PGS-1426`, `?start=PGS-1402&sources=empty` | StartDialog additions | ToggleGroup, RadioGroup, Select, Checkbox, Input, Alert |
| R2 · Settings (4.6) | `settings.html#design`, `#workspaces`, `#repos` (kelola-web), `#general`, `#about` | Design sources, Workspaces, repo detail | Card, Input (password for Figma token), RadioGroup, ToggleGroup, Switch, Alert |
| R2 · Board, sidebar, notifications (4.7) | `board.html` (PGS-1429, 1431, 1433), `workspace.html?task=PGS-1431`, `notifications.html` | Card meta, AgentRow labels, notifications | Badge, Alert |
| OAuth callback (7.11) | `oauth-callback.html?status=ok` / `?status=error&reason=…` | static page served by mandord | none: self-contained HTML, ship as-is |

Useful run states for building: PGS-1391 waiting for you · PGS-1388 working · PGS-1384 creating PR · PGS-1371 quiet period · PGS-1366 waiting for reviewer comments · PGS-1362 fixing · PGS-1379 failed · PGS-1376 parked · PGS-1393 sync failed · PGS-1395 overwritten · PGS-1359 deleted · PGS-1385 unassigned · PGS-1368 unknown status.

## 5. Mandor components (not in shadcn)

Build from shadcn primitives and the tokens. Layer names in the prototype markup are in `data-ui` attributes (e.g. `data-ui="RunStatusBadge / state=awaiting_dev"`). Reference renderers:

| Component | Prototype source | Notes |
|---|---|---|
| RunStatusBadge | `js/shell.js` `runBadge`, states in `js/data.js` `runStates` | icon + label per state; `awaiting_dev` filled chip |
| CardFlagBadge | `js/shell.js` `flagBadge` | overwritten, sync_failed (Retry), deleted, unassigned (Close run) |
| TaskCard | `js/components.js` `taskCard` | 18 variants, see components.html › TaskCard |
| AgentRow | `js/shell.js` `agentRow` | groups: Waiting for you (pinned) · Needs attention · Working · Waiting |
| ChatMessage | `js/components.js` `chatMessage` | developer, agent (markdown, sanitise with DOMPurify), system, turn finished/failed, reviewer comment |
| ToolCallGroup | `js/components.js` `toolGroup` | collapsed summary; error and denied open by default |
| DiffViewer | `js/workspace.js` `renderChanges` | file list with A/M/D/R/U letters + unified diff with line numbers |
| Composer | `js/workspace.js` `renderComposer` | Send → Queue while working, Send now, Stop, queued chips, draft per task |
| StepProgress | `js/board.js` `stepsHtml` | todo / active / done / error with inline error |
| CommentItem + QuietPeriod | `js/workspace.js` `renderComments` | trusted badge, sent state, countdown |
| Stepper | `js/onboarding.js` `stepper` | Badge + Separator |
| NotificationCenter | `js/shell.js` `notifCenterHtml` | unread tint + dot, Mark all as read |
| ACLedger, ACLedgerRow | `js/ac.js` `UI.ac.row`, `removedRow` | Release 2. Design and Verdict chips, stale, new/changed, selected, fix in progress, removed |
| DesignChip, VerdictChip | `js/ac.js` `UI.ac.chip` | values in `UI.ac.DESIGN` / `UI.ac.VERDICT` |
| ACReviewSummary | `js/ac.js` `UI.ac.summary` | counts, gate state, last review, Run review |
| GateButton + gate dialogs | `js/ac.js` `UI.ac.gateButton`, `ACTAB` | gate logic in `UI.ac.gate` maps to `lib/ac-review.ts` |
| RunAppControl, ReadinessItem, DesignSourceItem | `js/ac.js` `runApp`, `readyItem`, `sourceItem` | 2.1 source kinds carry a dashed border and a 2.1 tag |
| Run app flow, AppLogs, AppOwner | `js/runapp.js` `RUNAPP.mount`, `js/shell.js` `appIndicator` | one app at a time; state in `MANDOR.getApp/setApp` |
| PlanTab + DesignGapRow | `js/plan.js` `PLAN.mount` | Follow design with Undo until approval |
| ACSummary chip | `js/components.js` `acChip` | `AC 5/7`, `2 failed`, `Review stale`, `Reviewing AC`, design-gap marker |

## 6. Where the details live

- Exact values and contrast: `docs/brand-spec.md`.
- Spacing scale, control heights, layout regions at 1440 and 960, motion timings, keyboard map, ARIA semantics, notification copy table: `docs/design-notes.md`.
- Every component with every variant, dark/light side by side: `prototype/components.html`.
- Every screen and state as a real window: `prototype/frames.html`.

## 7. Not verified yet

- Visual review of all frames (spacing, overflow, truncation, the 960 layout). Only automated DOM checks were run.
- Light theme contrast was computed per token pair, not checked on rendered screens.
- Native surfaces (desktop notifications, tray menu) are mockups; the OS draws the real ones.

## 8. Lucide icons used

`app-window` · `arrow-left` · `arrow-left-right` · `arrow-right` · `arrow-right-left` · `ban` · `bell` · `bell-off` · `book-open` · `bot` · `bot-off` · `check` · `chevron-down` · `chevron-right` · `circle` · `circle-alert` · `circle-check` · `circle-dashed` · `circle-dot` · `circle-help` · `circle-minus` · `circle-pause` · `circle-slash` · `circle-x` · `clipboard-check` · `clock` · `code` · `component` · `contrast` · `copy` · `database` · `ellipsis` · `external-link` · `eye` · `file-check` · `file-code` · `file-code-2` · `file-diff` · `file-image` · `file-pen` · `file-plus` · `file-text` · `folder-code` · `folder-git-2` · `folder-open` · `folder-plus` · `folder-search` · `folder-x` · `folders` · `forward` · `frame` · `git-branch` · `git-commit-horizontal` · `git-compare` · `git-pull-request` · `git-pull-request-arrow` · `globe` · `hammer` · `hand` · `history` · `info` · `layout-grid` · `lightbulb` · `link-2` · `list-checks` · `list-ordered` · `list-plus` · `list-tree` · `list-x` · `loader` · `loader-circle` · `lock` · `log-in` · `megaphone` · `message-circle-question` · `message-square-dashed` · `message-square-dot` · `message-square-text` · `messages-square` · `minus` · `moon` · `moon-star` · `notebook-pen` · `octagon-x` · `palette` · `panel-bottom` · `panel-left-close` · `panel-right` · `panel-right-open` · `pen-tool` · `play` · `plug-zap` · `plus` · `power` · `refresh-cw` · `refresh-cw-off` · `rotate-ccw` · `ruler` · `scan-search` · `scroll-text` · `search` · `search-x` · `send-horizontal` · `server-crash` · `settings` · `settings-2` · `shield-alert` · `shield-check` · `shield-question` · `signpost-big` · `sliders-horizontal` · `sparkles` · `square` · `square-kanban` · `square-parking` · `square-terminal` · `sun` · `sunrise` · `text-search` · `timer` · `trash-2` · `triangle-alert` · `undo-2` · `unplug` · `upload` · `user-round-check` · `user-round-x` · `user-x` · `wifi-off` · `wrench` · `x` · `zap`
