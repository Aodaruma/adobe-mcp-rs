# Worktree workflow

この local checkout は linked worktree として使う前提です。

```text
C:\Users\aodaruma\Documents\GitHub\
  adobe-mcp-rs.git\   # bare repository
  adobe-mcp-rs\       # main worktree
```

確認:

```powershell
git worktree list
git status --short --branch
```

新しい作業用 worktree を作る例:

```powershell
git worktree add ..\adobe-mcp-rs-photoshop -b codex/photoshop-support main
git worktree add ..\adobe-mcp-rs-illustrator -b codex/illustrator-support main
git worktree add ..\adobe-mcp-rs-premiere-fix -b codex/premiere-hardening main
```

削除:

```powershell
git worktree remove ..\adobe-mcp-rs-photoshop
git worktree prune
```

注意:

- 同じ branch を複数 worktree で同時 checkout しない。
- 既存 branch で作業する場合は `git worktree add ..\path branch-name` を使う。
- 新規作業は host ごとに branch/worktree を分ける。
- `adobe-mcp-rs.git` は bare repository なので、通常の編集は `adobe-mcp-rs` などの worktree 側で行う。
