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

本 Skill 要求 `ling >= 0.2.0`。先运行：

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

在初始化项目、修改代码、构建、部署、拉取端侧仓库、编译或烧录前，先向用户
说明选择的路径、将操作的对象和验收方式，并等待确认。这项确认用于防止把
简单的平台或设备配置需求扩大成不必要的云端、端侧开发。

普通查询、用户已经指定的应用配置操作、`request/trace` 和 PID/SID 配置不应
因此自动触发代码拉取或构建。

## 3. 平台操作

目标应用按以下顺序确定：

1. 用户显式给出的 Product ID、Project ID 或 App ID；
2. 当前目录 `listenai.toml` 中的 `product_id`；
3. 运行 `ling app list` 后由用户确认。

不要替用户猜目标应用。应用、角色、MCP、知识库、文档和专业词汇删除，以及
OTA 正式发布/撤销和设备列表，只使用 CLI 返回的网页入口。强制白名单没有
写入命令，`ling app device enforce` 只显示当前状态和网页入口。

`tone` 操作的是合成提示音所使用的文案，不是音频文件。

## 4. 自定义 Agent 工作流

用户确认进入自定义 Agent 开发后：

```bash
ling app --product-id <product_id> init <agent_name>
cd <agent_name>
ling app build
ling app deploy --version <version> --dry-run
```

确认预览正确后，部署并激活应用测试链路：

```bash
ling app deploy --version <version> --activate
```

不激活时只能完成构建、预览或上传，普通 `request` 无法定向调用某个未激活
版本。要验证自定义实现，必须先使该版本成为应用当前测试链路：

```bash
ling app chain show
ling app --product-id <product_id> request --text <text>
ling app trace <sid>
```

默认先阅读人类可读时间线；需要逐事件排查时再使用 `--verbose`，需要机器可读
记录时使用 `--json`。

## 5. 真实设备 PID/SID 切换

用于“切设备 PID”“切应用”“换设备绑定”等需求。此流程不需要拉取或编译
任何代码仓库。

1. 按目标应用选择规则确认 Product ID。
2. 让用户在自己的终端准备设备 SID，不要要求其把敏感值粘贴到对话中。
3. 由用户在自己的终端写入：

   ```bash
   adb shell device set_pid <product_id>
   adb shell device set_sid <sid>
   ```

4. 重新唤醒或重连设备，验证应用配置生效。

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
