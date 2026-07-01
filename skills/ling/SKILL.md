---
name: ling
description: ListenAI 平台本地 CLI 工具，支持账号登录、模型浏览、AI 对话、应用管理、文档搜索、云端 Agent 项目 create/build/dev/deploy，以及端侧固件/arcs_mini 项目初始化。当用户需要在终端中与 ListenAI 平台或 Agent/固件开发项目交互时使用。
---

# ling - ListenAI 本地 CLI 工具

ListenAI 平台的命令行工具。使用 ListenAI API Key 登录后，可以在终端里查看账号、模型、应用，并发起对话。

## 何时使用

- 用户需要在终端中与 ListenAI 平台交互（登录、查看账号、浏览模型）
- 用户需要在终端中与 ListenAI AI 模型对话
- 用户需要管理或查看 ListenAI 应用
- 用户需要搜索 ListenAI 文档中心
- 用户需要创建、构建、本地运行或部署 ListenAI Agent 项目
- 用户在安装 ling 后，需要完成 API Key 登录、需求确认、云端/端侧项目初始化的标准启动流程
- 用户需要创建云端 Agent 项目，或拉取端侧固件/arcs_mini 项目
- 用户需要在不同 ListenAI API 环境之间切换

## 安装

macOS / Linux：

```bash
curl -fsSL https://raw.githubusercontent.com/LISTENAI/ling/main/install.sh | sh
```

Windows PowerShell：

```powershell
irm https://raw.githubusercontent.com/LISTENAI/ling/main/install.ps1 | iex
```

Homebrew（macOS）：

```bash
brew install LISTENAI/tap/ling
```

API Key 从 `platform.listenai.com/keys` 获取。

## 安装后标准工作流

完成安装并确认 `ling` 可执行后，按顺序推进：

1. **登录**：请用户到 `https://platform.listenai.com/keys` 获取 API Key。优先运行 `ling login`，让用户在交互提示中粘贴密钥；不要在回复、日志或截图里暴露完整密钥。登录后执行 `ling account` 验证账号状态。
2. **确认需求**：用最少问题确认目标：云端 Agent 还是端侧固件；已有项目还是新建/拉取；目标设备、应用或 Product ID；本轮要完成开发、调试、构建还是部署。若用户描述已明确，复述判断并继续。
3. **判断类型**：提到 Agent、云端技能、平台应用、模型对话、API 集成时，按云端 Agent 处理；提到固件、端侧、设备、开发板、唤醒、`arcs_mini` 时，按端侧固件处理；不确定时先向用户确认。
4. **初始化项目**：
   - 云端 Agent：默认使用 API 创建项目，在目标父目录执行 `ling --api-base-url https://api.listenai.com create ling_agent`。如果用户指定项目名，用该名称替换 `agent`；如果已有项目，进入项目目录后继续 `ling build`、`ling dev` 或 `ling deploy`。
   - 端侧固件：拉取 arcs_mini 仓库。目录不存在时执行 `git clone https://cloud.listenai.com/CSKG836746/arcs-sdk/public/arcs_mini.git`；目录已存在时执行 `git -C arcs_mini pull --ff-only`。拉取后先阅读仓库 README 和构建脚本，再按用户需求操作。
5. **执行前检查**：涉及 `npm install`、`ling create`、`git clone/pull` 等联网或写文件步骤时，简要说明将执行的动作；涉及生产部署、密钥或产品密钥时，先确认环境和目标，避免泄露敏感信息。

## 登录

交互式输入 API Key（检测到粘贴事件后会立即显示脱敏预览，如 `65785f8b...ab632ee2`）：

```bash
ling login
```

通过参数或环境变量传入 API Key：

```bash
ling login --api-key '<api-key>'
LING_API_KEY='<api-key>' ling login
```

配置保存到 `~/.config/listenai/ling/config.json`，可用 `LING_CONFIG` 环境变量覆盖路径。

## 账号与模型

```bash
ling account           # 查看当前账号信息
ling account --json    # 输出原始 JSON

ling models            # 查看可用模型列表
ling models --json     # 输出原始 JSON
```

## 对话

默认使用 `doubao-seed-1.6-flash` 模型：

```bash
ling chat "你好"
ling chat "你好" --model spark-general-max-32k
ling chat "你好" --system "你是小聆助手"
ling chat "写一首短诗" --temperature 0.7 --max-tokens 200
ling chat "解释一下 RAG" --stream    # 流式输出
ling chat "解释一下 RAG" --json      # 原始 JSON
```

## 应用

```bash
ling app list                                    # 终端表格，带分页
ling app list --page 2                           # 第 2 页
ling app list --page 2 --page-size 20            # 自定义页大小
ling app list --service-type device              # 按服务类型过滤
ling app list --json                             # 原始 JSON

ling app inspect <product_id>                     # 精简摘要视图
ling app inspect <product_id> --json              # 原始 JSON
```

`inspect` 展示内容：项目 ID、应用 ID、产品 ID/密钥、计费、角色、模型、能力（长期记忆、声纹识别、联网搜索、文字生成图片、图片内容理解）。

**注意**：`inspect` 会明文展示产品密钥，不要将终端输出贴到公开日志或截图里。

## 文档中心搜索

搜索 ListenAI docs2 文档中心。多个关键词按空格拆分，分别独立搜索：

```bash
ling wiki search 标准API 获取密钥
ling wiki search 标准API                    # 单关键词（最多 20 条）
ling wiki search "标准API" "获取密钥"         # 多关键词（每组最多 5 条）
ling wiki search 标准API --json               # 原始 JSON
```

## Agent 项目

```bash
ling create my-agent                           # 获取最新 Framework SDK 并创建 Agent 项目，随后自动 npm install
ling create my-agent --no-install              # 只生成文件，跳过 npm install
ling build                                     # 打包 agent.ts 到 dist/agent.js
ling build --release                           # 生产压缩构建
ling dev                                       # 本地热重载 + Mock 设备 REPL
ling deploy --product-id <product_id> --version v1.0.0 --dry-run
ling deploy \
  --product-id <product_id> \
  --version v1.0.0 \
  --version-name 首次发布 \
  --description 支持基础语音对话
```
`ling deploy` 参数含义：

- `ling create` 会调用 `/external/framework/sdk/latest` 获取最新 Framework SDK，解压默认模板，并把 SDK 版本写入 `.version`
- `ling build/dev/deploy` 会用 `.version` 和最新 Framework SDK 版本做对比；需要更新时，交互终端输入 `y` 会更新项目内 `sdk/`
- `--product-id`：目标 Product ID 或 App ID，必填
- `--version`：Agent 版本，必填；可以传 `0.1.0` 或 `v0.1.0`，同一 App 下不能重复，且必须大于当前最高版本
- `--version-name`：版本展示名称；不传时默认为 `<version> 版本`，例如 `--version 0.1.0` 会生成 `0.1.0 版本`
- `--description`：版本描述
- `--sdk-version`：Agent SDK 版本；不传时读取当前目录 `.version`，读取不到则不传该参数
- `--bundle`：JS bundle 路径，默认 `dist/agent.js`
- `--dry-run`：只预览，不上传
- `--endpoint`：平台 API 地址；默认跟随 `LING_API_BASE_URL` / 全局 `--api-base-url`

## 注意事项

- `--json` 标志在几乎所有命令上都可用，输出服务端原始 JSON
- `--api-base-url` 标志必须放在子命令**之前**
- `app list` 底部会显示分页信息和推荐的上一页/下一页命令
