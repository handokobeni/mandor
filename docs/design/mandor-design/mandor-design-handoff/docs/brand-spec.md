# Mandor brand spec

Source: `mvdwy6oy-mandor-screens-prompt.md` section 4.2 (Tokens) and 4.4. The brief's amber values are bound onto the stock shadcn theme variable names so the handoff to shadcn-svelte + Tailwind is 1:1. This spec overrides the generic shadcn neutral palette.

## Theme variables (shadcn names)

| Variable | Dark (default) | Light | Note |
|---|---|---|---|
| `--background` | `#12110e` | `#f6f3ee` | from brief / derived |
| `--foreground` | `#d4c5a9` | `#2a231a` | 11.1:1 / 14.0:1 |
| `--card` / `--card-foreground` | `#1a1814` / `#d4c5a9` | `#ffffff` / `#2a231a` | |
| `--popover` / `--popover-foreground` | `#1a1611` / `#d4c5a9` | `#ffffff` / `#2a231a` | |
| `--primary` / `--primary-foreground` | `#c8a97e` / `#12110e` | `#7d5c2e` / `#fffaf2` | 8.5:1 / 5.9:1 |
| `--secondary` / `--secondary-foreground` | `#2a2520` / `#d4c5a9` | `#ede7dd` / `#2a231a` | |
| `--muted` / `--muted-foreground` | `#1e1b17` / `#988b74` | `#efe9df` / `#6b5f4b` | brief `#8a7e6a` was 4.45:1 on card, raised to 5.3:1 |
| `--accent` / `--accent-foreground` | `#2a2520` / `#e6dac3` | `#ebe4d8` / `#2a231a` | hover / selected rows |
| `--destructive` | `#a8463e` | `#b23a31` | brief `#b5564e` as text was 3.7:1, darkened for white label |
| `--border` / `--input` | `#3a3530` | `#ddd4c5` | |
| `--ring` | `#c8a97e` | `#7d5c2e` | |
| `--sidebar` (+ `-foreground`, `-accent`, `-border`) | `#0f0d0a` | `#efeae1` | agents sidebar |

## Mandor status tokens

Every status token is used with an icon and a text label. Color alone never carries the state.

| Token | Dark | Light | States |
|---|---|---|---|
| `--status-working` | `#7fb2bd` | `#22677a` | preparing, running, publishing, wf_running |
| `--status-attention` | `#e8b45c` (filled, text `#1a1406`) | `#e3a63c` (filled, text `#2a1d04`) | awaiting_dev |
| `--status-waiting` | `#b3a5cf` | `#6a5296` | in_review_loop, wf_awaiting_comment, wf_quiet |
| `--status-failed` | `#e08a80` | `#b23a31` | failed, sync_failed, deleted |
| `--status-warning` | `#d9925a` | `#a3561c` | overwritten, unassigned, orphaned |
| `--status-ok` | `#8fb07a` | `#3f6a2a` | done, success steps |
| `--status-pr` | `#8e9cd4` | `#3f4fa0` | PR links |
| `--diff-add` / `--diff-add-bg` | `#8fb07a` / `#1a2a1a` | `#2f5e1f` / `#e6f2df` | |
| `--diff-del` / `--diff-del-bg` | `#e08f8f` / `#2a1a1a` | `#9b2c2c` / `#f8e3e1` | |

All pairs above were checked at 4.5:1 or better against `--card`.

**Decision (2026-10-19):** keep the two AA adjustments (`--muted-foreground` `#988b74`, failed/destructive text `#e08a80` and fill `#a8463e`) instead of the brief's `#8a7e6a` and `#b5564e`. Every other token matches the brief exactly.

## Type

- UI: `"Space Grotesk", ui-sans-serif, system-ui, sans-serif`. Body 13px, dense UI 12px, titles 15 / 18 / 22px. Weights 400 / 500 / 600.
- Mono: `"JetBrains Mono", ui-monospace, "SF Mono", Menlo, Consolas, monospace`. Task keys, branches, diffs, tool calls, paths.

## Posture

1. Radius `--radius: 6px` (shadcn default reduced). Badges are 4px, avatars round.
2. Hairline 1px `--border` everywhere. Shadow only on Dialog, Popover, Dropdown Menu, Command.
3. `--primary` is spent on one primary action per view plus the focus ring. Everything else is `secondary`, `outline`, or `ghost`.
4. Attention (awaiting_dev) is the only filled status chip; every other run state is a dot + label.
5. Density: `sm` controls (28px) in toolbars, 32px default, 12px card padding.
