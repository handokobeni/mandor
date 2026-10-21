-- Workspace multi-repo: beberapa repo terdaftar di bawah satu folder induk.
-- Run workspace menyimpan folder induk dan repo tambahan di state_json run.
CREATE TABLE workspaces (
    id                TEXT PRIMARY KEY NOT NULL,
    name              TEXT NOT NULL,
    root_path         TEXT NOT NULL,
    -- Nama MCP server dari `.mcp.json` folder induk yang boleh dipakai agent.
    mcp_servers_json  TEXT NOT NULL DEFAULT '[]',
    created_at_ms     INTEGER NOT NULL
);

CREATE TABLE workspace_repos (
    workspace_id  TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    repo_id       TEXT NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    position      INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, repo_id)
);
