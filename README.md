# ling

ListenAI 本地 CLI 工具。使用 ListenAI API Key 登录后，可以在终端里查看账号、模型、应用，并发起对话。

- `ling login`：保存并校验 API Key。
- `ling account`：查看当前 API Key 对应的账号信息，`--json` 输出原始 JSON。
- `ling models`：查看当前 API Key 可用模型列表，`--json` 输出原始 JSON。
- `ling chat <prompt>`：发起对话，支持 `--stream` 和 `--json`。
- `ling app list`：查看平台应用列表，默认输出终端表格，`--json` 输出原始 JSON。
- `ling app inspect <project_id>`：查看单个应用摘要，默认输出精简配置视图，`--json` 输出原始 JSON。
- `ling wiki search <关键词...>`：搜索 ListenAI 文档中心，默认输出标题和 URL；多关键词按词分组展示，`--json` 输出完整 JSON。

## 快速安装

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
curl -fsSL https://raw.githubusercontent.com/LISTENAI/ling/main/install.sh | LING_VERSION=v0.1.0 sh
```

```powershell
$env:LING_VERSION = "v0.1.0"; irm https://raw.githubusercontent.com/LISTENAI/ling/main/install.ps1 | iex
```

## 本地开发

开发机上推荐直接安装到 `~/.cargo/bin/ling`：

```bash
make install
ling --help
```

等价的 Cargo 命令：

```bash
cargo install --path crates/ling --locked --force
```

如果 `ling` 命令找不到，确认 `~/.cargo/bin` 在 PATH 中：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
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
ling --api-base-url https://xxx.listenai.com app inspect <project_id>
```

也可以长期设置环境变量：

```bash
export LING_API_BASE_URL=https://xxx.listenai.com
ling app list
```

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

默认使用 `qwen3-next-80b-a3b-instruct` 调用 `/v1/chat/completions`：

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
Name │ Project ID │ App ID │ Type │ Deploy │ Cost │ Status │ Created
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
ling app inspect <project_id>
```

摘要包含：

- 概览：项目 ID、应用 ID、产品 ID、产品密钥、计费、创建人、创建时间
- 角色：角色名、默认角色、类型、音色、角色知识库数量
- 配置：唤醒词、主模型、应用版本、更新策略、知识库数量、专业词汇数量、提示语数量、MCP 服务器数量
- 能力：长期记忆、声纹识别、联网搜索、文字生成图片、图片内容理解

`inspect` 默认会明文展示用户自己项目下的产品密钥，方便复制使用；注意不要把终端输出贴到公开日志或截图里。

输出服务端原始 JSON：

```bash
ling app inspect <project_id> --json
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
