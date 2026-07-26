# ListenAI ling 标准工作流

本文件面向公开用户环境。不要写本机绝对路径、个人目录或内部测试工程。

## 目录

- [登录与状态确认](#1-登录与状态确认)
- [开发前方案确认](#2-开发前方案确认)
- [云端 Agent 工作流](#3-云端-agent-工作流)
- [真实设备 PID/SID 切换](#4-真实设备-pidsid-切换)
- [端侧固件工作流](#5-端侧固件工作流)

## 1. 登录与状态确认

1. 先确认 CLI 可用：

   ```bash
   ling --version
   ```

2. 如果用户未登录，提示用户打开 `https://platform.listenai.com/keys` 获取 API Key。
3. 运行登录命令，让用户在交互提示中粘贴 API Key：

   ```bash
   ling login
   ```

4. 登录后验证账号：

   ```bash
   ling account
   ```

5. 告诉用户下一步可以执行：`ling ai models`、`ling app list`、`ling app init <agent_name> --product-id <product_id>`、或设备 PID/SID 切换流程。

不要在回复、日志或截图中展示完整 API Key。

## 2. 开发前方案确认

在执行以下动作前，先输出方案并等待用户确认：创建项目、修改代码、构建、部署、拉仓库、写设备配置。

方案包含：

- 需求理解
- 选择的链路：云端 Agent、设备 PID/SID 切换、端侧固件
- 将执行的命令
- 会修改或访问的对象
- 验收方式
- 敏感信息处理方式

如果用户需求已明确，可以复述判断并给方案；如果缺少 Product ID、Agent 名称、目标设备等关键信息，先提一个必要问题。

## 3. 云端 Agent 工作流

新建项目：

```bash
ling app list
ling app init <agent_name> --product-id <product_id>
cd <agent_name>
```

`ling app init` 会把目标应用写入项目根目录的 `listenai.toml`。如果用户尚未确定应用，先用 `ling app list` 查询；不要替用户猜测 `<product_id>`。

应用定位默认使用 Product ID，并自动转换到云端 Project ID。也可以显式传 `--project-id <project_id>` 或 `--app-id <app_id>`；三种 ID 参数互斥。`ling app list` 只展示已关联 Product ID、可由 CLI 管理的应用。

高危操作遵循原始需求边界：原则上不由 CLI 删除资源，OTA 正式发布/撤销、设备强制白名单切换等也只引导网页操作；删除未正式发布的 OTA 包和维护 OTA 测试白名单是明确例外。`ling app delete` 保留命令入口但绝不调用删除 API。应用级提示链接必须包含已解析的 Project ID：`https://platform.listenai.com/appConfig?id=<project_id>`。

构建与本地调试：

```bash
ling app build
ling app dev
```

部署：

```bash
ling app deploy --product-id <product_id> --version <version>
```

可先 dry-run：

```bash
ling app deploy --product-id <product_id> --version <version> --dry-run
```

## 4. 真实设备 PID/SID 切换

用于“切设备 PID”“切应用”“换设备绑定”等需求。此流程不需要拉端侧仓库，不需要编译固件。

1. 用 `ling app list` 确认目标 `<product_id>`；不要替用户猜测应用。
2. 明确请用户本人前往平台网页的应用详情查看产品密钥。Agent 不得要求用户把 Secret 粘贴到对话、日志或截图中，也不得代替用户保存它。
3. 如需发起端云链路模拟请求，让用户在自己的终端执行：

   ```bash
   ling app --product-id <product_id> request --product-secret '<product_secret>' --text <text>
   ```

   也可以临时设置 `LING_PRODUCT_SECRET`，避免 Secret 进入 shell 历史。
   默认输出带方向的双向事件摘要；逐帧诊断使用 `--verbose`，保存语音使用
   `--output-tts <FILE>`。完整输出约定见
   [命令参考](commands.md#端云请求与日志)。
   `request` 显式设置 `llm_ws_version=2.0`；设备入口
   `/v1/interaction` 的路径版本不代表内部 LLM 链路版本。
4. 绑定真实设备时，将 Product ID 作为 PID，将产品密钥作为 SID，由用户在自己的终端写入。交互式：

   ```bash
   adb shell
   device set_pid <product_id>
   device set_sid <product_secret>
   ```

   或非交互式：

   ```bash
   adb shell device set_pid <product_id>
   adb shell device set_sid <product_secret>
   ```

5. 让用户重新唤醒或重连设备，验证新产品配置生效。

安全要求：不要把 `<product_secret>` / SID 明文写入公开回复、日志或截图；如需说明，只展示脱敏形式。

## 5. 端侧固件工作流

只有用户明确要求固件源码、SDK、开发板编译、烧录、`arcs_mini` 相关开发时，才进入端侧仓库流程。

单纯的 PID/SID 切换、应用切换、设备绑定切换，一律使用第 4 节的平台
Product Secret + `adb shell device set_pid/set_sid` 流程。
