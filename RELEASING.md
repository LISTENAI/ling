# ling 发版手册

本文面向负责发版的项目维护者。最终用户使用方式见 `README.md`；自动构建和
GitHub Release 发布逻辑见 `.github/workflows/release.yml`。

## 自动化边界

推送符合 `v*.*.*` 的 Tag 会触发 Release 工作流。工作流负责：

- 校验 Tag 是 `v` 开头的 SemVer，并与根目录 `Cargo.toml` 中的
  `workspace.package.version` 完全一致。
- 为 8 个目标平台构建、冒烟测试并打包二进制。
- 生成 `SHA256SUMS`，创建 GitHub Release 并上传产物。
- 将带预发布后缀的版本标记为 prerelease，例如 `v0.2.0-beta.1`。
- 保证 prerelease 不成为 GitHub Latest，也不更新 Homebrew Tap。
- 正式版本发布后通知 Homebrew Tap 更新；未配置 Token 时只跳过此步骤。

发版人负责准备版本提交、推送 `main`、创建并推送 Tag，以及检查自动化结果。
不要在推送 Tag 之前手动创建同名 GitHub Release。

## 发布 `v0.2.0-beta.1`

### 1. 确认发布基线

从最新且干净的 `main` 开始：

```bash
git switch main
git pull --ff-only
git status --short
```

`git status --short` 应没有输出。确认本次需要发布的代码均已进入本地 `main`，
并且没有仍在运行或失败的必需检查。

### 2. 更新版本

将根目录 `Cargo.toml` 中的版本改为：

```toml
[workspace.package]
version = "0.2.0-beta.1"
```

更新锁文件：

```bash
cargo check --workspace
```

只应产生预期的 `Cargo.toml` 和 `Cargo.lock` 版本变化：

```bash
git diff -- Cargo.toml Cargo.lock
```

### 3. 执行发布前检查

```bash
cargo test --workspace --lib --bins --locked
cargo build --release --locked -p ling
./target/release/ling --version
```

最后一条命令必须输出：

```text
ling 0.2.0-beta.1
```

提交版本前，可以从 GitHub Actions 的 Release 工作流选择当前分支，并启用
`dry_run`。它会执行完整的 8 平台构建、冒烟测试和打包，但不会创建或更新
GitHub Release：

```bash
gh workflow run release.yml \
  --ref main \
  -f tag=v0.2.0-beta.1 \
  -f dry_run=true
```

### 4. 提交版本并推送 `main`

```bash
git add Cargo.toml Cargo.lock
git commit \
  -m "chore(release): prepare v0.2.0-beta.1" \
  -m "Set the workspace and lockfile version for the beta release."
git push origin main
```

确认 `main` 的远端检查通过后再创建 Tag。

### 5. 创建并推送 Tag

使用 annotated Tag，并确保它指向刚才的版本提交：

```bash
git tag -a v0.2.0-beta.1 -m "v0.2.0-beta.1"
git show --no-patch v0.2.0-beta.1
git push origin v0.2.0-beta.1
```

不要移动或复用已经发布的 Tag。修复 beta 版本时发布新的递增版本，例如
`v0.2.0-beta.2`。

### 6. 检查自动发布

打开
[Release 工作流](https://github.com/LISTENAI/ling/actions/workflows/release.yml)。

也可以使用 GitHub CLI：

```bash
gh run list --workflow Release --limit 5
gh release view v0.2.0-beta.1 \
  --json tagName,isDraft,isPrerelease,assets,url
```

预期结果：

- 8 个平台构建全部成功。
- Release 不是 draft，`isPrerelease` 为 `true`。
- Release 包含 8 个平台压缩包和一个 `SHA256SUMS`。
- GitHub Latest 仍指向最近的正式版本。
- Homebrew Tap 没有被 beta 版本更新。

### 7. 安装验证

macOS / Linux：

```bash
curl -fsSL https://raw.githubusercontent.com/LISTENAI/ling/main/install.sh \
  | env LING_VERSION=v0.2.0-beta.1 sh
ling --version
```

Windows PowerShell：

```powershell
$env:LING_VERSION = "v0.2.0-beta.1"
irm https://raw.githubusercontent.com/LISTENAI/ling/main/install.ps1 | iex
ling --version
Remove-Item Env:LING_VERSION
```

默认安装不指定 `LING_VERSION`，仍会安装 GitHub Latest 中的正式版本。

## 失败处理

- 构建或上传偶发失败：在 GitHub Actions 中重试失败的 Job。
- 需要对同一 Tag 重新执行完整流程：手动运行 Release 工作流并传入已存在的
  Tag；工作流会重新构建并覆盖同名产物。

  ```bash
  gh workflow run release.yml -f tag=v0.2.0-beta.1
  ```

- 工作流发现现有 Release 的 prerelease 状态与 Tag 不一致时会停止。先人工
  核对 Release 状态，不要让自动化静默改写发布类型。
- 已发布代码有问题：修复代码并发布新的版本；不要强推或移动已发布 Tag。

## 发布正式版

正式发布 `v1.0.0` 时，重复上述流程，将 workspace 版本和 Tag 分别改为
`1.0.0`、`v1.0.0`。正式版会参与 GitHub Latest 选择，并在
`HOMEBREW_TAP_TOKEN` 可用时触发 Homebrew Tap 更新。
