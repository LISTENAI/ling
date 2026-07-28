---
name: ling
description: ListenAI（聆思）平台本地 CLI 操作指南，覆盖安装登录、基础 AI 能力、平台应用与知识库管理、端云 request/trace 调试、真实设备应用绑定，以及自定义 Agent 项目的初始化、构建和测试链路部署。当用户提到 ling、小聆、聆思、LSPlatform、product_id、listenai.toml、发音人、提示语文案、端云调试、SID 查询，或需要在终端中操作 ListenAI 平台时使用。
---

# ListenAI ling

使用 `ling` 调用 ListenAI 平台能力、管理应用和知识库、调试端云链路，
以及开发自定义 Agent。

## 版本要求

本 Skill 适用于 `ling >= 0.2.0`。

- 每个任务开始时运行一次 `ling --version`。
- 版本低于要求时，说明需要升级并读取
  [标准工作流](references/workflow.md) 的安装章节。
- 命令不存在时先检查对应的 `--help`。不要猜测参数，也不要为了获得新版
  CLI 而克隆源码仓库自行编译。

## 读取参考

- 安装、登录、任务分流、Agent 部署、设备绑定或端侧任务：读取
  [标准工作流](references/workflow.md)。
- 需要精确命令、参数或输出约定：只读取
  [命令参考](references/commands.md) 中与当前任务有关的章节。

## 任务分流

- 模型、对话、TTS 或 ASR：使用 `ling ai`。
- 应用资料、角色、知识库、专业词汇、提示语文案、MCP、设备、OTA 或模型
  接入配置：使用对应的 `ling app` 子命令。
- 模拟端云请求或回查 SID：使用 `request` 和 `trace`。
- 只有明确涉及自定义 Agent 源码时，才进入 `init/build/deploy` 流程。
- 单纯切换真实设备 PID/SID 或应用绑定时，不要拉取、构建任何代码仓库。
- 涉及固件源码、SDK、开发板、编译或烧录时，发现并转交给匹配的端侧开发
  Skill。找不到专用 Skill 时停止，不要自行猜测仓库、工具链或烧录命令。
- `tone` 管理的是最终通过端云通道下发并合成为提示音的文案，不管理音频
  文件。
- `--set key=value` 的取值按 JSON 解析。文案本身像 JSON 时（纯数字、
  `true`/`false`、以 `[` 或 `{` 开头）必须写成 `--set key='"文本"'`，
  否则类型会出错；详见[命令参考](references/commands.md)。

## 目标应用

- 用户显式给出 Product ID、Project ID 或 App ID 时使用该标识；三者互斥。
- 未显式给出时，先检查当前目录的 `listenai.toml`，使用其中的
  `product_id`。
- 两者都没有时才运行 `ling app list`，不要替用户猜目标应用。
- `ling app list` 只展示已关联 Product ID、可由 CLI 管理的应用。

## 凭据与隐私

- 让用户本人在自己的终端运行 `ling login` 并输入 API Key；不要索取、
  代填、回显或记录完整密钥。
- `ling app request` 会自行完成应用鉴权，不要要求用户提供额外凭据。
- `inspect`、`request --verbose` 和 `trace --verbose/--json` 可能包含敏感
  应用信息、对话、请求上下文或工具结果；展示和转述前先脱敏。
- 真实设备 SID 属于敏感配置。需要用户输入时，让用户在自己的终端完成，
  不要要求其粘贴到对话中。

## 网页操作边界

以下命令入口只提供网页指引，不调用对应的高风险接口：

| 操作 | 网页 |
| --- | --- |
| 删除应用、角色、MCP 或专业词汇 | `https://platform.listenai.com/appConfig?id=<project_id>` |
| 查看设备列表 | `https://platform.listenai.com/appConfig?id=<project_id>` |
| 切换设备强制白名单（`device enforce` 只读） | `https://platform.listenai.com/appConfig?id=<project_id>` |
| OTA 正式发布或撤销 | `https://platform.listenai.com/appConfig?id=<project_id>` |
| 删除账号级知识库 | `https://platform.listenai.com/datasets` |
| 删除知识库文档 | `https://platform.listenai.com/datasets/detail?id=<index_id>` |

- 不要绕过 CLI 限制直接调用这些接口。
- 明确允许的删除例外只有未正式发布的 OTA 包和 OTA 测试白名单设备。

## 端云调试

```bash
ling app --product-id <product_id> request --text 你好
```

- 默认输出带时间和方向的双向事件摘要，MCP 的 `initialize` 和 `tools/list`
  折叠为工具数量和名称。
- 只有需要逐事件排查时才使用 `--verbose`；分享输出前先脱敏。
- `--output-tts <file>` 保存首个 TTS 音频。
- 默认使用 CLI 管理的 Device ID。只有用户明确指定设备身份时才传
  `--device-id`；只有用户明确要求定向诊断某个 App ID 时才传
  `--llm-app`。
- 如果鉴权返回 `20105`，读取本次 Device ID，询问用户是否授权将它导入
  当前应用。取得明确授权后才能执行 `device add`；强制白名单只能在网页
  切换，不要代用户去开关。
- 使用返回的 SID 执行 `ling app trace <sid>`，先查看默认时序概览。
  `trace` 按 SID 全局查询，不要给它传应用标识。
- 概览不足、需要查看未识别事件或逐步交互时使用 `--verbose`。
- 只有诊断解析歧义或保存机器可读证据时才使用 `--json`。

## 自定义 Agent

在用户确认进入自定义 Agent 开发流程后：

```bash
ling app --product-id <product_id> init <agent_name>
cd <agent_name>
ling app build
ling app deploy --version <version> --dry-run
ling app deploy --version <version> --activate
```

- `init` 将本地项目与目标应用关联。
- `--dry-run` 检查目标应用和构建产物，不上传版本。
- `--activate` 上传版本并将其用于应用测试链路。只有激活后，才能通过普通
  `request` 验证这个自定义版本。
- 使用 `ling app chain show` 确认测试链路模式和版本。
- 使用 `ling app chain versions` 查询已上传版本。
- 版本必须为 `X.Y.Z` 或 `vX.Y.Z`，同一 App 下不能重复且必须递增。
- 切换已有版本使用 `chain set custom`；恢复官方托管链路使用
  `chain set managed`。

## 常见错误

| 错误 | 处理 |
| --- | --- |
| CLI 不存在或版本过低 | 按标准工作流安装或升级官方 Release |
| 未找到 API Key / HTTP 401 | 让用户运行 `ling login`，再用 `ling account` 验证 |
| 未指定应用 | 使用显式标识、`listenai.toml` 或 `app list` |
| 设备授权失败 `20105` | 询问用户是否授权导入错误中显示的 Device ID |
| WAV 格式不符合要求 | 转为 16kHz、16bit LE、单声道 |
| `trace` 未找到 SID | 核对 SID，并适当扩大 `--hours` |
| 未发布 OTA 删除需要 `--yes` | 先确认用户确实要求删除，再追加参数 |

## 公开内容

- 只使用公开 URL、相对路径和 `<placeholder>`。
- 不写入本机绝对路径、个人目录、内部测试工程或不可公开信息。
- 不宣传内部部署环境或环境切换方式。
