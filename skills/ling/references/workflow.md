# ListenAI ling 标准工作流

本文件面向公开用户环境。

## 目录

- [CLI 检测与安装](#0-cli-检测与安装)
- [登录与状态确认](#1-登录与状态确认)
- [任务分流与方案确认](#2-任务分流与方案确认)
- [平台操作](#3-平台操作)
- [自定义 Agent 工作流](#4-自定义-agent-工作流)
- [真实设备 PID/SID 切换](#5-真实设备-pidsid-切换)
- [端侧任务转交](#6-端侧任务转交)

## 0. CLI 检测与安装

本 Skill 要求 `ling >= 1.0.0`。先运行：

```bash
ling --version
```

版本满足要求时继续使用。版本过低、命令不存在或无法执行时，说明将从
`LISTENAI/ling` 的 GitHub Release 安装官方二进制，再按平台执行。

macOS / Linux：

```bash
curl -fsSL https://raw.githubusercontent.com/LISTENAI/ling/main/install.sh | sh
```

Windows PowerShell：

```powershell
irm https://raw.githubusercontent.com/LISTENAI/ling/main/install.ps1 | iex
```

macOS 也可使用：

```bash
brew install LISTENAI/tap/ling
```

安装需要联网并写入用户目录，使用运行环境提供的授权机制。Agent 能执行时
直接完成，不要只把命令交给用户。不要克隆源码仓库、运行 `cargo install`
或自行构建开发版本。

安装后再次运行 `ling --version`。当前 shell 尚未刷新 PATH 时，使用安装器
返回的二进制路径验证；不要擅自修改用户的 shell 配置。

## 1. 登录与状态确认

1. 用户未登录时，请其打开 `https://platform.listenai.com/keys` 获取 API Key。
2. 运行 `ling login`，由用户本人在交互提示中输入密钥。
3. 运行 `ling account` 验证账号。

不要要求用户把完整 API Key 粘贴到对话、日志或截图中。

## 2. 任务分流与方案确认

先判断满足需求所需的最小路径：

- 基础 AI 或平台配置：直接使用对应的 `ling ai`、`ling app` 或 `ling kb`
  命令。
- 模拟请求和日志回查：使用 `request/trace`。
- 真实设备只需切换应用：使用 PID/SID 配置流程，不进入代码开发。
- 自定义云端逻辑：进入 Agent 项目流程。
- 固件源码、编译或烧录：转交专用端侧 Skill。

在进入自定义 Agent 或端侧开发流程前，先向用户说明选择的路径、将操作的
对象、会产生的外部变化和验收方式，并等待一次确认。这项确认用于防止把简单
的平台或设备配置需求扩大成不必要的云端、端侧开发。确认后，在已说明的目标
和范围内完成正常的修改、构建、测试链路部署、激活和验证，不要为每一步重复
询问；目标、范围或风险发生实质变化时再重新确认。

普通查询、用户已经指定的应用配置操作、`request/trace` 和 PID/SID 配置不应
因此自动触发代码拉取或构建。

## 3. 平台操作

目标应用按以下顺序确定：

1. 用户显式给出的 Product ID、Project ID 或 App ID；
2. 当前目录 `listenai.toml` 中的 `product_id`；
3. 运行 `ling app list` 后由用户确认。

不要替用户猜目标应用。应用、角色、MCP、知识库、文档和专业词汇删除，以及
OTA 正式发布/撤销和设备列表，只使用 CLI 返回的网页入口。强制白名单没有
写入命令，`ling app device enforce` 只显示当前状态和网页入口。遇到网页
交接时遵循主 Skill 的“网页操作边界”，停止自动执行并由用户本人完成。

`tone` 操作的是合成提示音所使用的文案，不是音频文件。

## 4. 自定义 Agent 工作流

用户要求开发或修改自定义 Agent 时，默认执行完整工作流。只有用户明确要求
只修改代码、本地构建、dry-run 或只上传版本时，才在对应阶段停止。

开始前确认目标应用、版本安排、测试链路变化和验收方式。应用测试链路的部署
和激活不影响生产环境；用户确认该计划后，正常的部署、激活与验证无需再次
询问。

先检查当前状态和已有版本：

```bash
ling app --product-id <product_id> chain show
ling app --product-id <product_id> chain versions
```

选择未使用且递增的版本。尚未初始化项目时：

```bash
ling app --product-id <product_id> init <agent_name>
cd <agent_name>
```

完成代码修改后构建并预览部署：

```bash
ling app build
ling app deploy --version <version> --dry-run
```

自行核对预览中的应用和版本。符合既定计划时直接部署并激活测试链路；只有
预览暴露了错误目标、异常影响或其他实质变化时才暂停并重新确认：

```bash
ling app deploy --version <version> --activate
```

版本已经上传但尚未激活，或上传成功后激活失败时，先确认版本存在，再补做
激活：

```bash
ling app chain versions
ling app chain set custom <version>
```

不激活时只能完成构建、预览或上传，普通 `request` 无法定向调用该版本。
激活后确认当前测试链路，再用能覆盖本次改动的输入验证实际行为：

```bash
ling app chain show
ling app --product-id <product_id> request --text <text>
ling app trace <sid>
```

默认先阅读人类可读时间线；需要逐事件排查时再使用 `--verbose`，需要机器可读
记录时使用 `--json`。如果 `chain show` 不是自定义链路和目标版本，或者
`request` 未命中预期实现，就继续排查，不要宣告接入完成。

完整接入的完成条件：

1. `chain show` 显示自定义链路及目标版本；
2. `request` 返回符合本次实现的行为；
3. 必要时通过 `trace` 确认没有阻断错误。

最终报告目标应用、部署版本、当前链路和实际验证结果；有 SID 时一并报告。

## 5. 真实设备 PID/SID 切换

用于“切设备 PID”“切应用”“换设备绑定”等需求。此流程不需要拉取或编译
任何代码仓库。用户明确要求绑定或切换设备，即授权 Agent 对已确认的设备
执行本节中的 PID 和 SID 写入；不要仅因 Product Secret 敏感而把 SID 写入
交回用户。

1. 按目标应用选择规则确认 Product ID。
2. 运行 `adb devices` 检查设备。没有设备或设备未授权时，引导用户连接并
   接受 USB 调试授权，然后继续同一流程。此时只把无法代办的物理连接和
   设备授权交给用户，并明确连接后 Agent 会继续写入 Product ID 和 Product
   Secret；不要告诉用户稍后自行写入 SID。检测到多台设备时，让用户确认
   目标设备。
3. 捕获 `ling app inspect --json` 的本地输出并取得目标应用的完整 Product
   Secret，不要让该命令的标准输出直接进入回复或日志。只有 CLI 确实取不
   到完整值时，才让用户通过本地隐藏输入补充；不要要求用户把它粘贴到
   对话中。
4. Agent 将 Product ID 和 Product Secret 都写入已确认的设备：

   ```bash
   adb shell device set_pid <product_id>
   adb shell device set_sid <product_secret>
   ```

   向设备传递 Product Secret 时使用不会回显或记录完整值的本地方式。两条
   命令都成功前，不要宣告绑定完成。
5. 重新唤醒或重连设备，验证应用配置生效。

如果需求只是模拟设备请求，不执行上述设备写入，直接使用：

```bash
ling app --product-id <product_id> request --text <text>
```

## 6. 端侧任务转交

涉及固件源码、SDK、开发板、编译或烧录时：

1. 使用当前 Agent 环境提供的 Skill 发现能力，搜索与目标芯片、开发板和任务
   匹配的端侧开发或烧录 Skill。
2. 找到后读取并遵循该 Skill，再向用户说明端侧方案并等待确认。
3. 找不到时明确告知缺少专用端侧能力，并协助用户查找或安装合适的 Skill。

没有专用 Skill 时，不要猜测仓库地址、工具链、构建参数、串口或烧录命令。
