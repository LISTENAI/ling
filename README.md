# ling

ListenAI 本地 CLI 工具。使用 ListenAI API Key 登录后，可以在终端里调用平台 AI 能力、管理应用与知识库，并完成 Agent 项目的开发部署。

- `ling login`：保存并校验 API Key。
- `ling account`：查看当前 API Key 对应的账号信息，`--json` 输出原始 JSON。
- `ling ai models`：查看可用模型列表。
- `ling ai chat <prompt>`：发起对话，支持 `--stream` 和 `--json`。
- `ling ai tts <text>`：语音合成，输出音频 URL；`-o` 保存文件，`--list-vcn` 列出发音人。
- `ling ai asr <file>`：语音识别（16k 16bit LE 单声道 PCM / WAV）。
- `ling app list / inspect`：查看平台应用列表与摘要。
- `ling app init <name>`：初始化本地 Agent 项目并关联平台应用（写入 `listenai.toml`）。
- `ling app build / dev / deploy`：构建、本地运行和部署 Agent 项目。
- `ling app request`：向云端发起一次端云链路模拟请求，打印所有返回帧并输出 SID。
- `ling app trace <sid>`：按 SID 查询既有请求记录。
- `ling app device quota/query/enforce`：设备额度、授权查询与白名单状态。
- `ling app role/interact-mode/kb/lexicon/tone`：查看应用的角色、交互模式、知识库、专业词汇与提示语配置。
- `ling kb`：账号级知识库增删查 + 文档管理 + 文本检索。
- `ling wiki search <关键词...>`：搜索 ListenAI 文档中心。

> 部分管理写操作（创建应用、设备导入/白名单开关、OTA 管理、角色/提示语/专业词汇/MCP/唤醒词的增删改）依赖的平台开放 API 尚未上线，对应命令会给出明确提示；后端打通 API Key 授权链路后即可启用。

## 环境依赖

- 基础 CLI 功能只需要 `ling` 二进制。
- Agent 项目命令（`ling app init/build/dev`）依赖 `Node.js 18+`；`ling app init` 会从平台获取最新 Framework SDK 并默认执行 `npm install`。

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

也可以使用 Cargo 命令直接安装；所有子命令都在 Rust 主程序内实现，不需要额外二进制：

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

默认 API 地址是生产环境。访问其他环境时，把 `--api-base-url` 放在子命令前：

```bash
ling --api-base-url https://xxx.listenai.com account
ling --api-base-url https://xxx.listenai.com app list
```

也可以长期设置环境变量：

```bash
export LING_API_BASE_URL=https://xxx.listenai.com
export LING_PLATFORM_BASE_URL=https://xxx-platform.listenai.com   # 影响 --list-vcn 等平台接口
ling app list
```

## 基础 AI 能力（ling ai）

### 模型与对话

```bash
ling ai models
ling ai models --json

ling ai chat "广州有什么好玩的"
ling ai chat "广州有什么好玩的" --model spark-general-max-32k
ling ai chat "只输出一句话介绍你自己" --system "你是小聆助手"
ling ai chat "写一首短诗" --temperature 0.7 --max-tokens 200
ling ai chat "解释一下 RAG" --stream
ling ai chat "解释一下 RAG" --json
```

### 语音合成（TTS）

传入一段文本，返回一个音频拉取 URL；`-o` 同时把音频保存到文件：

```bash
ling ai tts 你好，今天天气怎么样
ling ai tts --vcn x5_lingyuzhao_flow --speed 60 你好
ling ai tts --format pcm --sample-rate 16000 -o hello.pcm 你好
ling ai tts --emotion cheerful --emotion-scale 10 今天真开心
ling ai tts --list-vcn                # 列出所有支持的发音人
```

常用参数：`--vcn` 发音人、`--format mp3|pcm`、`--sample-rate 8000|16000|24000`、`--speed/--volume/--pitch 1-100`、`--emotion/--emotion-scale/--style`（smartTTS）。

### 语音识别（ASR）

传入一个音频文件，输出识别文本。暂只支持 PCM（16kHz 16bit LE 单声道）；传入 WAV 时会校验格式并自动去掉文件头：

```bash
ling ai asr hello.pcm
ling ai asr hello.wav --vad-eos 800
ling ai asr hello.pcm --json
```

### 唤醒词

`ling ai wakeword` 依赖的平台开放 API 尚未上线，命令暂不可用。

## 应用管理（ling app）

### 列表与详情

```bash
ling app list
ling app list --page 2 --page-size 20
ling app list --service-type device
ling app list --json

ling app inspect <product_id>
ling app inspect                     # 在含 listenai.toml 的项目目录内可省略 product_id
ling app inspect <product_id> --json
```

`inspect` 展示：概览（项目/应用/产品 ID、密钥、计费）、角色、配置（唤醒词、主模型、版本、知识库/专业词汇/提示语/MCP 数量）、能力开关。

**注意**：`inspect` 会明文展示产品密钥，不要将终端输出贴到公开日志或截图里。

### 项目初始化与关联

```bash
ling app init my-agent                          # 拉取最新 Base 项目并交互式选择关联应用
ling app init my-agent --product-id <product_id>
ling app init my-agent --no-install             # 跳过 npm install
```

`init` 会把选中的 `product_id` 写入项目根目录的 `listenai.toml`。此后在项目目录内执行的 `ling app` 命令（inspect/deploy/request/device 等）都会默认使用该应用，无需再传 `--product-id`。

### 构建 / 本地运行 / 部署

```bash
cd my-agent
ling app build
ling app build --release
ling app dev
ling app deploy --version v1.0.0 --dry-run              # product_id 取自 listenai.toml
ling app deploy \
  --product-id 2b108aff-3da2-479b-b1b9-88e58f8fad2d \
  --version v1.0.0 \
  --version-name 首次发布 \
  --description 支持基础语音对话
```

`deploy` 参数：`--version` 必填（`0.1.0` 或 `v0.1.0`）；`--version-name` 默认 `<version> 版本`；`--sdk-version` 默认读取 `.version`；`--bundle` 默认 `dist/agent.js`；`--dry-run` 只预览。API Key 解析顺序：`--api-key`、`LING_API_KEY`、`ling login` 配置、`LISTENAI_API_KEY`。

### 端云链路模拟请求

对云端发起一次真实的端云交互（设备授权 + `/v1/dispatch`），并把所有链路返回帧原样打印：

```bash
ling app request --text 你好，介绍一下你自己
ling app request --file hello.pcm                        # 走 ASR + NLU + TTS 全链路
ling app request --text 你好 --device-id <device_id>     # 应用开启白名单时需用已导入的设备 ID
ling app request --text 你好 --llm-app <app_id>          # 多应用场景指定应用
```

输出为 JSON 帧流（`connected`/`started`/`result`(iat/nlp/tts)/`finish`），便于管道处理；结束后在 stderr 输出本次请求的 `sid`。

按 SID 回查请求记录（默认检索最近 24 小时，`--hours` 调整时间窗）：

```bash
ling app trace <sid>                 # 概览 + 时间线（请求到达/技能命中/工具进出参/回复/响应完成）
ling app trace <sid> --full          # 追加完整请求上下文与工具结果明细
ling app trace <sid> --hours 2
ling app trace <sid> --json          # 输出完整原始记录
```

### 设备管理

```bash
ling app device quota                    # 总额度 / 已使用 / 强制白名单状态
ling app device query <device_id>        # 查询设备是否已授权
ling app device enforce                  # 查看强制白名单开关
```

`device list/add`、`device enforce on|off` 依赖的平台开放 API 尚未上线。

### 应用配置查看

以下命令从应用详情中读取配置，均支持 `--json`；`--product-id` 统一放在 `ling app` 之后（紧跟 action 之后也兼容），项目目录内可省略：

```bash
ling app --product-id <product_id> role list   # 角色列表（发音人/语速/音量/默认角色/唤醒词）
ling app interact-mode         # 当前唤醒交互模式（oneshot / half-duplex / full-duplex）
ling app kb list               # 应用关联的知识库
ling app lexicon list          # 专业词汇
ling app tone show             # 设备提示语表
```

对应的写操作（`role add/edit/...`、`tone edit`、`interact-mode <mode>`、`ling app mcp`、`ling app ota`、`ling app create`）依赖的平台开放 API 尚未上线，执行时会给出明确提示。

## 知识库（ling kb）

```bash
ling kb list
ling kb create 产品手册
ling kb delete <index_id>                # 交互确认；--yes 跳过
ling kb doc <index_id> list
ling kb doc <index_id> add --name 说明书.txt --url https://example.com/说明书.txt
ling kb doc <index_id> delete <doc_id>...
ling kb query <index_id> 空调怎么开
ling kb query <index_id> 空调怎么开 --limit 5 --threshold 0.3
```

## 文档中心搜索

按空格拆分多个关键词，分别调用 docs2 GraphQL 搜索；单关键词默认最多输出前 20 条标题和已解码 URL，多关键词按搜索词分组、每组最多输出前 5 条。`--json` 输出合并去重后的完整 JSON：

```bash
ling wiki search 标准API 获取密钥
ling wiki search 标准API --json
```
