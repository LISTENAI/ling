# ListenAI ling 标准工作流

本文件面向公开用户环境。不要写本机绝对路径、个人目录或内部测试工程。

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

如用户指定环境，把 `--api-base-url` 放在子命令前：

```bash
ling --api-base-url <api_base_url> app init <agent_name> --product-id <product_id>
```

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

1. 确认目标 `<product_id>`。
2. 获取产品信息和密钥：

   ```bash
   ling app inspect <product_id>
   ```

3. 将输出中的“产品 ID”作为 PID，将“密钥”作为 SID。
4. 写入设备。交互式：

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

单纯的 PID/SID 切换、应用切换、设备绑定切换，一律使用第 4 节的 `ling app inspect` + `adb shell device set_pid/set_sid` 流程。
