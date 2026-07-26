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
- `ling app request`：向云端发起一次端云链路模拟请求，默认输出事件摘要并显示 SID；`--verbose` 输出协议级明细。
- `ling app trace <sid>`：按 SID 查询既有请求记录。
- `ling app create / device / ota / role / kb / lexicon / tone / mcp / config`：通过 `/v1` 管理平台应用。
- `ling kb`：账号级知识库创建、文档添加、列表与文本检索。
- `ling wiki search <关键词...>`：搜索 ListenAI 文档中心。

> 生产安全：CLI 不执行应用、角色、MCP、知识库、知识库文档和专业词汇删除，也不执行设备列表、设备强制白名单开关以及 OTA 正式发布/撤销；对应命令只给出网页操作入口。未发布 OTA 包和 OTA 测试白名单仍可在 CLI 中删除。

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
cd <ling-repo>
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
make test  # 只运行单元测试
make lint
make build
```

## 登录

交互输入 API Key。先打开 `https://platform.listenai.com/keys` 获取 API Key，再执行：

```bash
ling login
```

检测到粘贴事件后会立即显示脱敏预览，例如 `65785f8b...ab632ee2`，无需等回车；完整 key 不会回显。登录成功后，默认输出会展示当前 API Base URL、可用模型数量和下一步建议。

如需机器读取登录结果，可输出 JSON：

```bash
ling login --json
```

通过参数或环境变量传入 `/keys` 页面 API Key：

```bash
ling login --api-key '<api-key>'
LING_API_KEY='<api-key>' ling login
```

默认配置保存到 `~/.config/listenai/ling/config.json`，也可以用 `LING_CONFIG` 覆盖配置文件路径。

登录后建议先确认账号状态：

```bash
ling account
```

## 环境切换

默认 API 地址是生产环境。访问其他环境时，把 `--api-base-url` 放在子命令前：

```bash
ling --api-base-url https://xxx.listenai.com account
ling --api-base-url https://xxx.listenai.com app list
```

也可以长期设置环境变量：

```bash
export LING_API_BASE_URL=https://xxx.listenai.com
ling app list                              # 仅列出已关联 Product ID 的应用
```

所有平台 HTTP、WebSocket 和部署请求都使用这一个 API 基地址；子命令不提供独立的环境地址。某个接口在不同环境中的临时可用性差异不会写入 CLI 的环境判断逻辑。

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
ling app capabilities              # 查看服务端 CLI 管理 API 契约版本与能力

ling app inspect <product_id>
ling app inspect                     # 在含 listenai.toml 的项目目录内可省略 product_id
ling app inspect --project-id <project_id>
ling app inspect --app-id <app_id>
ling app inspect <product_id> --json
ling app delete --product-id <product_id>    # 仅提示前往应用配置网页
```

`app list` 会在客户端过滤没有 Product ID 的 API 类应用，并基于过滤后的结果重新分页。

默认位置参数和 `--product-id` 都会先转换成 Project ID；转换接口尚未部署时，CLI 会从完整应用列表回退解析。`--project-id` 直接定位 Project，`--app-id` 从应用列表解析对应的 Project 和 Product。三种显式 ID 参数互斥。

`inspect` 展示：概览（项目/应用/产品 ID、服务端返回时包含产品密钥、计费）、角色、配置（唤醒词、主模型、版本、知识库/专业词汇/提示语/MCP 数量）、能力开关。

`app delete` 保留命令入口但绝不调用删除 API，只提示前往
`https://platform.listenai.com/appConfig?id=<project_id>` 确认影响范围并操作。

**注意**：部分服务端不会在应用详情中返回产品密钥。需要 Secret 时，请由用户本人前往平台网页的应用详情查看；不要把 Secret 或包含它的 `inspect` 输出贴到公开日志、截图或 issue 中。

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
  --product-id <product_id> \
  --version v1.0.0 \
  --version-name 首次发布 \
  --description 支持基础语音对话
```

`deploy` 参数：`--version` 必填（`0.1.0` 或 `v0.1.0`）；`--version-name` 默认 `<version> 版本`；`--sdk-version` 默认读取 `.version`；`--bundle` 默认 `dist/agent.js`；`--dry-run` 只预览。API Key 解析顺序：`--api-key`、`LING_API_KEY`、`ling login` 配置、`LISTENAI_API_KEY`。

### 端云链路模拟请求

对云端发起一次真实的端云交互（设备授权 + `/v1/interaction`），并把所有链路返回帧原样打印。`request` 在会话参数中显式设置 `llm_ws_version=2.0`，确保云端内部 LLM WebSocket 使用 v2；入口路径中的 `/v1/interaction` 不代表内部 LLM 链路版本：

```bash
ling app --product-id <product_id> request --product-secret '<product_secret>' --text 你好
ling app --product-id <product_id> request --product-secret '<product_secret>' --file hello.pcm
ling app --product-id <product_id> request --product-secret '<product_secret>' --text 你好 --device-id <device_id>
ling app --product-id <product_id> request --product-secret '<product_secret>' --text 你好 --llm-app <app_id>
ling app --product-id <product_id> request --product-secret '<product_secret>' --text 你好 --verbose
ling app --product-id <product_id> request --product-secret '<product_secret>' --text 你好 --output-tts reply.mp3
```

`--product-secret` 是 `request` 专用参数，仅用于设备鉴权，其他 `app` 命令不接受；若应用详情已返回完整 Secret，可以省略。也可在自己的终端临时设置 `LING_PRODUCT_SECRET`，避免把 Secret 留在 shell 历史中。不要把真实 Secret 粘贴到聊天、日志或 issue。

默认输出带本地时间戳的完整双向事件摘要，`↑` 表示 CLI 上行的创建会话、文本/音频数据、上传结束或 MCP 结果，`↓` 表示云端下行的连接、会话 SID、识别结果、回复文本 URL、TTS URL、MCP 调用和结束状态。收到文本 URL 后会并行读取 text_streaming SSE。在 TTY 中，累计回复保持为一条活动行并原位更新；预览按终端宽度限制为单行，过长时显示省略号和最新尾部，文本流结束后再打印一次完整回复。其他帧到达时会先打印该帧，再重画当前预览。输出被管道或外部工具捕获时，不使用 ANSI 控制序列，也不打印中间增长过程，只在文本流结束后输出一次完整回复。时序结束后空一行输出 SID、TTS URL、文本 URL、耗时和双向帧统计。interaction 不返回 token 用量，`request` 不会额外调用 trace 补充统计。

追加 `--verbose` 时，每行按 `[时间] ↑|↓ 协议帧` 输出 WebSocket 协议级明细：JSON 帧正文保持原文，二进制帧显示字节数和可选文本；text_streaming 会按事件边界输出 SSE，每帧压缩为一行，原始 `event:`/`data:` 行之间使用 ` | ` 分隔。verbose 不做增长预览，并在 SSE 结束后打印一次完整回复。由于方向、时间和 SSE 标记是诊断所必需的，该输出不是严格 JSON，`request` 不提供 `--json`。

`--output-tts <FILE>` 下载本次交互返回的第一个 TTS URL。输出目录不存在时会自动创建；下载完成后末尾摘要显示文件路径和字节数。

按 SID 回查 Agent 执行日志。CLI 优先调用按 SID 直查接口；旧服务端没有该接口时，才兼容扫描请求记录（默认最近 7 天，`--hours` 调整时间窗）：

```bash
ling app trace <sid>                 # 概览 + 时间线（请求到达/技能命中/工具进出参/回复/响应完成）
ling app trace <sid> --full          # 追加完整请求上下文与工具结果明细（也可用 --verbose）
ling app trace <sid> --hours 2
ling app trace <sid> --json          # 输出完整原始记录
```

### 设备管理

```bash
ling app device quota                    # 总额度 / 已使用 / 强制白名单状态
ling app device list                     # 仅提示前往应用配置网页
ling app device add <device_id>...
ling app device add --file devices.txt   # 每行一个 device_id
ling app device query <device_id>        # 查询设备是否已授权
ling app device enforce                  # 查看强制白名单开关
ling app device enforce on               # 仅提示前往应用配置网页
ling app device enforce off              # 仅提示前往应用配置网页
```

网页操作统一指向当前应用：`https://platform.listenai.com/appConfig?id=<project_id>`。

### 应用与配置管理

查询命令支持 `--json`。`--product-id`、`--project-id`、`--app-id` 可放在 `ling app` 之后或 action 之后；三者互斥。在含 `listenai.toml` 的项目目录内可全部省略，此时读取顶层 `product_id`。

```bash
ling app create Demo --template-id 12 --description "语音助手"

ling app role list
ling app role add 助手 --set persona='"简洁回答"' --set vcn=x4_yezi
ling app role edit <role_id> --set speed=60 --set volume=50
ling app role set-default <role_id>

ling app interact-mode
ling app interact-mode full-duplex

ling app kb list
ling app kb link <index_id>
ling app kb unlink <index_id>

ling app lexicon list
ling app lexicon add ListenAI

ling app tone show
ling app tone edit --set network_suc="网络连接成功"
ling app tone edit --reset network_suc
ling app tone edit --reset-all
ling app tone edit --file tones.json

ling app mcp list
ling app mcp add Weather --server-id weather --transport http --url https://mcp.example.com
ling app mcp edit <mcp_id> --set description='"天气服务"'
ling app mcp enable <mcp_id>
ling app mcp disable <mcp_id>

ling app config show
ling app config edit --set interaction-mode=half-duplex
ling app config edit --set system-prompt='"你是语音助手"'
ling app config edit --set endpoint=https://model.example/v1 --set model=model-name
ling app config reset-model
ling app config test-model --endpoint https://model.example/v1 --model model-name
```

角色与 MCP 的 `--set` 支持点路径和 JSON 字面量；`on/off` 会解析为布尔值。也可用 `--file <json>` 提交对象。

### OTA

```bash
ling app ota list
ling app ota upload firmware.bin --version 2.4.0 --version-number 240 --ota-mode mandatory
ling app ota get <package_id>
ling app ota edit <package_id> --description "修订说明"
ling app ota delete <package_id> --yes        # 仅未发布包；服务端再次校验状态
ling app ota whitelist list
ling app ota whitelist add <device_id>
ling app ota whitelist delete <device_id>
```

`ota publish/revoke` 不会调用发布接口，只提示前往当前 Project 的应用配置网页。删除未正式发布的 OTA 包、维护 OTA 测试白名单是需求明确允许的受限操作。

## 知识库（ling kb）

```bash
ling kb list
ling kb create 产品手册
ling kb delete <index_id>                # 仅提示前往网页操作
ling kb doc <index_id> list
ling kb doc <index_id> add --name 说明书.txt --url https://example.com/说明书.txt
ling kb doc <index_id> delete <doc_id>... # 仅提示前往网页操作
ling kb query <index_id> 空调怎么开
ling kb query <index_id> 空调怎么开 --limit 5 --threshold 0.3
```

知识库删除只提示前往 `https://platform.listenai.com/datasets`；文档删除指向
`https://platform.listenai.com/datasets/detail?id=<index_id>`。CLI 不调用对应的删除 API。

## 真实设备 PID/SID 切换

切换真实设备 PID/SID 时，不需要拉取端侧项目，也不需要编译固件。Product ID 可由 `ling app list` 获取；请由用户本人前往平台网页的应用详情查看产品密钥。确认目标后，由用户在自己的终端通过 `adb shell` 写入设备：

```bash
adb shell device set_pid <product_id>
adb shell device set_sid <product_secret>
```

`<product_secret>` 属于敏感信息，不要贴到公开日志、截图或 issue 中。

## 文档中心搜索

按空格拆分多个关键词，分别调用 docs2 GraphQL 搜索；单关键词默认最多输出前 20 条标题和已解码 URL，多关键词按搜索词分组、每组最多输出前 5 条。`--json` 输出合并去重后的完整 JSON：

```bash
ling wiki search 标准API 获取密钥
ling wiki search 标准API --json
```
