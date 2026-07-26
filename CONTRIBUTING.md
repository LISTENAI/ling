# Contributing to ling

本文件面向仓库维护者。最终用户的安装和使用说明见 [README.md](README.md)。

## 开发环境

仓库通过 `rust-toolchain.toml` 固定 Rust 工具链，并包含 `rustfmt` 和
`clippy`。检出仓库后，Cargo 会自动使用对应版本。

常用命令：

```bash
make fmt
make test
make lint
make build
```

- `make test` 运行 workspace 单元测试。
- `make lint` 运行格式检查和严格 Clippy。
- `make build` 构建 release 二进制。

## 本地安装

默认安装到 `~/.cargo/bin/ling`：

```bash
make install
ling --help
```

安装到其他目录：

```bash
make install INSTALL_ROOT="$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
ling --version
```

也可以直接使用 Cargo：

```bash
cargo install --path crates/ling --locked --force --root "$HOME/.local"
```

如果命令仍然指向旧版本，使用 `type -a ling` 检查 PATH 中的所有副本。

## Docker Compose

不希望在本机安装 Rust 工具链时，可以使用 Docker：

```bash
make docker-test
make docker-lint
make docker-build
```

等价命令：

```bash
docker compose run --rm test
docker compose run --rm lint
docker compose run --rm dev cargo build --release
```

## 提交前检查

至少运行：

```bash
make lint
make test
make build
```

提交规则和历史维护要求见 [AGENTS.md](AGENTS.md)。
