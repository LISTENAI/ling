---
name: ling
description: ListenAI 平台本地 CLI 工具，支持账号登录、模型对话、语音合成/识别（TTS/ASR）、应用与知识库管理、端云链路调试、文档搜索，以及云端 Agent 项目 init/build/dev/deploy 和端侧固件/arcs_mini 项目初始化。当用户需要在终端中与 ListenAI 平台或 Agent/固件开发项目交互时使用。
---

# ling - ListenAI 本地 CLI 工具

ListenAI 平台的命令行工具。使用 ListenAI API Key 登录后，可以在终端里调用平台 AI 能力（对话/TTS/ASR）、管理应用与知识库、发起端云链路调试，并完成 Agent 项目开发部署。

## 何时使用

- 用户需要在终端中与 ListenAI 平台交互（登录、查看账号、浏览模型）
- 用户需要在终端中与 ListenAI AI 模型对话，或调用语音合成（TTS）、语音识别（ASR）
- 用户需要管理或查看 ListenAI 应用（列表、详情、角色、提示语、设备额度等）
- 用户需要管理 ListenAI 知识库（增删查、文档、检索）
- 用户需要向云端应用发起端云链路模拟请求（在线调试）
- 用户需要搜索 ListenAI 文档中心
- 用户需要初始化、构建、本地运行或部署 ListenAI Agent 项目
- 用户在安装 ling 后，需要完成 API Key 登录、需求确认、云端/端侧项目初始化的标准启动流程
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
   - 云端 Agent：在目标父目录执行 `ling app init <项目名>`，随后按提示选择要关联的平台应用（或用 `--product-id` 直接指定），关联结果写入项目 `listenai.toml`。已有项目则进入项目目录后继续 `ling app build`、`ling app dev` 或 `ling app deploy`。
   - 端侧固件：拉取 arcs_mini 仓库。目录不存在时执行 `git clone https://cloud.listenai.com/CSKG836746/arcs-sdk/public/arcs_mini.git`；目录已存在时执行 `git -C arcs_mini pull --ff-only`。拉取后先阅读仓库 README 和构建脚本，再按用户需求操作。
5. **执行前检查**：涉及 `npm install`、`ling app init`、`git clone/pull` 等联网或写文件步骤时，简要说明将执行的动作；涉及生产部署、密钥或产品密钥时，先确认环境和目标，避免泄露敏感信息。

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
ling account            # 查看当前账号信息
ling account --json     # 输出原始 JSON

ling ai models          # 查看可用模型列表
ling ai models --json   # 输出原始 JSON
```

## 对话

默认使用 `doubao-seed-1.6-flash` 模型：

```bash
ling ai chat "你好"
ling ai chat "你好" --model spark-general-max-32k
ling ai chat "你好" --system "你是小聆助手"
ling ai chat "写一首短诗" --temperature 0.7 --max-tokens 200
ling ai chat "解释一下 RAG" --stream    # 流式输出
ling ai chat "解释一下 RAG" --json      # 原始 JSON
```

## 语音合成（TTS）

传入一段文本，返回音频拉取 URL；`-o` 同时保存到文件：

```bash
ling ai tts 你好，今天天气怎么样                       # 输出音频 URL（限时有效）
ling ai tts --vcn x5_lingyuzhao_flow --speed 60 你好   # 指定发音人和语速
ling ai tts --format pcm --sample-rate 16000 -o hello.pcm 你好
ling ai tts --list-vcn                                 # 列出所有支持的发音人
```

参数：`--vcn` 发音人、`--format mp3|pcm`、`--sample-rate 8000|16000|24000`、`--speed/--volume/--pitch 1-100`、`--emotion/--emotion-scale/--style`（smartTTS）。

## 语音识别（ASR）

传入音频文件输出识别文本。暂只支持 PCM（16kHz 16bit LE 单声道）；WAV 会自动校验格式并去头：

```bash
ling ai asr hello.pcm
ling ai asr hello.wav --vad-eos 800
ling ai asr hello.pcm --json
```

## 应用

```bash
ling app list                                    # 终端表格，带分页
ling app list --page 2 --page-size 20            # 分页
ling app list --service-type device              # 按服务类型过滤
ling app list --json                             # 原始 JSON

ling app inspect <product_id>                    # 精简摘要视图
ling app inspect                                 # 项目目录内（listenai.toml）可省略 product_id
ling app inspect <product_id> --json             # 原始 JSON
```

**注意**：`inspect` 会明文展示产品密钥，不要将终端输出贴到公开日志或截图里。

### 应用配置查看

均支持 `--json`；`--product-id` 统一放在 `ling app` 之后（紧跟 action 之后也兼容），项目目录内可省略：

```bash
ling app --product-id <product_id> role list   # 角色列表（发音人/语速/音量/默认角色/唤醒词）
ling app interact-mode         # 唤醒交互模式（oneshot / half-duplex / full-duplex）
ling app kb list               # 应用关联的知识库
ling app lexicon list          # 专业词汇
ling app tone show             # 设备提示语表
ling app device quota          # 设备额度与白名单状态
ling app device query <device_id>   # 查询设备是否已授权
```

角色/提示语/专业词汇/MCP/OTA/创建应用等**写操作**的平台开放 API 尚未上线，对应命令会输出明确提示，引导用户到平台网页端操作。

### 端云链路模拟请求（在线调试）

发起一次真实端云交互并打印所有链路帧（JSON 行）：

```bash
ling app request --text 你好，介绍一下你自己
ling app request --file hello.pcm                     # 音频输入，走 ASR+NLU+TTS 全链路
ling app request --text 你好 --device-id <device_id>  # 应用开启白名单时需用已导入的设备 ID
```

请求结束后 stderr 会输出本次 `sid`，可用它回查请求记录：

```bash
ling app trace <sid>              # 概览 + 时间线（请求到达/技能命中/工具进出参/回复/响应完成），默认检索最近 24 小时
ling app trace <sid> --full       # 追加完整请求上下文（system+多轮历史）与工具结果明细
ling app trace <sid> --hours 2 --json
```

## Agent 项目

```bash
ling app init my-agent                        # 拉取最新 Base 项目 + 交互式关联平台应用
ling app init my-agent --product-id <product_id>
ling app init my-agent --no-install           # 跳过 npm install
cd my-agent
ling app build                                        # 打包 agent.ts 到 dist/agent.js
ling app build --release                              # 生产压缩构建
ling app dev                                          # 本地热重载 + Mock 设备 REPL
ling app deploy --version v1.0.0 --dry-run            # product_id 取自 listenai.toml
ling app deploy \
  --product-id <product_id> \
  --version v1.0.0 \
  --version-name 首次发布 \
  --description 支持基础语音对话
```

要点：

- `init` 会调用 `/external/framework/sdk/latest` 获取最新 Framework SDK，解压默认模板，把 SDK 版本写入 `.version`，并把选中应用的 `product_id` 写入 `listenai.toml`
- `build/dev/deploy` 会用 `.version` 与最新 SDK 版本对比，需要更新时交互确认
- `--version`：必填，`0.1.0` 或 `v0.1.0`，同一 App 下不能重复且需大于当前最高版本
- `--product-id`：不传时读取当前目录 `listenai.toml`
- `--dry-run`：只预览，不上传

## 知识库

```bash
ling kb list
ling kb create 产品手册
ling kb delete <index_id>              # 交互确认，--yes 跳过
ling kb doc <index_id> list
ling kb doc <index_id> add --name 说明书.txt --url https://example.com/说明书.txt
ling kb doc <index_id> delete <doc_id>...
ling kb query <index_id> 空调怎么开 --limit 5
```

## 文档中心搜索

多个关键词按空格拆分，分别独立搜索：

```bash
ling wiki search 标准API 获取密钥
ling wiki search 标准API                    # 单关键词（最多 20 条）
ling wiki search 标准API --json             # 原始 JSON
```

## 注意事项

- `--json` 标志在几乎所有查询命令上都可用，输出服务端原始 JSON
- `--api-base-url`（及 `LING_API_BASE_URL`）必须放在子命令**之前**；`--list-vcn` 等平台接口使用 `LING_PLATFORM_BASE_URL`
- `app list` 底部会显示分页信息和推荐的上一页/下一页命令
- 在含 `listenai.toml` 的项目目录内，`ling app` 系列命令自动使用其中的 `product_id`
