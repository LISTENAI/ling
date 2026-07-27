---
name: ling
description: ListenAI（聆思）平台本地 CLI 工具，覆盖安装与账号登录、模型对话、语音合成/识别（TTS/ASR）、平台应用与知识库管理、端云链路在线调试（request/按 sid trace 请求）、文档中心搜索、真实设备 PID/SID 切换，以及云端 Agent 项目 init/build/dev/deploy（listenai.toml 关联 product_id）和端侧固件/arcs_mini 项目初始化。当用户提到 ling、小聆、聆思、LSPlatform、product_id、listenai.toml、唤醒词、提示语/提示音、发音人、端云调试、sid 查询，或需要在终端中与 ListenAI 平台、Agent 项目或设备配置交互时使用。
---

# ListenAI ling

使用 `ling` 调用 ListenAI 平台能力、管理应用和知识库、调试端云链路，
以及开发 Agent 项目。

## 读取参考

- 安装、登录、项目初始化、设备绑定、部署或固件任务：先读
  [标准工作流](references/workflow.md)。
- 需要精确命令、参数或输出约定：只读
  [命令参考](references/commands.md) 中与当前任务相关的章节。

## 启动检查

- 执行任何 `ling` 任务前先运行 `ling --version`。
- 命令可用时继续使用当前版本；除非用户要求，否则不要重装或升级。
- 命令缺失或无法执行时，读取标准工作流的“CLI 检测与安装”，按当前操作
  系统安装官方 Release。不要为了使用 CLI 而克隆仓库或从源码编译。
- 安装需要联网并写入用户目录。说明动作并使用运行环境提供的授权机制；
  能由 Agent 执行时，不要只把命令丢给用户。
- 安装后再次运行 `ling --version`。如果当前 shell 尚未刷新 PATH，使用安装器
  返回的绝对路径验证，并告诉用户如何让后续终端找到 `ling`。

## 核心路由

- Agent、平台应用、模型或 API 集成：使用云端 Agent 流程。
- 切换 PID/SID、应用或设备绑定：使用设备配置流程，不拉固件仓库。
- 只有明确涉及固件源码、SDK、开发板、烧录或 `arcs_mini` 时，才进入
  端侧固件流程。
- 不确定目标应用时先运行 `ling app list`，不要替用户猜 Product ID。
- 非交互执行 `ling app init` 时显式传入 `--product-id`。

## 凭据与隐私

- 让用户本人在自己的终端运行 `ling login` 并粘贴 API Key；不要索取、
  代填、回显或记录完整密钥。
- Product Secret 只用于设备绑定和 `ling app request`。让用户本人从平台
  应用详情获取，并在自己的终端输入。
- 可建议用户临时设置 `LING_PRODUCT_SECRET`，避免 Secret 进入 shell 历史。
- `inspect`、`request --verbose` 和 `trace --verbose/--json` 可能包含
  密钥、对话、请求上下文或工具结果；展示和转述前先脱敏。

## 应用标识

- 默认使用 Product ID。
- 也可显式传 `--project-id` 或 `--app-id`；三种标识互斥。
- 项目目录中的 `listenai.toml` 可提供默认 `product_id`。
- `ling app list` 只展示已关联 Product ID、可由 CLI 管理的应用。

## 安全边界

- CLI 不执行应用、角色、MCP、知识库、知识库文档或专业词汇删除。
- CLI 不执行 OTA 正式发布/撤销、设备列表或设备强制白名单切换。
- 上述应用级操作引导到
  `https://platform.listenai.com/appConfig?id=<project_id>`。
- 账号级知识库删除引导到 `https://platform.listenai.com/datasets`；文档删除
  引导到
  `https://platform.listenai.com/datasets/detail?id=<index_id>`。
- 明确允许的删除例外只有未正式发布的 OTA 包和 OTA 测试白名单设备。
- 不要绕过 CLI 限制直接调用受限制的删除或发布接口。

## 端云调试

让用户本人提供 Product Secret 到自己的终端，然后执行：

```bash
ling app --product-id <product_id> request \
  --product-secret '<product_secret>' --text 你好
```

- 默认输出带时间、方向的双向事件摘要。
- 只有需要逐事件排查时才使用 `--verbose`；分享输出前先脱敏。
- `--output-tts <file>` 保存首个 TTS 音频。
- 默认使用 CLI 管理的 Device ID，实际值会显示在请求汇总或鉴权错误中。
  只有用户明确指定设备身份时才传 `--device-id`；只有用户明确要求定向诊断
  某个 App ID 时才传 `--llm-app`。
- 如果设备鉴权返回 `20105`，从错误中读取本次 Device ID，先询问用户是否
  授权将该 ID 导入当前应用。只有取得明确授权后，才执行
  `ling app --product-id <product_id> device add <device_id>`；不要擅自开启或
  关闭强制白名单。
- 使用返回的 SID 执行 `ling app trace <sid>`，先阅读默认的人类可读时序概览。
- 概览不足以定位问题、需要检查未被概览识别的新事件或每一步交互时，改用
  `ling app trace <sid> --verbose`。
- 只有需要诊断解析歧义或保留机器可读证据时，才使用
  `ling app trace <sid> --json`。不要把未经脱敏的详细日志直接展示给用户。

## 真实设备绑定

不要拉取或编译固件。让用户本人在终端写入：

```bash
adb shell device set_pid <product_id>
adb shell device set_sid <product_secret>
```

完成后重新唤醒或重连设备验证。不要在回复、日志或截图中展示 SID 明文。

## Agent 项目

```bash
ling app init <agent_name> --product-id <product_id>
cd <agent_name>
ling app build
ling app dev
ling app deploy --version <version> --dry-run
```

- `init` 将本地项目与目标应用关联。
- 部署版本必须为 `X.Y.Z` 或 `vX.Y.Z`，同一 App 下不能重复且必须递增。
- 正式上传前先用 `--dry-run` 检查目标应用和构建产物。

## 端侧固件

只有用户明确要求端侧开发时才拉取：

```bash
git clone https://cloud.listenai.com/CSKG836746/arcs-sdk/public/arcs_mini.git
```

目录已存在时使用 `git -C arcs_mini pull --ff-only`。随后阅读仓库 README 和
构建脚本，再执行编译或烧录。

## 常见错误

| 错误 | 处理 |
| --- | --- |
| 未找到 API Key / HTTP 401 | 让用户运行 `ling login`，再用 `ling account` 验证 |
| 未指定应用 | 传入一种应用标识，或进入含 `listenai.toml` 的目录 |
| 应用详情未返回产品密钥 | 让用户本人从应用详情获取并传给 `request` |
| 设备授权失败 `20105` | 读取错误中的 Device ID，询问用户是否授权导入，明确同意后再执行 |
| WAV 格式不符合要求 | 转为 16kHz、16bit LE、单声道 |
| `trace` 未找到 SID | 核对 SID，并适当扩大 `--hours` |
| 非交互环境需要 `--yes` | 只对未发布 OTA 删除追加，且先取得用户同意 |

## 公开内容

- 只使用公开 URL、相对路径和 `<placeholder>`。
- 不写入本机绝对路径、个人目录、内部测试工程或不可公开信息。
- 不宣传内部部署环境或环境切换方式。
