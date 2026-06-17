---
name: ling
description: ListenAI 平台本地 CLI 工具，支持账号管理、模型浏览、AI 对话、应用管理和文档搜索。当用户需要在终端中与 ListenAI 平台交互时使用。
---

# ling - ListenAI 本地 CLI 工具

ListenAI 平台的命令行工具。使用 ListenAI API Key 登录后，可以在终端里查看账号、模型、应用，并发起对话。

## 何时使用

- 用户需要在终端中与 ListenAI 平台交互（登录、查看账号、浏览模型）
- 用户需要在终端中与 ListenAI AI 模型对话
- 用户需要管理或查看 ListenAI 应用
- 用户需要搜索 ListenAI 文档中心
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

## 注意事项

- `--json` 标志在几乎所有命令上都可用，输出服务端原始 JSON
- `--api-base-url` 标志必须放在子命令**之前**
- `app list` 底部会显示分页信息和推荐的上一页/下一页命令
