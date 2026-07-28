# Ling 命令参考

适用于 `ling >= 0.2.0`。只读取与当前任务有关的章节。

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
ling ai tts --vcn x5_lingyuzhao_flow --speed 60 你好
ling ai tts --format pcm --sample-rate 16000 -o hello.pcm 你好
ling ai tts --list-vcn

ling ai asr hello.pcm
ling ai asr hello.wav --vad-eos 800
ling ai asr hello.pcm --json
```

用 `ling ai tts --list-vcn` 查看支持的发音人。TTS 返回 0 字节音频时应当报错，
不要把 URL 或空文件作为成功结果交付。

ASR 音频应为 16kHz、16bit LE、单声道 PCM；WAV 会先校验格式。

## 应用查询

```bash
ling app list
ling app list --page 2 --page-size 20
ling app list --service-type device
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

ling app role list
ling app role show <role_id>
ling app role create <name> --set persona='"..."' --set vcn=<vcn>
ling app role edit <role_id> --set speed=60
ling app role set-default <role_id>

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
ling app config edit --set interaction-mode=full-duplex
ling app config edit --set system-prompt='"..."'
ling app config reset-model
ling app config test-model --endpoint <url> --model <model>

ling app device quota
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
ling app ota whitelist delete <device_id>
```

- `role show` 会列出 `role edit --set` 可用的准确 Key、类型和限制。
- `mcp list` 同时展示记录 ID 和 Server ID。后续操作使用记录 ID；
  `mcp show` 不输出 Authorization 明文。
- `config show` 默认以表格列出可编辑 Key、枚举值和格式约束；
  `--json` 输出结构化 `editable_fields`。
- `tone` 中的值是合成设备提示音的文案，不是音频文件。

## 网页交接命令

以下命令只显示网页入口，不执行目标操作：

```bash
ling app --product-id <product_id> delete
ling app role delete <role_id>
ling app mcp delete <mcp_id>
ling app lexicon delete <hotword_id>
ling app device list
ling app device enforce on
ling app ota publish <package_id>
ling app ota revoke <package_id>

ling kb delete <index_id>
ling kb doc <index_id> delete <doc_id>...
```

不要绕过这些入口直接调用删除、发布或白名单切换接口。

## 端云请求与日志

```bash
ling app --product-id <product_id> request --text 你好
ling app --product-id <product_id> request --file hello.pcm
ling app --product-id <product_id> request --text 你好 --verbose
ling app --product-id <product_id> request \
  --text 你好 --output-tts reply.mp3

ling app trace <sid>
ling app trace <sid> --verbose
ling app trace <sid> --json
ling app trace <sid> --hours 2
```

`request` 自行完成应用鉴权，不需要额外凭据参数。它不提供 `--json`：默认
输出人类可读时间线，`--verbose` 逐行输出带方向的原始诊断事件。请求汇总和
鉴权错误会显示实际 Device ID；`--device-id` 只覆盖本次请求。
`--llm-app` 只用于用户明确要求的定向诊断。

`trace` 默认提炼关键时序事件。概览不足时使用 `--verbose`；需要保存机器可读
记录时使用 `--json`。详细输出可能包含完整请求上下文和工具结果。

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
