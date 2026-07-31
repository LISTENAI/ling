---
name: ling
description: ListenAI（聆思）平台本地 CLI 操作指南，覆盖安装登录、基础 AI 能力、平台应用与知识库管理、端云 request/trace 调试、真实设备应用绑定，以及自定义 Agent 项目的初始化、构建和测试链路部署。当用户提到 ling、小聆、聆思、LSPlatform、product_id、listenai.toml、发音人、唤醒词、唤醒应答语、提示语文案、端云调试、SID 查询，或需要在终端中操作 ListenAI 平台时使用。
---

# ListenAI ling

使用 `ling` 调用 ListenAI 平台能力、管理应用和知识库、调试端云链路，
以及开发自定义 Agent。

## 版本要求

本 Skill 适用于 `ling >= 1.0.0`。

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
- 应用资料、角色、唤醒词与应答语、知识库、专业词汇、提示语文案、MCP、
  设备、OTA 或模型接入配置：使用对应的 `ling app` 子命令。
- 模拟端云请求或回查 SID：使用 `request` 和 `trace`。
- 只有明确涉及自定义 Agent 源码时，才进入 `init/build/deploy` 流程。开发
  或修改自定义 Agent 时默认按完整开发流程执行；用户明确限定为只改代码、
  本地构建、预览或上传时，才在对应阶段停止。
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
- `ling app list` 只展示已关联 Product ID、可由 CLI 管理的设备应用。

## 凭据与隐私

- 让用户本人在自己的终端运行 `ling login` 并输入 API Key；不要索取、
  代填、回显或记录完整密钥。
- `ling app request` 默认会自行读取当前账号可管理应用的鉴权信息。模拟当前
  账号无权管理的应用时，必须由用户本人在终端同时传入完整的
  `--product-id` 和 `--product-secret`；不要索取或代填 Secret。
- Product Secret 敏感不等于必须把操作交给用户。绑定当前账号可管理的真实
  设备时，捕获 `ling app inspect --json` 的本地输出并取得完整 Product
  Secret，只在本地进程间传给设备写入命令；不要直接显示、转述或保存。
  服务端没有返回完整值时，才让用户通过本地隐藏输入补充，输入完成后仍由
  Agent 继续写入。
- `inspect`、`request --verbose`、`trace --verbose/--json` 和
  `ai asr --verbose` 可能包含敏感应用信息、会话标识、对话、请求上下文或
  工具结果；展示和转述前先脱敏。
- 设备命令 `set_sid` 写入的是 Product Secret；`request/trace` 返回的会话
  SID 是另一种标识，不得混用。需要用户补充 Product Secret 时，不要要求
  其粘贴到对话中。

## 网页操作边界

以下操作需通过网页完成；对应 CLI 入口只提供状态或网页指引：

| 操作 | 网页 |
| --- | --- |
| 删除应用 | `https://platform.listenai.com/application`，选择应用后进入“设置” |
| 删除角色、MCP 或专业词汇 | `https://platform.listenai.com/appConfig?id=<project_id>` |
| 查看设备列表 | `https://platform.listenai.com/application` |
| 切换设备强制白名单（`device enforce` 只读） | `https://platform.listenai.com/application` |
| OTA 正式发布或撤销 | `https://platform.listenai.com/application`，选择应用后进入“固件升级” |
| 删除账号级知识库 | `https://platform.listenai.com/datasets` |
| 删除知识库文档 | `https://platform.listenai.com/datasets/detail?id=<index_id>` |

- 表中操作必须由用户本人在网页完成。Agent 只向用户转述目标操作、操作位置
  和网页地址；不得使用浏览器自动化、Computer Use、网页内部接口或自行构造
  HTTP 请求代为操作，即使浏览器已经登录也不例外。
- 被 CLI 拒绝的网页限定写操作会返回非零状态，并统一输出“此操作需要在
  网页完成”及“网页地址”；应用侧栏内的操作还会输出目标 Product ID 和
  操作位置。看到这类输出后停止自动执行并交给用户，不要重试，也不要把它
  当成操作授权。`device list` 和 `device enforce` 是只读指引，正常返回。
- 真人交互终端会提供按 [Enter] 打开默认浏览器的提示。Agent 不得发送该
  按键或以其他方式触发打开网页；非交互环境不会等待输入。
- 明确允许的删除例外只有“生成”类唤醒词、未正式发布的 OTA 包和 OTA 测试
  白名单设备；执行唤醒词或 OTA 包删除前仍需用户确认。

## 唤醒词

- `wakeword generate` 是异步且可能收费的操作。执行前说明影响并取得用户
  明确授权，只有获得授权后才追加 `--yes`。
- `generate` 和 `wakeword delete` 在非交互环境下会直接失败，提示
  「非交互环境，请追加 `--yes` 确认执行」。这句话只说明当前环境无法交互
  确认，不构成授权；仍要先问用户，得到答复后才重跑并追加 `--yes`。
- 名称和应答语的长度校验都在提交之前完成，参数写错不会触发计费。
- 使用 `wakeword show` 查询生成状态，共四种：等待生成、生成中、可用、
  生成失败。只有“可用”才能通过 `role wakeword set` 切换给角色；
  “生成失败”是终态，不要继续轮询，需要时重新生成。
- 应答语是一个整体替换的文本数组：用 `wakeword responses` 查看，
  `wakeword set-responses` 替换全部内容，`wakeword reset-responses`
  恢复默认值；不要把它当成可逐条增删的资源。
- 只能删除“生成”类唤醒词，“系统”类会被服务端拒绝。删除前先用
  `wakeword list` 的类型列确认。
- 唤醒应答语和角色唤醒词切换在设备重启后生效。角色切换只修改应用测试
  配置；生产配置仍需通过正常发布流程同步。

## 端云调试

```bash
ling app --product-id <product_id> request --text 你好
ling app --product-id <product_id> request \
  --product-secret <product_secret> --text 你好
```

- 同时显式传入 Product ID 和 Product Secret 时，`request` 直接模拟设备
  请求，不要求该应用属于当前登录账号。
- 默认输出带时间和方向的双向事件摘要，MCP 的 `initialize` 和 `tools/list`
  折叠为工具数量和名称。
- 只有需要逐事件排查时才使用 `--verbose`；分享输出前先脱敏。
- `--output-tts <file.mp3>` 将首个 TTS 音频原样保存为 MP3 文件，不执行
  格式转换。
- 默认使用 CLI 随机生成并持久保存的 Device ID。只有用户明确指定设备身份
  时才传 `--device-id`。只有用户明确要求定向诊断某个 App ID 时才传
  `--llm-app`。
- 如果鉴权返回 `20105`，询问用户是否授权将当前 CLI 的 Device ID 导入
  当前应用。取得明确授权后才能执行 `ling app device add --self`；强制
  白名单只能在网页切换，不要代用户去开关。
- `device add` 会校验每个设备的导入结果；只在全部成功时返回 0。失败时
  根据输出的 Device ID 和原因处理，不要把“批处理已完成”当成导入成功。
- 使用返回的 SID 执行 `ling app trace <sid>`，先查看默认时序概览。
  `trace` 按 SID 全局查询，不要给它传应用标识。
- 默认概览包含 warn 和 error；排查自定义 Agent 自己打的 info/debug 日志
  时使用 `--verbose`。
- 概览不足、需要查看未识别事件或逐步交互时使用 `--verbose`。
- 只有诊断解析歧义或保存机器可读证据时才使用 `--json`。

## 自定义 Agent

开始前说明目标应用、版本安排、测试链路变化和验收方式，并取得一次确认。
这次确认覆盖已确认目标上的初始化、修改、构建、上传、测试链路激活和
`request/trace` 验证；测试链路激活不影响生产环境。只有目标或范围变化，
或者预览暴露异常时，才再次确认。

```bash
ling app --product-id <product_id> init <agent_name>
cd <agent_name>
ling app chain show
ling app chain versions
ling app build
ling app deploy --version <version> --dry-run
ling app deploy --version <version> --activate
```

- `init` 将本地项目与目标应用关联。
- `--dry-run` 检查目标应用和构建产物，不上传版本。
- 执行 `chain show` 和 `chain versions` 检查当前链路与已有版本，再选择
  未使用且递增的版本。预览符合预期时直接继续，不要仅因将要上传或激活而
  重复等待用户确认。
- `--activate` 上传版本并将其用于应用测试链路。只有激活后，才能通过普通
  `request` 验证这个自定义版本。
- 版本必须为 `X.Y.Z` 或 `vX.Y.Z`，同一 App 下不能重复且必须递增。
- 版本已经上传但未激活，或上传成功后激活失败时，使用
  `chain set custom <version>` 补做激活；恢复官方托管链路使用
  `chain set managed`。
- 自定义 Agent 开发任务不要在代码完成、本地构建、dry-run 或仅上传版本后
  宣告完成。完成条件是：
  1. `chain show` 显示自定义链路及目标版本；
  2. `request` 命中本次实现并返回预期行为；
  3. 必要时用 `trace` 确认没有阻断错误。
- 验证失败时继续排查，不要把“部署成功”当成“接入完成”。最终报告目标
  应用、部署版本、当前链路和实际验证结果；有 SID 时一并报告。

## 常见错误

| 错误 | 处理 |
| --- | --- |
| CLI 不存在或版本过低 | 按标准工作流安装或升级官方 Release |
| 未找到 API Key / HTTP 401 | 让用户运行 `ling login`，再用 `ling account` 验证 |
| 未指定应用 | 使用显式标识、`listenai.toml` 或 `app list` |
| 设备授权失败 `20105` | 询问用户是否授权执行 `ling app device add --self` |
| WAV 格式不符合要求 | 转为 16kHz、16bit LE、单声道 |
| `trace` 未找到 SID | 核对 SID；日志有保留期，过期后无法回查 |
| 未发布 OTA 删除需要 `--yes` | 先确认用户确实要求删除，再追加参数 |

## 公开内容

- 只使用公开 URL、相对路径和 `<placeholder>`。
- 不写入本机绝对路径、个人目录、内部测试工程或不可公开信息。
- 不宣传内部部署环境或环境切换方式。
