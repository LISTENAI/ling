# Ling 命令参考

只读取与当前任务有关的章节。

## 目录

- [安装](#安装)
- [登录与账号](#登录与账号)
- [AI 能力](#ai-能力)
- [应用查询](#应用查询)
- [应用管理](#应用管理)
- [端云请求与日志](#端云请求与日志)
- [Agent 项目](#agent-项目)
- [知识库](#知识库)
- [文档搜索](#文档搜索)

## 安装

先运行：

```bash
ling --version
```

成功时继续使用当前版本，不要无故重装。命令缺失时安装官方 Release。

macOS / Linux：

```bash
curl -fsSL https://raw.githubusercontent.com/LISTENAI/ling/main/install.sh | sh
```

Windows PowerShell：

```powershell
irm https://raw.githubusercontent.com/LISTENAI/ling/main/install.ps1 | iex
```

macOS 也可使用 Homebrew：

```bash
brew install LISTENAI/tap/ling
```

安装后再次运行 `ling --version`。安装器已完成二进制运行检查；如果当前
shell 仍找不到命令，按安装器输出处理 PATH。

## 登录与账号

```bash
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

用 `ling ai tts --list-vcn` 查看并选择支持的发音人。TTS 返回 0 字节音频时，
将其视为当前发音人不支持所选参数，不要把对应 URL 或空文件作为成功结果交付。

ASR 音频应为 16kHz、16bit LE、单声道 PCM；WAV 会先校验格式。

## 应用查询

```bash
ling app list
ling app list --page 2 --page-size 20
ling app list --service-type device
ling app list --json

ling app inspect <product_id>
ling app inspect --project-id <project_id>
ling app inspect --app-id <app_id>
ling app inspect <product_id> --json
```

`inspect` 默认输出摘要；`--json` 输出服务端原始响应。服务端返回完整 Product
Secret 时，原始或摘要输出可能包含敏感信息。

## 应用管理

标识参数放在 `ling app` 后；项目目录内可从 `listenai.toml` 读取
`product_id`。

```bash
ling app create <name> --template-id <id>
ling app delete --product-id <product_id>

ling app role list
ling app role show <role_id>
ling app role add <name> --set persona='"..."' --set vcn=<vcn>
ling app role edit <role_id> --set speed=60
ling app role set-default <role_id>
ling app role delete <role_id>

ling app interact-mode
ling app interact-mode oneshot
ling app interact-mode half-duplex
ling app interact-mode full-duplex

ling app kb list
ling app kb link <index_id>
ling app kb unlink <index_id>

ling app lexicon list
ling app lexicon add <word>
ling app lexicon import words.txt
ling app lexicon edit <hotword_id> <word>
ling app lexicon delete <hotword_id>

ling app tone show
ling app tone edit --set key=text
ling app tone edit --reset key
ling app tone edit --reset key-a --reset key-b --set key-a=text
ling app tone edit --reset-all

ling app mcp list
ling app mcp add <name> --server-id <id> \
  --transport http --url <url>
ling app mcp edit <server_id> --set enabled=on
ling app mcp enable <server_id>
ling app mcp disable <server_id>
ling app mcp delete <server_id>

ling app config show
ling app config edit --set system-prompt='"..."'
ling app config reset-model
ling app config test-model --endpoint <url> --model <model>

ling app device quota
ling app device list
ling app device add <device_id>...
ling app device add --file devices.txt
ling app device query <device_id>
ling app device enforce
ling app device enforce on

ling app ota list
ling app ota upload firmware.bin --version 2.4.0 \
  --version-number 240 --ota-mode mandatory
ling app ota get <package_id>
ling app ota edit <package_id> --description "修订说明"
ling app ota publish <package_id>
ling app ota revoke <package_id>
ling app ota delete <package_id> --yes
ling app ota whitelist list
ling app ota whitelist add <device_id>
ling app ota whitelist delete <device_id>
```

`config show` 默认以表格列出可用于 `config edit --set` 的准确 Key。
表格同时给出枚举字段的可用值和其他字段的格式约束。
`config show --json` 输出扁平配置值、凭据配置状态和结构化
`editable_fields`。

删除、发布和设备命令是否实际调用 API，以 `SKILL.md` 的安全边界为准。

## 端云请求与日志

```bash
ling app --product-id <product_id> request \
  --product-secret '<product_secret>' --text 你好
ling app --product-id <product_id> request \
  --product-secret '<product_secret>' --file hello.pcm
ling app --product-id <product_id> request \
  --product-secret '<product_secret>' --text 你好 --verbose
ling app --product-id <product_id> request \
  --product-secret '<product_secret>' --text 你好 --output-tts reply.mp3

ling app trace <sid>
ling app trace <sid> --verbose
ling app trace <sid> --json
ling app trace <sid> --hours 2
```

`request` 不提供 `--json`。默认输出人类可读时间线；`--verbose` 逐行输出带
方向的原始诊断事件。请求汇总和鉴权错误会显示实际 Device ID；
`--device-id <device_id>` 只覆盖本次请求。`--llm-app <app_id>` 只用于用户
明确要求的定向诊断，默认请求不要传。

`trace` 默认提炼关键时序事件。默认概览不足、需要查看未识别事件或逐步交互时
使用 `--verbose`；需要保留机器可读记录时使用 `--json`。两种详细输出都可能
包含完整请求上下文和工具结果，展示前先脱敏。

## Agent 项目

```bash
ling app init <agent_name> --product-id <product_id>
ling app init <agent_name> --no-install
ling app build
ling app build --release
ling app deploy --version v1.0.0 --dry-run
ling app deploy --product-id <product_id> \
  --version v1.0.0 --version-name 首次发布 --activate
ling app chain versions
ling app chain set custom v1.0.0
ling app chain set managed
```

## 知识库

```bash
ling kb list
ling kb create 产品手册
ling kb delete <index_id>
ling kb doc <index_id> list
ling kb doc <index_id> add \
  --name 说明书.txt --url https://example.com/manual.txt
ling kb doc <index_id> delete <doc_id>...
ling kb query <index_id> 空调怎么开 --limit 5 --threshold 0.3
```

知识库和文档删除只给出网页入口，不调用删除 API。

## 文档搜索

```bash
ling wiki search 标准API
ling wiki search "标准API" "获取密钥"
ling wiki search 标准API --json
```

单关键词最多显示 20 条；多关键词按组显示，每组最多 5 条。
结果包含稳定的文档 ID；当前版本不提供正文读取命令。
