# Worktree workflow

このローカルリポジトリは、Gitデータ・main checkout・Issue別worktreeを1つのコンテナにまとめる。

```text
C:\Users\aodaruma\Documents\GitHub\adobe-mcp-rs\
  .repo.git\            # central bare repository
  main\                 # main worktree
  worktrees\            # Issue/feature worktrees
```

コンテナ直下はworktreeではない。通常の編集とGit操作は `main\` または `worktrees\<name>\` で行う。

確認:

```powershell
cd .\main
git worktree list
git status --short --branch
```

新しい作業用worktreeを作る例:

```powershell
cd .\main
git worktree add ..\worktrees\issue-123 -b codex/issue-123 main
git worktree add ..\worktrees\photoshop-hardening -b codex/photoshop-hardening main
```

既存branchをcheckoutする場合:

```powershell
git worktree add ..\worktrees\issue-123 codex/issue-123
```

削除:

```powershell
git worktree remove ..\worktrees\issue-123
git worktree prune
```

bare repoを直接確認する必要がある場合は、コンテナ直下から `--git-dir` を指定する。

```powershell
git --git-dir=.\.repo.git worktree list
```

注意:

- 同じbranchを複数worktreeで同時checkoutしない。
- 新規作業はIssueまたは独立した変更単位ごとにbranch/worktreeを分ける。
- worktreeを削除する前に、未commitの変更と未push commitを確認する。
- `.repo.git` はbare repositoryなので、直接ファイルを編集しない。
