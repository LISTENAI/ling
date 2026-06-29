# ling

ListenAI 本地 CLI 工具。使用 ListenAI API Key 登录后，可以在终端里查看账号、模型、应用，并发起对话。

- `ling login`：保存并校验 API Key。
- `ling account`：查看当前 API Key 对应的账号信息，`--json` 输出原始 JSON。
- `ling models`：查看当前 API Key 可用模型列表，`--json` 输出原始 JSON。
- `ling chat <prompt>`：发起对话，支持 `--stream` 和 `--json`。
- `ling app list`：查看平台应用列表，默认输出终端表格，`--json` 输出原始 JSON。
- `ling app inspect <product_id>`：查看单个应用摘要，默认输出精简配置视图，`--json` 输出原始 JSON。
- `ling wiki search <关键词...>`：搜索 ListenAI 文档中心，默认输出标题和 URL；多关键词按词分组展示，`--json` 输出完整 JSON。
- `ling create/build/dev/deploy`：创建、构建、本地运行和部署 ListenAI Agent 项目。

## 环境依赖

- 基础 CLI 功能只需要 `ling` 二进制。
- Agent 项目命令（`ling create/build/dev`）依赖 `Node.js 18+`；`ling create` 会从平台获取最新 Framework SDK 并默认执行 `npm install`。

## Agent Skill

本项目包含一个 [Agent Skill](https://github.com/vercel-labs/skills)，可以让 AI 编程助手（Cursor、Claude Code、Windsurf 等）自动了解 `ling` 的用法。

安装：

```bash
npx skills add LISTENAI/ling
```

## 快速安装

macOS：

```bash
brew install LISTENAI/tap/ling
```

macOS / Linux：

```bash
curl -fsSL https://raw.githubusercontent.com/LISTENAI/ling/main/install.sh | sh
```

Windows PowerShell：

```powershell
irm https://raw.githubusercontent.com/LISTENAI/ling/main/install.ps1 | iex
```

默认安装最新 GitHub Release。也可以指定版本：

```bash
LING_VERSION="v0.1.0" curl -fsSL https://raw.githubusercontent.com/LISTENAI/ling/main/install.sh | sh
```

```powershell
$env:LING_VERSION = "v0.1.0"
irm https://raw.githubusercontent.com/LISTENAI/ling/main/install.ps1 | iex
```

## 更新

Homebrew：

```bash
brew trust listenai/tap
brew update
brew upgrade ling
```

macOS / Linux 安装脚本：

```bash
curl -fsSL https://raw.githubusercontent.com/LISTENAI/ling/main/install.sh | sh
```

Windows PowerShell 安装脚本：

```powershell
irm https://raw.githubusercontent.com/LISTENAI/ling/main/install.ps1 | iex
```

本地开发版本：

```bash
cd /Users/zh/Projects/listenai/ling
make install
```

更新后可确认实际使用的二进制：

```bash
type -a ling
ling --version
```

## 本地开发

开发机上默认安装到 `~/.cargo/bin/ling`：

```bash
make install
ling --help
```

也可以使用 Cargo 命令直接安装；`ling create/build/dev/deploy` 已在 Rust 主程序内实现，不需要额外二进制：

```bash
cargo install --path crates/ling --locked --force --root "$HOME/.local"
```

如果想安装到 `~/.local/bin/ling`：

```bash
make install INSTALL_ROOT="$HOME/.local"
```

如果 `ling` 命令找不到，确认安装目录的 `bin` 在 PATH 中；如果 `ling` 仍指向旧路径，用 `type -a ling` 检查：

```bash
export PATH="$HOME/.local/bin:$PATH"
type -a ling
```

## Docker Compose 开发

容器内 Rust toolchain 固定为 `1.95.0`：

```bash
make docker-test
make docker-lint
make docker-build
```

也可以直接使用 Docker Compose：

```bash
docker compose run --rm test
docker compose run --rm lint
docker compose run --rm dev cargo build --release
```

本地常用开发命令：

```bash
make fmt
make test
make lint
make build
```

## 登录

交互输入 API Key：

```bash
ling login
```

检测到粘贴事件后会立即显示脱敏预览，例如 `65785f8b...ab632ee2`，无需等回车；完整 key 不会回显。

通过参数或环境变量传入 `/keys` 页面 API Key：

```bash
ling login --api-key '<api-key>'
LING_API_KEY='<api-key>' ling login
```

默认配置保存到 `~/.config/listenai/ling/config.json`，也可以用 `LING_CONFIG` 覆盖配置文件路径。

## 环境切换

默认 API 地址是生产环境：

```bash
ling account
ling models
ling chat "你好"
ling app list
```

访问其他环境时，把 `--api-base-url` 放在子命令前：

```bash
ling --api-base-url https://xxx.listenai.com account
ling --api-base-url https://xxx.listenai.com models
ling --api-base-url https://xxx.listenai.com chat "你好"
ling --api-base-url https://xxx.listenai.com app list
ling --api-base-url https://xxx.listenai.com app inspect <product_id>
```

也可以长期设置环境变量：

```bash
export LING_API_BASE_URL=https://xxx.listenai.com
ling app list
```

## Agent 开发命令

`ling create/build/dev/deploy` 都由 `ling` Rust 主程序直接实现：

```bash
ling create my-agent
cd my-agent
ling build
ling build --release
ling build --entry src/main.ts --out dist/agent.min.js --release
ling dev
```

`ling create` 会调用 `/external/framework/sdk/latest` 获取最新 Framework SDK，下载并解压其中的默认模板生成项目，然后自动执行 `npm install`；只想生成文件时可用 `ling create my-agent --no-install` 跳过依赖安装。

`ling create` 会把 Framework SDK 的版本写入项目级 `.version` 文件。后续执行 `ling build`、`ling dev`、`ling deploy` 时会通过同一个接口检查该版本是否低于最新 SDK 版本；如果需要更新，交互终端会提示输入 `y` 或 `n`，输入 `y` 后会下载最新 SDK 并更新项目内 `sdk/` 目录。

默认从当前目录读取 `agent.ts`，输出到 `dist/agent.js`。打包格式为 ES2017 IIFE，并把 `@listenai/agent-sdk` 解析到项目内 `sdk/src/index.ts`。`ling build` 会优先使用 `LING_ESBUILD_BIN`、项目内 `node_modules/.bin/esbuild`、PATH 中的 `esbuild`，找不到时通过 `npx`/`npm exec` 调用固定版本 esbuild。

`ling dev` 会启动 esbuild watch，并用 Node.js Mock Host Harness 加载 bundle，提供热重载和 Mock 设备 REPL；输入一行文本回车即可向 Agent 发送一次 `isLast=true` 的文本消息。

`ling deploy` 上传已构建的 JavaScript bundle 并创建 Agent 版本；也可以用 `--dry-run` 只做本地预览：

```bash
ling deploy --product-id prod_dev_local --version v1.0.0 --dry-run
ling deploy \
  --product-id 2b108aff-3da2-479b-b1b9-88e58f8fad2d \
  --version v1.0.0 \
  --version-name 首次发布 \
  --description 支持基础语音对话 \
  --sdk-version 0.1.0
```

`ling deploy` 会 PUT 上传 raw JavaScript bundle 到 `/v1/framework/agents/{productIdOrAppId}`。`--version` 必填；可以传 `0.1.0` 或 `v0.1.0`，上传时会规范为 `vX.Y.Z`。API Key 解析顺序：`--api-key`、`LING_API_KEY`、`ling login` 保存的配置、`LISTENAI_API_KEY`。

常用参数含义：

- `--product-id`：要部署到的 Product ID 或 App ID；服务端最终会解析为 App ID。
- `--version`：本次上传的 Agent 版本，必填；可以传 `0.1.0` 或 `v0.1.0`，同一 App 下不能重复，且要大于已有最高版本。
- `--version-name`：版本展示名称；不传时默认为 `<version> 版本`，例如 `--version 0.1.0` 会生成 `0.1.0 版本`。
- `--description`：版本说明，例如本次新增或修复的能力。
- `--sdk-version`：Agent SDK 版本；不传时读取当前目录 `.version`，读取不到则不传该参数。
- `--bundle`：已构建 JS bundle 路径，默认 `dist/agent.js`。
- `--endpoint`：平台 API 地址，默认 `https://api.listenai.com`，也可用全局 `--api-base-url` 指定。
- `--dry-run`：只检查本地参数和 bundle，不实际上传。

## 账号与模型

查看当前 API Key 对应账号：

```bash
ling account
ling account --json
```

查看当前 API Key 可用模型：

```bash
ling models
ling models --json
```

## 对话

默认使用 `doubao-seed-1.6-flash` 调用 `/v1/chat/completions`：

```bash
ling chat "广州有什么好玩的"
```

常用参数：

```bash
ling chat "广州有什么好玩的" --model spark-general-max-32k
ling chat "只输出一句话介绍你自己" --system "你是小聆助手"
ling chat "写一首短诗" --temperature 0.7 --max-tokens 200
ling chat "解释一下 RAG" --stream
ling chat "解释一下 RAG" --json
```

## 应用列表

默认输出终端表格，固定展示重要字段：

```bash
ling app list
```

表格列：

```text
Name │ Product ID │ App ID │ Type │ Deploy │ Cost │ Status │ Created
```

分页参数：

```bash
ling app list --page 2
ling app list --page 2 --page-size 20
ling app list --service-type device
```

底部会显示当前分页和下一页/上一页命令：

```text
Showing 20 of 64 apps (page 1/4; page size 20). Use --json for raw output.
Next: ling app list --page 2
```

输出服务端原始 JSON：

```bash
ling app list --json
```

## 应用详情

默认输出适合终端阅读的摘要，只展示关键信息：

```bash
ling app inspect <product_id>
```

摘要包含：

- 概览：项目 ID、应用 ID、产品 ID、产品密钥、计费、创建人、创建时间
- 角色：角色名、默认角色、类型、音色、角色知识库数量
- 配置：唤醒词、主模型、应用版本、更新策略、知识库数量、专业词汇数量、提示语数量、MCP 服务器数量
- 能力：长期记忆、声纹识别、联网搜索、文字生成图片、图片内容理解

`inspect` 默认会明文展示用户自己项目下的产品密钥，方便复制使用；注意不要把终端输出贴到公开日志或截图里。

输出服务端原始 JSON：

```bash
ling app inspect <product_id> --json
```

## 文档中心搜索

按空格拆分多个关键词，分别调用 docs2 GraphQL 搜索；单关键词默认最多输出前 20 条标题和已解码 URL，多关键词按搜索词分组、每组最多输出前 5 条。`--json` 输出合并去重后的完整 JSON：

```bash
ling wiki search 标准API 获取密钥
```

示例输出：

```text
找到 1 条文档：
1. 标准API
   https://docs2.listenai.com/zh/大模型开发/API接口/标准API

使用 --json 输出 JSON。
```

输出完整 JSON：

```bash
ling wiki search 标准API 获取密钥 --json
```
