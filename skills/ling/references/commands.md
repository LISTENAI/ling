# Ling 命令参考

适用于 `ling >= 1.0.0`。只读取与当前任务有关的章节。

所有分页列表的页码范围为 `1..=1000`，每页数量范围为 `1..=100`。
对应参数分别为 `--page` 和 `--page-size`；知识库命令使用 `--size`。

## 安装与账号

```bash
ling --version
ling login
ling login --json
ling account
ling account --json
```

登录由用户本人完成。API Key 获取地址：
`https://platform.listenai.com/keys`。

## AI 能力

```bash
ling ai models
ling ai models --json
ling ai chat "你好"
ling ai chat "解释 RAG" --stream
ling ai chat "解释 RAG" --json

ling ai tts 你好
ling ai tts --vcn <vcn> --speed 60 你好
ling ai tts --format pcm --sample-rate 16000 -o hello.pcm 你好

ling ai asr hello.pcm
ling ai asr hello.wav --vad-eos 800
ling ai asr hello.wav --verbose
ling ai asr hello.pcm --json
```

`ling ai tts` 不提供发音人列表，也不在本地限定 `--vcn` 的取值；使用用户
明确提供的 VCN，由平台根据 API Key 权限和发音人是否存在决定能否合成。
TTS 返回 0 字节音频时应当报错，不要把 URL 或空文件作为成功结果交付。
`--speed`、`--volume` 和 `--pitch` 范围均为 `1..=100`；
`--emotion-scale` 范围为 `-20..=20`。

ASR 音频应为 16kHz、16bit LE、单声道 PCM；WAV 会先校验格式。识别卡住、
超时或服务端报错时使用 `--verbose` 查看上下行控制帧和音频帧摘要；最终文本
或 `--json` 结果仍写到标准输出。`--verbose` 的帧里含会话标识和完整识别
文本，展示前先脱敏。

## 应用查询

```bash
ling app list
ling app list --page 2 --page-size 20
ling app list --json

ling app inspect <product_id>
ling app --project-id <project_id> inspect
ling app --app-id <app_id> inspect
ling app inspect <product_id> --json
```

`inspect` 默认输出摘要；`--json` 输出服务端原始响应。输出可能包含敏感应用
信息，展示前先脱敏。

## 应用配置

标识参数推荐紧跟在 `ling app` 后；项目目录内可从 `listenai.toml` 读取
`product_id`，无需重复传入。

```bash
ling app create <name>
ling app create <name> --description <description>

ling app role list
ling app role show <role_id>
ling app role create <name> --set persona='"..."' --set vcn=<vcn>
ling app role edit <role_id> --set speed=60
ling app role set-default <role_id>
ling app role wakeword show <role_id>
ling app role wakeword set <role_id> <wakeword_id>

ling app wakeword list
ling app wakeword list --page 2 --page-size 20
ling app wakeword show <wakeword_id>
ling app wakeword generate 小聆小聆 \
  --response "你好，我在"
ling app wakeword generate 小聆小聆 \
  --sensitivity high
ling app wakeword responses <wakeword_id>
ling app wakeword set-responses <wakeword_id> \
  "你好" "我在"
ling app wakeword reset-responses <wakeword_id>
ling app wakeword delete <wakeword_id> --yes

ling app kb list
ling app kb link <index_id>
ling app kb unlink <index_id>

ling app lexicon list
ling app lexicon add <word>
ling app lexicon import words.txt
ling app lexicon edit <hotword_id> <word>

ling app tone show
ling app tone edit --set key=text
ling app tone edit --reset key
ling app tone edit --reset key-a --reset key-b --set key-a=text
ling app tone edit --reset-all

ling app mcp list
ling app mcp show <mcp_id>
ling app mcp add <name> --server-id <id> \
  --transport http --url <url>
ling app mcp edit <mcp_id> --set enabled=on
ling app mcp enable <mcp_id>
ling app mcp disable <mcp_id>

ling app config show
ling app config edit --set name=<name>
ling app config edit --set description=<description>
ling app config edit --set interaction_mode=full-duplex
ling app config edit --set system_prompt='"..."'
ling app config reset-model
ling app config test-model --endpoint <url> --model <model>

ling config device-id show
ling config device-id reset

ling app device quota
ling app device add --self
ling app device add <device_id>...
ling app device add --file devices.txt
ling app device query <device_id>
ling app device enforce

ling app ota list
ling app ota upload firmware.bin --version 2.4.0 \
  --version-number 240 --ota-mode mandatory
ling app ota show <package_id>
ling app ota edit <package_id> --description "修订说明"
ling app ota delete <package_id> --yes
ling app ota whitelist list
ling app ota whitelist add <device_id>
ling app ota whitelist add --self
ling app ota whitelist delete <device_id>
```

- 复制 `ota list` 或 `ota show` 展示的“OTA 包 ID”，用于后续的 `show`、
  `edit` 和 `delete`。
- `ota edit` 可替换固件文件，或修改 `--version` 和 `--description`。
- `role show` 以表格列出 `role edit --set` 可用的准确 Key、当前值、类型和
  限制；长文本和一对多配置会在表格下方展开。
- 网页中的“角色描述”对应 `persona`，创建或修改时使用
  `--set persona=...`。
- `wakeword generate` 的唤醒词名最多 12 个字符，`--sensitivity` 取
  `low`/`medium`/`high`（默认 `medium`）。可不传应答语，最多接受 5 条
  `--response`；单条应答语最多 12 个字符。以上校验都在提交前完成，参数
  写错不会触发计费。
- 生成是异步且可能收费的操作。`generate` 和 `wakeword delete` 在非交互
  环境下直接失败并提示追加 `--yes`；该提示不构成授权，取得用户明确授权
  后才追加。
- `wakeword show` 的状态有四种：等待生成、生成中、可用、生成失败。只有
  “可用”能通过 `role wakeword set` 切换给角色；“生成失败”是终态，不要
  继续轮询。
- `wakeword list` 的类型列区分“系统”和“生成”。只能删除“生成”类，
  删除系统唤醒词会被服务端拒绝。
- 应答语或角色唤醒词修改后需要重启设备；角色切换只修改应用测试配置，
  生产配置仍需正常发布。
- `mcp list` 同时展示记录 ID 和 Server ID。后续操作使用记录 ID；
  `mcp show` 不输出 Authorization 明文。
- `config show` 默认以表格列出可编辑 Key、枚举值和格式约束；
  `--json` 输出结构化 `editable_fields`。Key 以 `config show` 的写法为准。
- `config edit` 成功后回显本次实际改动的字段名。
- `device enforce` 只读，说明当前接入规则，并引导用户从
  `https://platform.listenai.com/application` 选择目标应用后修改。
- `device list` 不查询设备数据，直接引导用户从同一应用列表进入设备管理
  查看。
- `device add` 是同步导入。只要有一个 Device ID 导入失败，就会列出其 ID
  和原因并返回非零状态；`--json` 仍输出完整响应，但退出状态保持失败。
- `tone` 中的值是合成设备提示音的文案，不是音频文件。

### `--set` 的取值规则

`role`、`mcp`、`config`、`tone` 的 `--set key=value` 按同一套规则解析取值：

- `on` 和 `off` 解析为布尔真假。
- 其余取值先按 JSON 解析，解析失败才当作字符串。

因此 `--set speed=60` 传出的是数字，`--set enabled=on` 是布尔。当文案本身
可能被当成 JSON——纯数字、`true`/`false`/`null`、以 `[` 或 `{` 开头——必须
用 `--set key='"文本"'` 显式包成 JSON 字符串。`config edit` 会当场报
「必须是字符串」，其余命令会把错误的类型直接发给服务端。

取值含引号、换行或结构化内容时，改用 `--file <file.json>` 传完整请求对象：

```bash
ling app role edit <role_id> --file role.json
ling app config edit --file config.json
ling app mcp edit <mcp_id> --file mcp.json
ling app tone edit --file tones.json
```

## 网页交接命令

以下命令只显示网页入口，不执行目标操作：

```bash
ling app --product-id <product_id> delete
ling app role delete <role_id>
ling app mcp delete <mcp_id>
ling app lexicon delete <hotword_id>
ling app device list
ling app ota publish <package_id>
ling app ota revoke <package_id>

ling kb delete <index_id>
ling kb doc <index_id> delete <doc_id>...
```

应用删除、设备列表、强制白名单和 OTA 发布/撤销位于应用列表的应用侧栏；
CLI 会提示目标 Product ID 和对应标签。角色、MCP 与专业词汇删除位于独立的
应用配置页。知识库相关命令会进入知识库列表或指定知识库详情；删除整个
知识库时，CLI 会显示知识库名称和 ID 供用户核对，名称无法取得时仍显示 ID。

网页交接错误使用统一格式：

```text
此操作需要在网页完成：<操作>
[目标应用：<product_id>]
[操作位置：<页面路径>]
网页地址：<url>
```

方括号内两行仅在需要先定位应用及侧栏模块时出现。被 CLI 拒绝的写操作返回
非零状态，用于提示操作边界；不要将它当成临时网络错误重试。`device list`
和 `device enforce` 是只读指引，正常返回。

真人交互终端随后显示：

```text
按 [Enter] 在浏览器中打开该链接；按 [Ctrl-C] 取消。
```

非交互环境不等待输入。打开浏览器只完成网页交接，不代表目标操作成功，因此
原命令仍返回非零状态。

切换设备强制白名单没有对应的 CLI 命令，只能在网页操作；`ling app device
enforce` 会连同当前状态一起给出入口。

网页交接后的处理遵循主 Skill 的[网页操作边界](../SKILL.md#网页操作边界)；
命令参考不改变该授权边界。

## 端云请求与日志

```bash
ling app --product-id <product_id> request --text 你好
ling app --product-id <product_id> request \
  --product-secret <product_secret> --text 你好
ling app --product-id <product_id> request --file hello.pcm
ling app --product-id <product_id> request --text 你好 --verbose
ling app --product-id <product_id> request \
  --text 你好 --output-tts reply.mp3

ling app trace <sid>
ling app trace <sid> --verbose
ling app trace <sid> --json
```

对于当前账号可管理的应用，`request` 自行取得鉴权信息。模拟无权管理的应用
时，用户在自己的终端同时显式提供 Product ID 和完整 Product Secret，即可
绕过应用管理查询直接模拟设备请求。它不提供 `--json`：默认输出人类可读
时间线，`--verbose` 逐行输出带方向的原始诊断事件。请求汇总和鉴权错误会
显示实际 Device ID；`--device-id` 只覆盖本次请求。
默认值由 CLI 随机生成并持久保存。CLI 不推断 Device ID 的合法性，显式值
和持久化值都会原样交给服务端，并展示服务端返回的错误。运行
`ling config device-id show` 可查看本地 ID，运行
`ling config device-id reset` 可重新生成；这两个命令是纯本地操作，不
需要应用标识，也不访问平台。应用启用设备白名单时，取得用户明确授权后可
运行 `ling app device add --self` 导入当前 CLI 的 ID；OTA 测试白名单使用
`ling app ota whitelist add --self`。
`--llm-app` 只用于用户明确要求的定向诊断。
`--output-tts <file.mp3>` 将首个 TTS 音频原样保存为 MP3 文件，不执行格式
转换。

默认时间线把 `initialize` 和 `tools/list` 折叠成工具数量和名称摘要；需要
完整的工具描述和 JSON Schema 时用 `--verbose`。`tools/call` 的参数和结果
在默认输出里就是完整的。

`trace` 按 SID 全局查询，自己解析出所属应用，因此不接受 `--product-id`、
`--project-id` 或 `--app-id`，也没有时间窗参数。默认提炼关键时序事件，并
始终显示 warn 和 error 级别的日志；info 和 debug 只在 `--verbose` 里出现。
需要保存机器可读记录时使用 `--json`。详细输出可能包含完整请求上下文和
工具结果。

自定义 Agent 用 SDK 的 `log.info` / `log.debug` 打的日志默认不显示，排查
自己的 Agent 时用 `--verbose`。`log.warn` 和 `log.error` 默认可见。

## 自定义 Agent 项目

```bash
ling app --product-id <product_id> init <agent_name>
ling app init <agent_name> --no-install
ling app build
ling app build --release
ling app deploy --version v1.0.0 --dry-run
ling app --product-id <product_id> deploy \
  --version v1.0.0 --version-name 首次发布 --activate
ling app chain show
ling app chain versions
ling app chain set custom v1.0.0
ling app chain set managed
```

`--activate` 使上传版本成为当前测试链路。普通 `request` 无法调用未激活的
指定版本。

## 知识库

```bash
ling kb list
ling kb create 产品手册
ling kb doc <index_id> list
ling kb doc <index_id> add \
  --name 说明书.txt --url https://example.com/manual.txt
ling kb query <index_id> 空调怎么开 --limit 5 --threshold 0.3
```

应用关联的知识库使用 `ling app kb`，账号级知识库使用 `ling kb`。

## 文档搜索

```bash
ling wiki search 标准API
ling wiki search "标准API" "获取密钥"
ling wiki search 标准API --json
```

单关键词最多显示 20 条；多关键词按组显示，每组最多 5 条。结果包含稳定文档
ID；当前版本不提供正文读取命令。
