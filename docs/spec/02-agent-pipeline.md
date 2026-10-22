# Korlap: Agent / LLM / MCP / LSP / Knowledge Pipeline, Reconstruction Spec

## 0. Big picture

```
Frontend (Svelte)                        Rust (Tauri)                                 External
sendMessage(ws, fullPrompt, Channel) --> send_message (agent.rs:164) --spawn--> claude -p ... stream-json (cwd=worktree)
  <-- Channel<AgentEvent> --------------- reader thread (agent_backend parse_stream_line)    |
  <-- emit "agent-status" running/waiting                                                    | spawns MCP stdio
                                         mcp_api.rs HTTP 127.0.0.1:<random>  <-- bun run src-mcp/server.ts
                                           /rename-branch /todos/* /lsp/*        (env KORLAP_API_PORT, KORLAP_WORKSPACE_ID)
                                           -> LspServerPool --stdio JSON-RPC--> rust-analyzer/tsserver/gopls/pyright
One-shot Haiku helpers: commit msg, suggest_replies, auto-rename, prioritize, deps, autopilot, check_invariants, draft_contradiction
Long-running KB agents (stream-json, --dangerously-skip-permissions, cwd=MAIN repo): build / post-merge / incremental
```

**State** (`state.rs`):
- `agents: HashMap<ws_id, AgentHandle{child}>` holds at most one agent per workspace.
- `session_ids` is persisted to `<data_dir>/sessions.json`. It is only written inside `save_workspaces()` and only when non-empty (state.rs:339-345).
- `context_agents: HashMap<repo_id, AgentHandle>`.
- `mcp_api_port` is set at startup (lib.rs:56-61).
- `LspServerPool` is a separate managed `Arc<Mutex>` (lib.rs:55,64).
- `RepoSettings` (state.rs:82-112): `pr_message`, `review_message`, `default_thinking`, `default_plan`, `system_prompt`, `lsp_servers`, `mcp_servers`, `agent_provider`.
- `McpServerConfig` (state.rs:117-136): `server_type` ("stdio" default or "sse"), `command`, `args`, `env`, `url`, `headers`.
- `ContextMeta` (state.rs:173-194) is stored in `context_meta.json`. Status `Building` is reset to `Failed` on restart (state.rs:302-307).
- `effective_provider = ws.provider_override.unwrap_or(settings.agent_provider)` (state.rs:66).

**Shell env** (helpers.rs:91-262):
- `inject_shell_env` applies every env var captured once from an interactive login shell (`$SHELL -l -i -c '... /usr/bin/env'`), plus a launchctl `SSH_AUTH_SOCK` fallback.
- `claude_path` and `codex_path` are resolved via `command -v`. Every spawn uses `claude_path.unwrap_or("claude")`.

**Git auth** (git_provider/github.rs:251-264):
- The token comes from `gh auth token --user <profile>` (github.rs:44-65).
- Spawned agents get `GH_TOKEN=<tok>` and `GIT_CONFIG_PARAMETERS='url.https://x-access-token:<tok>@github.com/.insteadOf=git@github.com:'`.

## 1. Agent spawning

### 1.1 `send_message` (agent.rs:163-523)
It is a sync command, so it runs on the main thread. Arguments: `workspace_id, prompt, on_event: Channel<AgentEvent>, plan_mode?, thinking_mode?, model?`.

1. Takes one lock to gather state. Errors with "Agent is already processing a message" if `st.agents` already has the workspace (agent.rs:195).
2. Resolves the token and builds the git auth env.
3. Resolves the MCP script path (agent.rs:236-252). It tries the compile-time `env!("CARGO_MANIFEST_DIR")/../src-mcp/server.ts`, then `<data_dir>/mcp/server.ts`. Nothing ever writes the second path, so release builds have no korlap MCP server.
4. Calls `build_mcp_server_map` (§4.1).
5. Builds the system prompt only when there is no session id yet (agent.rs:264).
6. Triggers auto-rename on a new session when the branch is not custom (agent.rs:279-347).
7. Builds `SessionContext` with `images_dir=<data_dir>/images` and `disallowed_tools="EnterWorktree,ExitWorktree,LSP"` (agent_backend.rs:166).
8. Spawns, stores the handle, sets status Running, saves, and emits `agent-status {workspace_id,status:"running"}` (agent.rs:319-335).
9. Starts the reader thread (§2.6).

**Claude command** (agent_backend.rs:169-240):
```
claude -p "<prompt>" --output-format stream-json --verbose
  plan: --permission-mode plan --allowedTools "mcp__korlap__rename_branch,WebSearch,WebFetch,mcp__korlap__lsp_goto_definition,mcp__korlap__lsp_find_references,mcp__korlap__lsp_hover,mcp__korlap__lsp_workspace_symbols,mcp__korlap__lsp_diagnostics"
  work: --permission-mode bypassPermissions --disallowedTools "EnterWorktree,ExitWorktree,LSP"
  --add-dir <data_dir>/images
  [--model <m>] (non-empty: sonnet|opus|haiku)   [--effort high] if thinking
  --resume <sid>  ELSE  --system-prompt "<prompt>"
  [--mcp-config <data_dir>/mcp/<ws_id>.json]  (rewritten each send: {"mcpServers":{...}})
cwd=worktree; env=shell env + GH auth
```
- Models offered: `""` (Default), `sonnet`, `opus`, `haiku`.
- `supports_thinking` and `supports_plan_mode` are both true.

**Codex command** (agent_backend.rs:506-586):
- If `codex_path` is missing, it errors with "Codex CLI not found. Install with: npm i -g @openai/codex".
- The system prompt goes to `<data_dir>/mcp/codex-data/<ws>/instructions.md` and is passed as `-c model_instructions_file=<path>`. CODEX_HOME is never overridden.
- New session: `codex exec --json --dangerously-bypass-approvals-and-sandbox [--model M] "<prompt>"`.
- Resume: `codex exec resume <sid> --json --dangerously-bypass-approvals-and-sandbox "<prompt>"`. The model choice is ignored on resume.
- Only the korlap MCP server is registered, and it goes into Codex's global config via `codex mcp add korlap_<ws[..8]>_korlap` (it runs `codex mcp remove` first).
  - stdio: `--env K=V ... -- cmd args`, with `bun run X` rewritten to `npx tsx X`.
  - sse: `--url`.
  - It is unregistered after the process exits (agent.rs:509-516).
- Models: `""`, `codex-mini`, `o3`, `o4-mini`. No thinking support. Plan mode is ignored by the builder.
- `switch_workspace_provider` (agent.rs:90-119) refuses while an agent is running and clears the session id.

### 1.2 One-shot helpers
All are `claude -p ... --output-format text`. Stdout is piped and stderr goes to null. They use `inject_shell_env` but do not get GH_TOKEN.

| Helper | file:line | Model | Extra flags | cwd | Timeout | On failure |
|---|---|---|---|---|---|---|
| generate_commit_message | agent.rs:712-833 | claude-haiku-4-5-20251001 | --max-turns 1, --disallowedTools EnterWorktree,ExitWorktree,LSP | worktree | 30s | Returns "chore: update files" |
| suggest_replies | agent.rs:835-898 | haiku 4.5 | --max-turns 1, --max-tokens 200 | none set | 30s, then kill -9 | Err |
| auto-rename | agent.rs:904-1017 | haiku 4.5 | --max-turns 1, --max-tokens 50, --system-prompt | none set | 15s | Logs a warning |
| prioritize_todos | agent.rs:1030-1096 | haiku 4.5 | --max-turns 1, --system-prompt | data_dir | 60s | Err |
| determine_dependencies | agent.rs:1098-1166 | haiku 4.5 | --max-turns 1, --system-prompt | data_dir | 60s | Err |
| interpret_autopilot_command | agent.rs:1176-1252 | haiku 4.5 | --max-turns 1, --system-prompt | data_dir | 60s | Unparsed reply becomes `{response: raw, action_type: "none"}` |
| check_invariants | context.rs:692-882 | precheck_model, default haiku 4.5 | --max-turns 1 | worktree | 30s | Fail-open (passes) |
| draft_contradiction_resolution | context.rs:1492-1640 | precheck_model | --max-turns 1 | none set | 60s | Err |
| mcp_oauth_start | oauth.rs | CLI default | --mcp-config tmp, --allowedTools "", stdin null | none set | 120s | Err |

- The timeout pattern is a thread running `wait_with_output` plus `mpsc::recv_timeout`; on timeout it runs `kill -9 <pid>`.
- Exception: on timeout, `generate_commit_message` only calls `drop(handle)`, so the process leaks (agent.rs:824-827).
- `strip_json_fences` (agent.rs:1022) and `strip_ansi` clean the output before parsing.

### 1.3 Knowledge-base agents
Used by build, post-merge update and incremental update (context.rs:542-551, 997-1004, 1291-1298):
```
claude -p "<prompt>" --output-format stream-json --verbose --dangerously-skip-permissions --add-dir <data_dir>/context/<repo_id>
```
- cwd is the **main repo**; env is the shell env plus GH auth.
- No model is set, there is no timeout, and no MCP server is attached.

### 1.4 Review
There is no separate review process. `triggerReview` (+page.svelte:2170-2275) calls the same `sendMessage` on the resumed chat session, with plan mode, thinking and model all at their defaults.

### 1.5 Models
- Chat: whatever the user picks.
- Review and KB agents: CLI default. The review prompt itself asks for haiku, sonnet and opus subagents.
- Utilities: Haiku 4.5, hard-coded.
- Invariant pre-check and contradiction drafting: the configurable `precheck_model` (UI default Haiku 4.5, RepoSettings.svelte:117).

## 2. Backend trait, parsing, events, lifecycle

**Trait** (agent_backend.rs:46-80):
- `build_session_command(&ctx) -> Result<(Command, Vec<String> cleanup_mcp_names), String>`
- `parse_stream_line(line, worktree) -> Option<ParsedEvent>`
- `list_models()`, `supports_thinking()`, `supports_plan_mode()`, `binary_name()`
- `get_backend(provider) -> &'static dyn` returns one of two unit structs.

**SessionContext** fields (agent_backend.rs:9-26): prompt, worktree_path, repo_path, session_id, plan_mode, thinking_mode, model, system_prompt, mcp_servers (JSON map), mcp_dir, workspace_id, images_dir, git_auth_env, disallowed_tools.

**ParsedEvent**: `SessionId(String)`, `AssistantMessage{text, tool_uses, thinking}`, `Usage{input_tokens, output_tokens, cumulative}`, `Done`.

Commands built on the trait: `list_models(repo_id?)`, `get_provider_info(repo_id)`, `get_workspace_provider_info(ws)`. The last two return `{provider, supports_thinking, supports_plan_mode, models}`.

**Wire type** (agent.rs:123-159, ipc.ts:64-68):
- `ToolUseInfo{name, input_preview?, file_path?, old_string?, new_string?}`
- `AgentEvent` is tagged by `type`: `assistant_message{text, tool_uses, thinking?}` | `usage{input_tokens, output_tokens, cumulative}` | `done` | `error{message}`. The `error` variant is never constructed.
- Events go over a per-invocation Channel (ipc.ts:398-409). Status goes over the global emit `agent-status`.

**Claude parsing** (agent_backend.rs:242-382):
- `system` → `SessionId`.
- `assistant` → walks `message.content`:
  - `text` blocks are joined with "\n".
  - `tool_use` blocks: `file_path` has the worktree prefix stripped (`<wt>/` → `./`). `input_preview` is chosen in this order (agent_backend.rs:444-499):
    1. AskUserQuestion: `JSON(input.questions)`.
    2. TodoWrite: `JSON(input.todos)`.
    3. `file_path`.
    4. `command`, with `cd ./ && `-style prefixes stripped, capped at 120 chars.
    5. `pattern`.
    6. `query` (120 chars).
    7. `description` (120 chars).
    8. `skill`.
    9. `url` (120 chars).
    10. Otherwise the first string value under 200 chars.
  - `Edit` tool uses also fill `old_string` / `new_string`.
  - For `ExitPlanMode`, `input.plan` is appended to the text.
  - `thinking` blocks are joined.
  - If a message has no content but has usage, it becomes `Usage{cumulative:false}`.
- `result` → `Done`.
- Everything else is ignored, so tool results are never shown.

**Second pass in the reader** (agent.rs:397-449):
- Claude assistant `message.usage` → `Usage{cumulative:false}`.
- Claude `result` → captures `session_id`, then sends `Usage{cumulative:true}` (this arrives after `Done`).
- Codex `turn.completed` → `Usage{cumulative:true}`.
- `claude_extract_usage` sums `input_tokens + cache_creation_input_tokens + cache_read_input_tokens` as input, and uses `output_tokens` as output.

**Codex parsing** (agent_backend.rs:588-713):
- `thread.started` → `SessionId(thread_id)`.
- `item.completed`:
  - `agent_message` → text.
  - `command_execution` → tool "shell", with the `/bin/zsh -lc "..."` (or bash) wrapper stripped.
  - `mcp_tool_call` → tool `<tool>` with its first short argument as preview; on error, text "MCP s.t failed: msg".
- `turn.completed` and `turn.failed` → `Done`. Usage is `input_tokens + cached_input_tokens` and `output_tokens`.

**Token accounting** (stores/messages.svelte.ts:36-62):
- Non-cumulative usage adds to `turnTokens`.
- Cumulative usage adds to `completedTokens` and deletes `turnTokens`.
- The display value is the sum of both.

**End of stream** (agent.rs:459-519):
1. Reads stderr. If no events were produced at all, sends a synthetic message ("claude error: <stderr>" or "...exited without producing any output") followed by `Done`.
2. Under the lock: removes from `agents` and calls `child.wait()`, stores the session id, sets status Waiting, and calls `save_workspaces` (which persists the session).
3. Emits `waiting`.
4. Runs Codex MCP cleanup.

**Frontend side** (+page.svelte:607-622, 1715-1800):
- On `done`: clears the sending flag, refreshes diff, changes and PR status, refreshes suggestions, sets `pendingDrain`, and re-runs the autopilot evaluation.
- On `agent-status: waiting`: clears sending; if `pendingDrain` is set, sends the next queued message.
- `running` is suppressed while a review is in progress.
- Per-workspace queue: `routeMessage` queues while busy. "Send now" calls `stopAgent` then `sendDirect`.
- On restart, Running is reset to Waiting (state.rs:250).

**Stop** (agent.rs:647-676):
- Idempotent: `kill()` (SIGKILL), wait, set Waiting, emit `waiting`.
- Only the `claude` process is killed; the `bun` MCP child and Bash child processes are left running.
- `stop_context_build` (context.rs:648-674) reverts `Building` to `Built` or `NotBuilt`, and its reader then returns without sending `Done`.

**Plan mode** is chosen per message. "Execute plan" (ChatPanel.svelte:592-604 → +page.svelte:2636) sends "Execute the plan above. Do not ask for confirmation — just do it." in work mode on the same session.

## 3. Prompts (verbatim)

### 3.1 System prompt (agent.rs:526-645)

```
You are a coding agent inside Korlap, working in a git worktree.
Worktree: {wt} | Main repo: {repo} (DO NOT access)
Target branch: {ws_branch} | Base branch: {base_branch}

ISOLATION: All file operations MUST stay under {wt}. EnterWorktree/ExitWorktree are DISABLED. Do not use Agent with isolation:"worktree". The .git file referencing the main repo is normal for worktrees — ignore it.

{rename_instruction}
Keep all changes on target branch.

LSP tools available (prefer over grep for precise navigation, 1-based positions):
lsp_goto_definition, lsp_find_references, lsp_hover, lsp_workspace_symbols, lsp_diagnostics, lsp_rename
After edits, call lsp_diagnostics. For renames, prefer lsp_rename over find-and-replace.

TOKEN EFFICIENCY — minimize tool output to save context:
• Bash: pipe verbose commands through `head -80` or `tail -40` (e.g. `cargo check 2>&1 | head -80`)
• Read: use offset/limit to read only the section you need, not entire files
• Grep: set head_limit to cap results (e.g. 20). Never run unbounded searches.
• Git: use `git diff --stat` first; only read full diff for files you need to change
• Avoid re-reading files you already have in context
```

- `base_branch` is `source_pr.base_branch`, else `detect_default_branch`, else "main".
- `rename_instruction` is either "Branch will be auto-renamed based on your task. If the auto-name is wrong, call rename_branch to fix it." or "Branch was manually named. Do NOT rename unless explicitly asked."

Warm context is appended only if the context dir exists, with a 20,000-character budget:
1. `## Repository Invariants (MUST follow)` + invariants.md.
2. hot.md.
3. `## Repository Facts` + the first 80 lines of facts.md.
4. `## Relevant Context`: entries from context.md whose id matches an index.md `## File affinity` line (`glob → id`). The glob base (trailing `*` and `/` trimmed) must appear as a substring of the user prompt (`find_relevant_context_ids`, agent.rs:682; `extract_entries_by_id`, context.rs:59).

PR context, added when the workspace came from a PR:
```
## PR Review Context

You are reviewing PR #{n}: {title}
PR URL: {url}
PR branch: {branch} → {base}

This workspace contains the PR's code branched from its HEAD.
Your task is to review, test, and identify issues in this PR.
You can make changes, add tests, and fix issues. Commits stay on your review branch and will NOT automatically update the PR.
```

Then `\n\nUser preferences:\n{settings.system_prompt}`.

### 3.2 Branch namer (agent.rs:918-924)
```
You are a branch name generator. Given a task description, output ONLY a git branch name.
Rules:
- Use conventional prefix: feat/, fix/, refactor/, chore/, docs/
- Use kebab-case after the prefix
- Keep it concise: max 5 words after prefix
- No quotes, no explanation, just the branch name
Examples: feat/add-auth-middleware, fix/login-redirect, refactor/split-utils
```
- The input is the first chat prompt.
- The output is trimmed of quotes and backticks and must contain `/` and be at most 60 characters.
- Then `rename_git_branch` (state.rs:441-470: `git branch --show-current`, then `branch -m`), state is updated, and `workspace-updated` is emitted.

### 3.3 Commit message (agent.rs:764-779)
- First runs `git add -A`, then `git diff --cached`. If the diff is over 50,000 bytes it is replaced by "[Diff too large for full context. Summary:]\n" plus `--stat`. The log comes from `git log --oneline origin/<base>..HEAD`.
- Without prior commits: "Write a conventional commit message for this diff. First line under 72 chars. Use conventional commit format (feat:, fix:, chore:, etc). Output ONLY the commit message, nothing else.\n\n{diff}"
- With prior commits: "Write a conventional commit message for the latest staged changes. First line under 72 chars. Use conventional commit format (feat:, fix:, chore:, etc). Output ONLY the commit message, nothing else.\n\nPrevious commits on this branch:\n{log}\n\nStaged diff:\n{diff}"

### 3.4 suggest_replies (agent.rs:838)
"Given this AI assistant message, suggest 2-4 short reply options that a user would likely want to send back. Return ONLY a JSON array of short strings, nothing else. Example: [\"Yes\", \"No, skip this\"]\n\nMessage:\n{text}"

- The UI only calls it when the last assistant text has a line ending in "?".
- A fast-path regex shows Yes/No immediately: `CONFIRM_RE=/\b(?:want me to|should i|shall i|would you like|do you want|can i|may i|like me to|ready to|proceed|go ahead|continue|fix (?:th|it|them))\b/i` (ChatPanel.svelte:247).
- Autopilot sends `replies[0]`, or "Yes, proceed." if the call fails (+page.svelte:1291-1312).

### 3.5 Autopilot prompts
- **prioritize** (agent.rs:1041): "You are a task scheduler. Analyze these TODO items and return them ordered by priority, considering dependencies. Return ONLY a JSON array of the todo IDs in execution order. Example: [\"id1\", \"id2\", \"id3\"]". The user message is JSON `[{id,title,description,depends_on}]`.
- **dependencies** (agent.rs:1109): "You are a task dependency analyzer for a software project. Given a list of TODO items (each with id, title, description), determine which tasks depend on other tasks. Task A depends on task B if A requires B's code changes to exist first — e.g., \"write API tests\" depends on \"build the API endpoint\". Only include direct dependencies, not transitive ones. Return ONLY a JSON object mapping task IDs to arrays of dependency task IDs. Only include entries for tasks that have dependencies. Example: {\"id2\": [\"id1\"], \"id3\": [\"id1\"]}. If no dependencies exist, return: {}"
- **autopilot command** (agent.rs:1188): "You are an autopilot orchestrator for a coding agent system. The user may ask questions about workspace status OR give commands to manage automated TODO pickup. Available action_types: 'pause', 'resume', 'skip_todo', 'prioritize', 'none'. Use 'none' for status questions, informational queries, or anything that doesn't require an action. You MUST ALWAYS respond with ONLY valid JSON — no markdown, no explanation outside JSON: {\"response\": \"<human-readable reply>\", \"action_type\": \"<type>\", \"todo_ids\": [...], \"reorder\": [...]}".
  - The user message is "Command: {cmd}\n\nCurrent state:\n{json}". The JSON holds todos, workspaces (with sending/review state, changes, planMode), prStatuses, autopilotEnabled, activeAgentCount and maxConcurrentAgents (+page.svelte:1474-1493).

### 3.6 Review prompt (src/lib/review-prompt.ts)
It is a near-copy of Anthropic's `/code-review` plugin, adapted to a local branch diff.
- **Output contract:** produce no text until step 8; output `[CLEAN]\nNo issues found.` or `[ISSUES]` followed by the list.
- **Diff:** `MERGE_BASE=$(git merge-base origin/{{base_branch}} HEAD)`, then `git diff $MERGE_BASE [--name-only]`, reviewed as one unified diff.
- **Steps:**
  1. A haiku agent lists the CLAUDE.md paths.
  2. Read the PR title and description.
  3. A sonnet agent summarizes the changes.
  4. Four parallel reviewers: two sonnet agents check CLAUDE.md/AGENTS.md compliance (scoped by path); opus bug agent 3 looks at the diff only for significant bugs; opus agent 4 looks for security and logic problems in the introduced code. Only HIGH SIGNAL issues are reported.
  5. Validate each issue in parallel (opus for bugs, sonnet for CLAUDE.md).
  6. Filter.
  7. Post one inline comment per issue via `gh api`.
  8. Output the list as `### **#N Title**\n\ndesc\n\nFile: path`.
- It also includes a false-positive list, notes (subagents must not post, no AskUserQuestion, gh CLI not web fetch, cite rules) and fallbacks.
- **Frontend substitution** (+page.svelte:2219-2229): appends `## Additional Instructions\n\n{review_message}`, then replaces `{{branch}}`, `{{base_branch}}`, `{{pr_number}}` and `{{pr_title}}` ("N/A" when there is no open PR).
- **Result classification** (`resultKind`, +page.svelte:1180): a `**Review failed:**` prefix means failed; starting with `[CLEAN]` or containing "no issues found" means clean; anything else means issues. The result is the last non-empty assistant text.

### 3.7 Other frontend prompts (+page.svelte)
- **Execute plan** (1288, 2638): "Execute the plan above. Do not ask for confirmation — just do it."
- **Autopilot review fix** (1339): "The code review found these issues. Fix them all:\n\n{md}"
- **Manual review fix** (2655): "Address all issues from this code review:\n\n{md}"
- **Invariant violations** (2199, autopilot only): "These invariant violations were found in your changes:\n\n- INV: desc (file:line)\n\nFix all of them before proceeding."
- **PR conflicts** (1992): "PR #{n} has merge conflicts with {base}.\n\nResolve them:\n1. Run `git fetch origin {base}`\n2. Run `git merge origin/{base}`\n3. Resolve all conflicts\n4. Commit the merge\n5. Push\n\nIf the conflicts are complex, explain what's conflicting before resolving."
- **Failing checks** (1996): "PR #{n} has failing checks. Investigate the failures using `gh pr checks {n}`, fix the issues, commit, and push."
- **Git error** (2018, 2035, 2065, 2094): "This git operation failed:\n\n```\n{e}\n```\n\nDiagnose the issue and fix it."
- **Update-from-base conflict** (2307): "Updating from {base} caused merge conflicts. The automatic merge was aborted.\n\nPlease resolve this:\n1. Run `git fetch origin {base}`\n2. Run `git merge origin/{base}`\n3. Resolve all conflicts\n4. Commit the merge\n\nIf the conflicts are complex, explain what's conflicting before resolving."
- **Create PR, default** (2104-2120): sent after Korlap has already committed with the Haiku message and pushed. "The code is already committed and pushed to origin.\n\nCreate a pull request using `gh pr create --base {base}`.\nKeep the title under 80 characters. Keep the description under five sentences unless there's a template.\nBase your PR title and description on what we've been working on in this conversation.\n" plus an optional "## PR Description Template\n\nThis repo has a PR template. Use it:\n\n```markdown\n{t}\n```".
- **Create PR, custom**: the user's `pr_message`, with `{{branch}}`, `{{base_branch}}`, `{{file_count}}` and `{{pr_template}}` substituted.
- **Todo start** (1572-1611): "{title}\n\n{description}" plus "\n\nReferenced files:\n@path". Image instructions are prepended: "I've attached an image. Read it using the Read tool:\n{path}" (or "I've attached N images. Read each using the Read tool:"). It is sent as a hidden message.
- **Chat mentions** (1890-1946): files become `<file path="P" lines="N"[ focus_line="L"] source="mention">\n{content}\n</file>` (or "(could not read file — use Read tool to access)"); folders become `<folder path="P" />`. Both are prepended. Images are saved under `<data_dir>/images/<ws>/`.

## 4. MCP

### 4.1 `build_mcp_server_map` (agent_backend.rs:86-150)
```json
"korlap": {"type":"stdio","command":"bun","args":["run","<abs server.ts>"],"env":{"KORLAP_API_PORT":"<port>","KORLAP_WORKSPACE_ID":"<ws>"}}
```
- User servers are added only when not in plan mode, and never under the name "korlap":
  - sse: `{"type":"sse","url","headers"}`, skipped if the URL is empty.
  - stdio: `{"type":"stdio","command","args","env"}`, skipped if the command is empty.
- There is no "http" transport type.
- `bun` is found via the PATH in the injected env.
- server.ts needs the repo's node_modules (`@modelcontextprotocol/sdk` ^1.27.1, `zod` ^4.3.6).

### 4.2 Tools in server.ts
`McpServer{name:"korlap", version:"0.1.0"}` over stdio. It exits with code 1 if the env vars are missing. `workspace_id` always comes from env.

| Tool | Input | HTTP call |
|---|---|---|
| rename_branch | new_name: string | POST /rename-branch |
| get_workspace_info | {} | GET /workspace-info?workspace_id= |
| notify | message, level?: info\|warn\|error | POST /notify |
| create_todo | title, description? | POST /todos/create |
| update_todo | todo_id, title?, description? | POST /todos/update |
| delete_todo | todo_id | POST /todos/delete |
| list_todos | {} | GET /todos/list?workspace_id= |
| lsp_goto_definition | file_path, line int>0, character int>0 (1-based) | POST /lsp/goto-definition |
| lsp_find_references | file_path, line, character | POST /lsp/references |
| lsp_hover | file_path, line, character | POST /lsp/hover |
| lsp_workspace_symbols | query | POST /lsp/workspace-symbols |
| lsp_diagnostics | file_path | POST /lsp/diagnostics |
| lsp_rename | file_path, line, character, new_name | POST /lsp/rename |

- LSP tools return `data.text`, or `isError` with "LSP error: {json}".
- `list_todos` formats each todo as "N. [id] title\n   desc".

### 4.3 HTTP API (mcp_api.rs)
- `bind 127.0.0.1:0` uses `expect`, so it panics on failure. One thread per connection.
- Hand-written HTTP/1.1 parsing. `Content-Length` is matched only as exactly `Content-Length: ` or `content-length: `.
- Responses are JSON with `Connection: close`. There is no authentication.

| Route | Request body | Response | Side effects |
|---|---|---|---|
| POST /rename-branch | {workspace_id, new_name} | {"ok":true,"branch","name"} | Runs `git branch -m` while holding the AppState lock, saves, emits `workspace-updated` |
| GET /workspace-info | query workspace_id | WorkspaceInfo JSON | none |
| POST /notify | any JSON | {"ok":true} | Emits `agent-notify`; no frontend listener exists |
| POST /lsp/goto-definition | {ws, file_path, line, character} (default 1) | {"text"} | May start the server; emits `lsp-status` starting/indexing/ready/error |
| POST /lsp/references | same | {"text"} | includeDeclaration=true |
| POST /lsp/hover | same | {"text"} | |
| POST /lsp/workspace-symbols | {ws, query} | {"text"} | Starts all detected servers and merges results (top 30) |
| POST /lsp/diagnostics | {ws, file_path} | {"text"} | |
| POST /lsp/rename | {ws, file, line, char, new_name} | {"text":"Renamed across N files (M edits):..."} or "Symbol cannot be renamed at this position." | Writes the edits to disk |
| POST /todos/create | {ws, title, description?} | {"ok":true,"id":uuid} | Appends `{id, repo_id, title, description, created_at}` to `<data_dir>/todos/<repo>.json`; emits `todos-changed {repo_id}` |
| POST /todos/update | {ws, todo_id, title?, description?} | {"ok":true} | Partial update; emits `todos-changed` |
| POST /todos/delete | {ws, todo_id} | {"ok":true} | Emits `todos-changed` |
| GET /todos/list | query workspace_id | raw todo array | none |

- Errors: 400 `{"error":"invalid json"}` or `{"error":"missing X"}`, 404 not found, 500 `{"error":"msg"}`.
- Output formats (lsp/mod.rs:453-667):
  - Locations: `rel:line:col  > source line`.
  - Hover: markdown text.
  - Symbols: `name (kind) — rel:l:c`.
  - Diagnostics: `ERROR l:c — msg (source)`.
  - Empty results: "No results found." / "No hover information available." / "No symbols found." / "No diagnostics."

## 5. LSP

**Config** (types.rs:10-98): `{command, args, extensions, detect_files, language_id, install_hint, project_roots}`. User overrides win over the built-ins.

| Id | Command | Extensions | Detect files | Install hint |
|---|---|---|---|---|
| rust | rust-analyzer | rs | Cargo.toml | rustup component add rust-analyzer |
| typescript | typescript-language-server --stdio | ts, tsx, js, jsx, mts, mjs, cts, cjs | tsconfig.json, package.json | bun i -g typescript-language-server typescript |
| go | gopls serve | go | go.mod | go install golang.org/x/tools/gopls@latest |
| python | pyright-langserver --stdio | py, pyi | pyproject.toml, requirements.txt, setup.py | pip install pyright |

**Detection** (detect.rs):
- Checks `project_roots` if set; otherwise the repo root, then one level of subdirectories.
- `validate_binary` searches the shell PATH.

**Pool** (server.rs):
- Keyed by `{repo_id, server_id}`, so one server is shared by all worktrees of a repo.
- Handle fields: `child`, `stdin` (behind its own mutex), `next_id`, `pending` (id → mpsc sender), `workspace_folders`, `initialized`, `open_documents`, `diagnostics_cache`.
- `get_or_start` returns `(handle, just_started)`.
- `start_server`:
  - cwd is the project root in the main repo.
  - If `.venv` or `venv` has a `pyvenv.cfg`, sets `VIRTUAL_ENV` and prepends its `bin` to PATH.
  - Starts a reader thread and a stderr-logging thread.
- `initialize` (60s timeout):
  - Capabilities: definition, references, hover (markdown/plaintext), publishDiagnostics, synchronization, workspaceFolders, symbol, didChangeWorkspaceFolders.
  - Then sends `initialized` and `didChangeConfiguration {python.analysis.diagnosticMode: "openFilesOnly"}`.
- `wait_for_ready`: polls `workspace/symbol ""` every 2s (10s per request) for up to 60s.
- Shutdown: `shutdown` + `exit`, wait 500ms, then kill.
- Worktrees: `add_worktree` sends `didChangeWorkspaceFolders`, mapping the subdirectory into the worktree. `remove_worktree` is called on workspace removal (workspace.rs:888) and stops the server when no folders remain. `shutdown_repo` runs on repo removal (repo.rs:35). `shutdown_all` is never called.

**Framing:**
- Messages are written as `Content-Length: N\r\n\r\n{json}`.
- The reader handles three cases: a message with both `id` and `method` (a server request) gets reply `{"result":null}`; a message with `id` resolves the matching pending sender; a `publishDiagnostics` notification is cached.
- `send_request` holds the handle lock only to allocate the id, writes under the stdin lock, and waits with `recv_timeout`.

**Queries** (mod.rs, 30s timeout):
- Each query opens the document (`didOpen`), sends the request, then closes it if it opened it.
- Diagnostics: open, sleep 500ms, read the cache.
- `apply_workspace_edit` handles both `changes` and `documentChanges` and applies edits bottom-up using byte offsets.

**Tauri commands** (commands/lsp.rs):
- `lsp_start_server(ws)` spawns in the background and emits `lsp-status {repo_id, server_id, status: starting|indexing|ready|error}`. It is called on workspace select (+page.svelte:2325) and from FileBrowser.
- `lsp_stop_server` emits `stopped`. Also `lsp_restart_server(repo, server, ws)` and `lsp_get_status`.
- Editor queries use `try_lock` + `get_existing` and never start a server:
  - `lsp_hover` → `{kind, text}?`
  - `lsp_goto_definition` → `{file_path, line, character}?`
  - `lsp_diagnostics` → `[{line, character, end_line, end_character, severity, message, source}]`
  - `lsp_rename` → `{files_changed, edits_applied, details}`
- By contrast, the MCP path starts servers synchronously and waits for readiness.

## 6. Knowledge base (`<data_dir>/context/<repo_id>/`)

**Files:**
- **hot.md** (no LLM; `regenerate_hot`, context.rs:87-197): "# Current state\n_Generated: ISO_" followed by:
  - "## Recent commits" (`git log --oneline -20`)
  - "## Remote branches" (`branch -r --sort=-committerdate`, 20)
  - "## Open PRs" (`gh pr list --json number,title,headRefName,state --limit 10`)

  It is regenerated on repo select (+page.svelte:899) and after a merge done through Korlap.
- **invariants.md**: `# Invariants\n\n- INV-001: ...`
- **facts.md**: free-form facts.
- **context.md**: entries of the form `## <slug>-<6hash>\n_Touches: globs_\n\n2-4 sentences`.
- **index.md**: Invariants, Facts, a "Context entries" table, and "## File affinity" lines `glob → id`.
- **contradictions.md**: `## CONTRA-001: title\nPattern A... Found in...\nPattern B...\nCannot determine which is canonical. Requires human resolution.` Resolved entries get the marker `<!-- RESOLVED: exception|updated -->` or `<!-- TECH_DEBT -->`.
- **tmp/*-raw.md**: scratch files, deleted after the build.

**Counts** (context.rs:279): `- INV-` lines, `- ` lines (facts), `## ` lines (context entries), `## CONTRA-` lines.

**Commands:**
- `get_context_meta`
- `save_context_scope(repo, include_globs, exclude_globs, precheck_model)`
- `read_context_file` (whitelist of six files)
- `write_context_file` (same whitelist minus hot.md; recounts afterwards)
- `stop_context_build`
- `check_invariants`
- `build_knowledge_base`, `update_context_after_merge`, `update_knowledge_base_incremental`
- `draft_contradiction_resolution`, `resolve_contradiction`

**Build** (context.rs:386-646; triggered manually from RepoSettings.svelte:366):
- Size constraint from `git ls-files` count (context.rs:246-254):
  - Under 500 files: "SMALL ... Full directory walk, full git history, sample up to 20 files".
  - Up to 5000: "MEDIUM ... Max 3 directory levels deep, last 6 months, 10 files".
  - Larger: "LARGE ... Max 2 levels, last 3 months, 5 files, skip test/vendor/generated".
- Scope text: "Include only files matching: ..." / "Exclude files matching: ...", or "No scope restrictions — analyze the entire repo."
- Prompt structure (context.rs:426-534):
  - "You are building a knowledge base for this repository. Write all output files to: {dir}".
  - **Phase 1**, five parallel subagents writing to `{dir}/tmp/`:
    1. stack: manifests and lockfiles.
    2. structure: directory tree.
    3. git: `git log --oneline -50`, `--merges -20`, `--diff-filter=A -- "*.toml" "*.json" "*.lock" -20`, looking for "replace/migrate/switch/instead/refactor".
    4. conventions.
    5. existing docs: README, CLAUDE.md, AGENTS.md, docs/, ARCHITECTURE*.
  - **Phase 2**, synthesis with "Only include information you can verify — omit rather than guess." Gives the format for invariants (observable, never invented), facts (concrete example per convention), context entries (evidence only), the index template (including `src/auth/* → auth-a3f8c2`), and contradictions.md (conflicting patterns go there, never into invariants).
  - **Phase 3**: delete tmp, summarize uncertainty.
  - Quality rules: everything observable and sourced; globs must match real paths; "If the repo is too new or small... say so rather than padding".
- The stream is parsed by `parse_context_stream_line` (context.rs:294-384; text and tool_use only).
- Success means exit 0 **and** invariants.md exists. Then status becomes `Built` with `last_built_at`, `built_at_commit` (HEAD) and counts; otherwise `Failed`. `Done` is sent after the meta update.

**check_invariants** (context.rs:692-882):
- Diff is `git diff origin/<default>` in the worktree, truncated to 30k characters with "...[truncated]" appended.
- Prompt: "Review this diff against the invariants below.\nList any violations as a JSON array. If no violations, respond with: {\"passed\": true, \"violations\": []}\n\nResponse format (JSON only, no markdown fences):\n{\"passed\": false, \"violations\": [{\"invariant_id\": \"INV-001\", \"file\": \"path/to/file.rs\", \"line\": 42, \"description\": \"brief description\"}]}\n\nInvariants:\n{inv}\n\nDiff:\n{diff}"
- Returns `{passed, violations[]}` and fails open.
- Runs before every review once the KB is built (+page.svelte:2181-2210). In autopilot, violations are sent back as a fix prompt; manually, they only show as a toast.

**update_context_after_merge** (context.rs:886-1084):
- Only called after a merge done through Korlap's own merge action (+page.svelte:2051-2061).
- Changed files: `git diff --name-only origin/<base>...<branch>`. Affected entries come from `find_entries_by_file_affinity` (prefix match, context.rs:24).
- Prompt:
  > The branch {b} was just merged into {base}.
  >
  > Changed files:
  > {files}
  >
  > {Affected context entries (from file affinity index):\n... | No matching context entries found in file affinity index. Check if this merge introduces something new worth adding to invariants or context.}
  >
  > Current invariants.md:
  > {inv}
  >
  > Task: Review whether the merge requires updates to the context.
  >
  > For each affected entry: is it still accurate? If not, update it.
  > Did this merge introduce a new invariant or context entry? If so, add it.
  > Did this merge make an existing entry obsolete? If so, remove it.
  >
  > If a merged change contradicts an existing invariant, do NOT update the invariant silently.
  > Instead, flag it and leave invariants.md unchanged.
  >
  > Write updated files to: {dir}
  > Only touch entries related to the changed files.
  > Do not rewrite entries unrelated to this merge.
  > Explain each change you made and why in a brief summary at the end.
- It does not set `Building` and ignores the exit status, but always updates the counts and `built_at_commit`.

**Incremental update** (context.rs:1088-1400; triggered manually):
- Preconditions: status is `Built`, `built_at_commit` is set and still exists (`git cat-file`). If HEAD equals that commit, it sends `Done` and stops. More than 200 changed files is an error.
- Diff is `<base>..HEAD`. Over 40k it is replaced by "[Diff too large (N bytes), showing summary]" plus `--stat`.
- Unresolved contradictions are included.
- Prompt sections: "You are updating a knowledge base after codebase changes." / "## What changed (diff from {base7} to {head7})" / Changed files / Diff / "## Current affected KB entries" / "## Current invariants" / "## Unresolved contradictions" / "## Instructions":
  1. For each affected context entry: update if inaccurate given the diff. Keep the same entry ID and format (`## <title-slug>-<hash>`).
  2. If the diff introduces a new pattern/dependency/decision worth tracking, add a NEW context entry with a new title-slug and 6-char hash.
  3. If the diff makes an entry completely obsolete (removed feature, deleted module), remove only that entry.
  4. Check invariants against the diff:
     - If STRENGTHENED by the changes: update its wording.
     - If VIOLATED by the changes: do NOT silently update. Add a new contradiction to contradictions.md.
     - If new invariant-worthy pattern introduced: add it with the next INV-NNN number.
  5. If the diff changes facts (new dependency, renamed module, changed convention), update the relevant sections in facts.md. Only touch facts directly affected.
  6. Update the file affinity section of index.md if new files/directories were added or paths changed.
  7. DO NOT rewrite entries unrelated to the changed files.
  8. DO NOT remove entries just because you cannot verify them from the diff — they may still be valid.
  9. Preserve all `<!-- RESOLVED -->` and `<!-- TECH_DEBT -->` tags in contradictions.md.

  Then: "Write updated files to: {dir}\nOnly overwrite files you actually changed. If a file needs no changes, do not write it.\nEnd with a brief summary of changes made."
- On failure it reverts to `Built`.

**Contradiction resolution:**
- `draft_contradiction_resolution(repo, id, resolution, note)` (context.rs:1492-1640):
  - `tech_debt` returns the current invariants without an LLM call.
  - `exception`: 'The human says BOTH patterns are intentional. Their note: "{n}"\nAdd a new invariant that captures the exception rule. Do NOT remove existing invariants.'
  - `update_invariant`: 'The human says one pattern is correct and the other is wrong/stale. Their note: "{n}"\nIf an existing invariant needs updating, modify it in place. If a new invariant is needed, add it with the next available INV-NNN number. Do NOT remove unrelated invariants.'
  - Prompt: "You are updating a knowledge base's invariants file.\n\n## Contradiction being resolved\n{c}\n## Resolution direction\n{i}\n\n## Current invariants.md\n{inv}\n\n## Instructions\nOutput the COMPLETE updated invariants.md file. Keep the exact format: ```# Invariants\n\n- INV-001: description ...```\nRules:\n- Preserve ALL existing invariants that are still valid\n- Only add/modify what the resolution requires\n- Use the next available INV-NNN number for new entries\n- Each invariant must be concrete and observable, not aspirational\n- Output ONLY the file content, no explanation, no markdown fences".
  - Markdown fences are stripped from the output.
- `resolve_contradiction(repo, id, resolution, invariant_text?)` (context.rs:1643-1705): rewrites the header to `## {id} <!-- RESOLVED: exception|RESOLVED: updated|TECH_DEBT -->`, writes the approved invariants, and recounts.

## 7. Jira and OAuth

**Jira** (jira.rs):
- `get_jira_issues()` is a sync command that runs `zsh -c "jira issue list --raw"`. This needs the ankitpokhrel jira-cli installed and configured; Korlap stores no Jira credentials.
- Parses `[{key, fields:{summary, description (ADF), status.name, issueType.name}}]` and flattens ADF by collecting the `text` of nested `content` nodes.
- Returns `[{key, summary, description?, status, issue_type}]`.
- In the UI (JiraImportPopover.svelte:89-100, KanbanBoard.svelte:276), each selected issue becomes a todo titled "[KEY] summary". Nothing is written back to Jira.

**MCP OAuth** (oauth.rs):
- Writes `<tmp>/korlap-oauth/mcp-auth.json` containing `{"mcpServers":{"oauth-target":{"type":"sse","url":U}}}`.
- Runs `claude --mcp-config F --allowedTools "" -p "List all available MCP tools from the oauth-target server. Just list the tool names, nothing else." --output-format text` with a 120s timeout. The Claude CLI performs discovery, DCR, PKCE and the browser flow, and caches the tokens itself. Korlap stores none.
- `test_mcp_server` (persistence.rs:110+):
  - sse: `curl --max-time 5`.
  - stdio: spawns the server and writes `initialize` (protocolVersion 2024-11-05), waiting up to 5s for the first line.

## 8. Interactive flows

- **Permissions:** there are no permission prompts anywhere. Work mode uses `bypassPermissions` with three disallowed tools (Korlap supplies its own LSP via MCP). Plan mode uses `--permission-mode plan` plus an allowlist. KB agents use `--dangerously-skip-permissions`. Codex runs with approvals and sandbox bypassed.
- **AskUserQuestion:**
  - The preview carries `JSON(input.questions)`. In `-p` mode the turn simply ends there.
  - It is rendered as a special block (chat-utils.ts:149-160 → ChatPanel.svelte:543 → AskUserQuestion.svelte). The component parses `[{question, header, options[{label, description}], multiSelect}]` and supports options, multi-select and custom answers. Its state is owned by the parent so it survives virtual-list recycling.
  - On submit it sends the answer, or "1. a\n2. b" for multiple questions, through `handleSendImmediate`, which bypasses the queue and resumes the session (+page.svelte:1948). There is a "Change" button to un-submit.
- **TodoWrite** renders as a TodoListBlock; only the latest is active.
- **Edit** renders as an EditDiffBlock.
- **ExitPlanMode** puts the plan text in the transcript and shows Execute/Revise buttons.
- **Autopilot** (+page.svelte:1270-1410), evaluated in this order:
  1. Auto-answer: execute pending plans, and answer questions with `suggestReplies[0]`.
  2. Finalize reviews: clean or failed → `triggerPrAction(skipMerge)`; issues → send a fix prompt.
  3. Spawn the next ready, unblocked todo while under `maxConcurrentAgents = max(2, min(cpu/2, memGB/4))` (default 3).
  4. Auto-review idle workspaces that have a diff, up to `MAX_AUTO_REVIEW_CYCLES = 5`, then go straight to a PR.
  5. Handle PR conflicts and failing checks.

  A dependency counts as resolved once the dependent's spawned workspace PR is merged. The `feat/autopilot-mode` branch is blacklisted.

## 9. Bugs and gotchas

**Timeouts:**
- Commit message: 30s (the process leaks on timeout).
- Suggested replies: 30s. Auto-rename: 15s. Prioritize, dependencies and autopilot: 60s each.
- Invariant check: 30s (fail-open). Contradiction draft: 60s. OAuth: 120s.
- LSP initialize: 60s. LSP queries: 30s. Ready polling: 60s. Diagnostics wait: 500ms.
- Main agent, KB agents and the MCP HTTP read: **no timeout**.

1. **Stop-then-send race** (agent.rs:487-499). The old reader thread calls `st.agents.remove(ws_id)`. If "Send now" (+page.svelte:1835-1846) has already started a new agent, this removes the **new** handle and calls `wait()` on it while holding the AppState mutex. The whole app freezes until that agent finishes, and the UI is told "waiting" while the agent is still running. Fix with a generation or pid check.
2. **System prompt probably lost after turn 1** (agent.rs:264; agent_backend.rs:211). `--system-prompt` replaces Claude Code's default prompt and is not re-sent with `--resume`, so isolation rules and invariants likely disappear from turn 2 on. Use `--append-system-prompt` on every turn.
3. **Korlap MCP server missing in release builds** (agent.rs:237-252). The path is compile-time, and `<data_dir>/mcp/server.ts` is never written (and would lack node_modules). The system prompt still advertises the LSP tools.
4. **Unverified CLI flags:** `--max-tokens` (agent.rs:855, 931) and `--effort` (agent_backend.rs:208). If rejected, suggestions and auto-rename silently fail and thinking-mode sends break.
5. **Usage double count:** an assistant event with usage but no content is emitted by both the parser and the second pass (agent_backend.rs:349 and agent.rs:403). Stream-json may also repeat usage across content blocks; only the `result` event corrects the total.
6. `AgentEvent::Error` is never emitted. A non-zero exit after partial output is silent, and a crash without a `result` event sends no `Done`; the UI depends on `agent-status`.
7. **Process cleanup:** stop kills only `claude`, leaving the bun MCP server and Bash children orphaned (there is no process group). Nothing is killed on app exit: `shutdown_all` is unused and there is no exit hook in lib.rs.
8. `generate_commit_message` runs `git add -A` as a side effect (agent.rs:727) and does not kill its process on timeout (agent.rs:824).
9. **MCP HTTP API weaknesses:**
   - No auth: any local process can rename branches, edit todos, or write files via /lsp/rename.
   - JSON is built with `format!` without escaping (mcp_api.rs:137, 149, 165, 309, 323, 372).
   - Query strings are not URL-decoded.
   - Header matching is case-sensitive.
   - No body size cap or read timeout.
   - `bind` uses `expect` (mcp_api.rs:20).
   - `rename-branch` runs git while holding the AppState lock (mcp_api.rs:135-156).
   - `agent-notify` is emitted but nobody listens.
10. **LSP isolation leak:** servers are rooted in the main repo (mcp_api.rs:347, server.rs:247), and `lsp_rename` writes to any URI the server returns, including main-repo files, with no worktree check (mod.rs:318-365).
11. **`apply_text_edits`** (mod.rs:372-423) treats UTF-16 offsets as bytes. `replace_range` can panic on a char boundary (mod.rs:404). CRLF files drift.
12. **LSP protocol issues:**
    - Timed-out requests stay in `pending` forever (server.rs:645).
    - Every server request gets a `null` result, which is wrong for `workspace/configuration` (server.rs:551).
    - The python config is sent to all servers (server.rs:397).
    - `initialized` is set immediately, so status almost always shows "ready".
    - Overlapping extensions resolve nondeterministically from HashMap order (types.rs:87).
    - Any `package.json` triggers TypeScript detection.
    - The fixed 500ms diagnostics wait gives stale results.
    - An agent's first LSP call can block up to 90s.
13. `check_invariants` slices `&diff[..30_000]`, which can panic on a UTF-8 boundary (context.rs:763); it fails open everywhere.
14. **KB agent issues:**
    - `--dangerously-skip-permissions`, cwd in the main repo, and no timeout contradict CLAUDE.md.
    - Post-merge update ignores the exit status and does not set `Building` (context.rs:1058-1077). It only runs for Korlap-performed merges, and it runs git on the main thread.
    - A stale invariants.md makes a failed build look successful (context.rs:624).
    - The fact count counts every `- ` bullet.
15. **Relevant-context injection rarely fires:** it needs the glob path to appear literally in the user prompt (agent.rs:682-708). The budget check drops whole sections instead of truncating.
16. **Codex issues:** registration mutates global Codex config on every send, with collision risk from the 8-character id prefix. Plan, thinking and model-on-resume are ignored. It uses `npx`.
17. **Jira issues:** `#[serde(rename="issueType")]` (jira.rs:16-17) does not match Jira's actual `issuetype` field, so parsing likely fails. The shape of the `--raw` output is unverified. `zsh` is hard-coded. There is no timeout and it runs on the main thread.
18. **OAuth issues:** the comment says "NOT -p" but the code uses `-p` (oauth.rs:57-63). The temp file name is fixed. The server type is hard-coded to sse.
19. **Sync work on the main thread:** `send_message` runs gh, git and codex child processes (agent.rs:164), and the KB commands run git commands.
20. **Auto-rename race:** it renames the branch concurrently while the agent is running (agent.rs:338). Validation is weak (only needs `/` and ≤60 chars).
21. **Review session sharing:** the review runs in the chat session and pollutes it. Step 7 posts PR comments even when no PR exists. Any output containing "no issues found" is classed as clean.
22. `sessions.json` is never cleared once `session_ids` becomes empty (state.rs:340).
23. suggest_replies, auto-rename and draft_contradiction_resolution have no cwd set, so they inherit the app's cwd and may load an unrelated CLAUDE.md.

**Rebuild recommendations:**
- Have `parse_stream_line` return `Vec<ParsedEvent>`.
- Pass `--append-system-prompt` on every turn.
- Use per-run generation ids and process groups; kill everything on exit.
- Ship the MCP server as a compiled sidecar and give the HTTP API a per-run bearer token.
- Clamp LSP edits to the worktree and make offsets UTF-16-aware.