---
name: ling
description: ListenAI（聆思）平台本地 CLI 工具，覆盖账号登录、模型对话、语音合成/识别（TTS/ASR）、平台应用与知识库管理、端云链路在线调试（request/按 sid trace 请求）、文档中心搜索、真实设备 PID/SID 切换，以及云端 Agent 项目 init/build/dev/deploy（listenai.toml 关联 product_id）和端侧固件/arcs_mini 项目初始化。当用户提到 ling、小聆、聆思、LSPlatform、product_id、listenai.toml、唤醒词、提示语/提示音、发音人、端云调试、sid 查询，或需要在终端中与 ListenAI 平台、Agent 项目或设备配置交互时使用。
---

# ling - ListenAI 本地 CLI 工具

ListenAI 平台的命令行工具。使用 ListenAI API Key 登录后，可以在终端里调用平台 AI 能力（对话/TTS/ASR）、管理应用与知识库、发起端云链路调试、完成 Agent 项目开发部署，并辅助真实设备切换 PID/SID。

## 何时使用

- 用户需要在终端中与 ListenAI 平台交互（登录、查看账号、浏览模型）
- 用户需要在终端中与 ListenAI AI 模型对话，或调用语音合成（TTS）、语音识别（ASR）
- 用户需要管理或查看 ListenAI 应用（列表、详情、角色、提示语、设备额度等）
- 用户需要管理 ListenAI 知识库（增删查、文档、检索）
- 用户需要向云端应用发起端云链路模拟请求（在线调试），或拿着 sid 回查一次请求
- 用户需要搜索 ListenAI 文档中心
- 用户需要初始化、构建、本地运行或部署 ListenAI Agent 项目
- 用户在安装 ling 后，需要完成 API Key 登录、需求确认、云端/端侧项目初始化的标准启动流程
- 用户需要切换真实设备 PID/SID、切应用或换设备绑定
- 用户需要创建云端 Agent 项目，或在明确固件源码开发时拉取端侧固件/arcs_mini 项目
- 用户需要在不同 ListenAI API 环境之间切换

## 工作流入口

当用户要登录、创建 Agent、构建、调试、部署、切换设备 PID/SID、判断云端/端侧链路，或描述 ListenAI 开发需求时，先阅读 `references/workflow.md`，再行动。

`references/workflow.md` 是标准工作流说明；不要在 `SKILL.md` 中重复扩写同一套流程。这里保留常用命令和关键注意事项，详细执行顺序以 `references/workflow.md` 为准。

## 公开内容原则

- 不写入本机绝对路径、个人目录、内部测试工程或不可公开信息
- 使用公开 URL、相对路径和 `<placeholder>` 表示用户环境中的值
- 不在回复、日志、截图或公开文档中暴露完整 API Key、产品密钥或 SID

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

API Key 从 `https://platform.listenai.com/keys` 获取。

## 安装后标准工作流

完成安装并确认 `ling` 可执行后，按顺序推进：

1. **登录（由用户本人完成）**：登录是交互式操作，Agent 不要代替用户执行，也绝不要让用户把 API Key 粘贴到对话里。引导用户：到 `https://platform.listenai.com/keys` 获取 API Key，然后**在用户自己的终端里**运行 `ling login`，在交互提示中粘贴密钥（会显示脱敏预览，完整 key 不回显）。用户完成后，Agent 运行 `ling account` 验证登录状态。
2. **确认需求**：用最少问题确认目标：云端 Agent、设备 PID/SID 切换还是端侧固件；已有项目还是新建/拉取；目标设备、应用或 Product ID；本轮要完成开发、调试、构建、部署还是设备配置。若用户描述已明确，复述判断并继续。
3. **开发前方案确认**：在创建项目、修改代码、部署、拉仓库或写设备配置前，先输出方案让用户确认。方案包含：需求理解、选择的链路、将执行的命令、会修改或访问的对象、验收方式、敏感信息处理。
4. **判断类型**：提到 Agent、云端技能、平台应用、模型对话、API 集成时，按云端 Agent 处理；提到切 PID/SID、切应用、换设备绑定时，按设备配置处理；提到固件、端侧源码、设备、开发板、烧录、`arcs_mini` 时，按端侧固件处理；不确定时先向用户确认。
5. **初始化项目**：
   - 云端 Agent：Agent 环境下没有交互式选择，先用 `ling app list` 查出目标应用的 product_id（不确定选哪个时问用户），再在目标父目录执行 `ling app init <项目名> --product-id <product_id>`；关联结果写入项目 `listenai.toml`。已有项目则进入项目目录后继续 `ling app build`、`ling app dev` 或 `ling app deploy`。
   - 端侧固件：拉取 arcs_mini 仓库。目录不存在时执行 `git clone https://cloud.listenai.com/CSKG836746/arcs-sdk/public/arcs_mini.git`；目录已存在时执行 `git -C arcs_mini pull --ff-only`。拉取后先阅读仓库 README 和构建脚本，再按用户需求操作。
6. **执行前检查**：涉及 `npm install`、`ling app init`、`git clone/pull`、`ling app deploy`、`adb shell device set_pid/set_sid` 等联网、写文件、部署或写设备步骤时，简要说明将执行的动作；涉及生产部署、密钥或产品密钥时，先确认环境和目标，避免泄露敏感信息。

## 登录

交互式输入 API Key（检测到粘贴事件后会立即显示脱敏预览，如 `65785f8b...ab632ee2`）：

```bash
ling login
```

登录默认输出友好状态和下一步建议。需要机器读取时输出原始 JSON：

```bash
ling login --json
```

通过参数或环境变量传入 API Key：

```bash
ling login --api-key '<api-key>'
LING_API_KEY='<api-key>' ling login
```

配置保存到 `~/.config/listenai/ling/config.json`，可用 `LING_CONFIG` 环境变量覆盖路径。

## 账号与模型

```bash
ling account            # 查看当前账号信息（验证登录状态）
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

**注意**：`trace`（尤其 `--full`）输出包含终端用户的对话内容与请求上下文，属敏感数据，展示或转述时注意脱敏。

## 真实设备 PID/SID 切换

切换真实设备 PID/SID、切应用或换设备绑定时，不要拉端侧项目，不要编译固件。先用 `ling` 获取产品密钥，再用 `adb shell` 写入设备。

```bash
ling app inspect <product_id>
adb shell device set_pid <product_id>
adb shell device set_sid <product_secret>
```

交互式写入也可以：

```bash
adb shell
device set_pid <product_id>
device set_sid <product_secret>
```

其中 `ling app inspect <product_id>` 输出中的“产品 ID”作为 PID，“密钥”作为 SID。执行后让用户重新唤醒或重连设备验证生效。不要把 `<product_secret>` / SID 明文写入公开回复、日志或截图；如需说明，只展示脱敏形式。

## Agent 项目

```bash
ling app init my-agent --product-id <product_id>   # 拉取最新 Base 项目并关联平台应用
ling app init my-agent --no-install                # 跳过 npm install
cd my-agent
ling app build                                     # 打包 agent.ts 到 dist/agent.js
ling app build --release                           # 生产压缩构建
ling app dev                                       # 本地热重载 + Mock 设备 REPL
ling app deploy --version v1.0.0 --dry-run         # product_id 取自 listenai.toml
ling app deploy \
  --product-id <product_id> \
  --version v1.0.0 \
  --version-name 首次发布 \
  --description 支持基础语音对话
```

要点：

- `init` 会获取最新 Framework SDK，解压默认模板，把 SDK 版本写入 `.version`，并把选中应用的 `product_id` 写入 `listenai.toml`；Agent 环境下请始终显式传 `--product-id`（交互式选择会被自动跳过）
- `build/dev/deploy` 会用 `.version` 与最新 SDK 版本对比，需要更新时交互确认
- `--version`：必填，`0.1.0` 或 `v0.1.0`，同一 App 下不能重复且需大于当前最高版本
- `--product-id`：不传时读取当前目录 `listenai.toml`
- `--dry-run`：只预览，不上传

## 知识库

```bash
ling kb list
ling kb create 产品手册
ling kb delete <index_id> --yes        # 删除知识库（不可恢复）
ling kb doc <index_id> list
ling kb doc <index_id> add --name 说明书.txt --url https://example.com/说明书.txt
ling kb doc <index_id> delete <doc_id>...
ling kb query <index_id> 空调怎么开 --limit 5
```

**删除规则（必须遵守）**：`kb delete` / `kb doc delete` 是不可恢复操作。Agent **禁止自作主张删除**——必须先向用户明确列出将要删除的对象并获得用户同意，之后才可以执行；Agent 执行时需带 `--yes`（非交互环境没有确认提示）。

## 文档中心搜索

多个关键词按空格拆分，分别独立搜索：

```bash
ling wiki search 标准API 获取密钥
ling wiki search 标准API                    # 单关键词（最多 20 条）
ling wiki search 标准API --json             # 原始 JSON
```

搜索结果只含标题和 URL。需要阅读文档全文时，用你自带的网页抓取工具（如 WebFetch）打开对应 URL 获取完整内容。

## 常见错误速查

| 错误信息 | 下一步 |
| --- | --- |
| `未找到 API Key` / HTTP 401 | 引导用户在自己的终端执行 `ling login`（见工作流第 1 步） |
| `未指定应用：请传 --product-id …` | 传 `--product-id`，或进入含 `listenai.toml` 的项目目录执行 |
| 设备授权失败 `20105`（白名单） | 该应用开启了设备白名单，改用已导入的设备 ID：`--device-id <id>` |
| `WAV 格式不符合要求` | 音频需 16kHz 16bit LE 单声道；用 ffmpeg 转换后重试 |
| `trace` 未找到 SID | 确认 sid 无误、未过期；加大 `--hours` 时间窗 |
| `version must match vX.Y.Z` / 版本重复 | deploy 版本号需为 `X.Y.Z` 且大于该应用当前最高版本 |
| `非交互环境，请追加 --yes` | 删除类命令在 Agent 环境需带 `--yes`（先取得用户同意） |

## 端侧固件

只有用户明确要求固件源码、SDK、开发板编译、烧录、`arcs_mini` 相关开发时，才进入端侧仓库流程。

```bash
git clone https://cloud.listenai.com/CSKG836746/arcs-sdk/public/arcs_mini.git
```

目录已存在时可以更新：

```bash
git -C arcs_mini pull --ff-only
```

拉取后先阅读仓库 README 和构建脚本，再按用户需求操作。单纯的 PID/SID 切换、应用切换、设备绑定切换，一律使用“真实设备 PID/SID 切换”流程。

## 注意事项

- `--json` 标志在几乎所有查询命令上都可用，输出服务端原始 JSON
- `app list` 底部会显示分页信息和推荐的上一页/下一页命令
- 在含 `listenai.toml` 的项目目录内，`ling app` 系列命令自动使用其中的 `product_id`
- 公开 skill 内容不要写入本机绝对路径、个人目录、内部测试工程或不可公开信息
