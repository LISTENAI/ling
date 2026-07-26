# ling

ListenAI 本地 CLI 工具。在终端中使用 AI 能力、管理应用和知识库、调试端云
请求，以及开发 Agent 项目。

## 太长不看？交给 AI 处理

为支持 Agent Skill 的 AI 编程助手安装 `ling` Skill：

```bash
npx skills add LISTENAI/ling
```

然后直接告诉 AI 你想完成什么。Skill 会检查本机是否已经安装 `ling`，并协助
完成安装、登录和后续操作。

API Key 和 Product Secret 只在你自己的终端中输入，不要粘贴到聊天中。

## 安装

macOS：

```bash
brew install LISTENAI/tap/ling
```

macOS / Linux：

```bash
curl -fsSL https://raw.githubusercontent.com/LISTENAI/ling/main/install.sh | sh
```

Windows PowerShell：

```powershell
irm https://raw.githubusercontent.com/LISTENAI/ling/main/install.ps1 | iex
```

确认安装成功：

```bash
ling --version
```

## 登录

从 [ListenAI 平台](https://platform.listenai.com/keys) 获取 API Key，然后运行：

```bash
ling login
ling account
```

`ling login` 只显示脱敏预览，不回显完整 API Key。

## 常用用法

### AI

```bash
ling ai models
ling ai chat "你好，请介绍一下自己"
ling ai chat "解释一下 RAG" --stream

ling ai tts 你好，今天天气怎么样
ling ai tts -o hello.mp3 你好

ling ai asr hello.pcm
ling ai asr hello.wav
```

ASR 音频应为 16kHz、16bit LE、单声道 PCM；WAV 会自动校验格式。

### 应用

```bash
ling app list
ling app inspect <product_id>
ling app inspect <product_id> --json
```

大多数应用命令使用 Product ID。在 Agent 项目目录中，`ling` 会读取
`listenai.toml` 中关联的应用。

查看应用配置相关命令：

```bash
ling app --help
ling app role --help
ling app device --help
ling app ota --help
```

### Agent 项目

`ling app init` 需要 Node.js 18 或更高版本：

```bash
ling app init my-agent --product-id <product_id>
cd my-agent
ling app build
ling app dev
```

部署前可以先预览：

```bash
ling app deploy --version v1.0.0 --dry-run
ling app deploy --version v1.0.0
```

### 端云调试

Product Secret 可在 ListenAI 平台的应用详情中查看：

```bash
ling app --product-id <product_id> request \
  --product-secret '<product_secret>' --text 你好
```

查看协议帧或保存返回的 TTS：

```bash
ling app --product-id <product_id> request \
  --product-secret '<product_secret>' --text 你好 --verbose

ling app --product-id <product_id> request \
  --product-secret '<product_secret>' --text 你好 --output-tts reply.mp3
```

使用返回的 SID 回查请求：

```bash
ling app trace <sid>
ling app trace <sid> --full
ling app trace <sid> --json
```

### 知识库

```bash
ling kb list
ling kb create 产品手册
ling kb doc <index_id> list
ling kb doc <index_id> add \
  --name 说明书.txt --url https://example.com/manual.txt
ling kb query <index_id> 空调怎么开
```

应用关联的知识库使用 `ling app kb`，账号级知识库使用 `ling kb`。

### 真实设备绑定

Product ID 可通过 `ling app list` 获取，Product Secret 可在平台应用详情中
查看：

```bash
adb shell device set_pid <product_id>
adb shell device set_sid <product_secret>
```

### 文档搜索

```bash
ling wiki search 标准API
ling wiki search "标准API" "获取密钥"
```

## 输出与帮助

查询命令通常支持 `--json`，适合脚本或其他工具读取：

```bash
ling account --json
ling app list --json
ling wiki search 标准API --json
```

查看所有命令和参数：

```bash
ling --help
ling <command> --help
```

## 凭据安全

- 使用交互式 `ling login` 输入 API Key。
- Product Secret 只用于 `app request` 和真实设备绑定。
- `inspect --json`、`request --verbose` 和 `trace --full` 可能包含敏感信息，
  分享前请先脱敏。
- 部分不适合在 CLI 中完成的操作会直接给出 ListenAI 平台网页入口。

## 更新与排查

Homebrew：

```bash
brew update
brew upgrade ling
```

使用安装脚本的用户可以重新运行对应平台的安装命令获取最新版本。

如果终端找不到 `ling` 或仍然使用旧版本：

```bash
type -a ling
ling --version
```

Windows 安装后如果当前终端尚未刷新 PATH，请重新打开终端。

## 参与开发

仓库构建、测试和 Docker 开发说明见 [CONTRIBUTING.md](CONTRIBUTING.md)。
