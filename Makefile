.PHONY: help install build test lint fmt check docker-test docker-lint docker-build clean

CARGO ?= cargo
PACKAGE ?= ling
BIN ?= ling

help:
	@echo "Targets:"
	@echo "  make install      Install ling to ~/.cargo/bin"
	@echo "  make build        Build release binary"
	@echo "  make test         Run workspace tests locally"
	@echo "  make lint         Run fmt check and clippy locally"
	@echo "  make fmt          Format Rust code locally"
	@echo "  make check        Run cargo check locally"
	@echo "  make docker-test  Run tests in Docker Compose"
	@echo "  make docker-lint  Run fmt check and clippy in Docker Compose"
	@echo "  make docker-build Build release binary in Docker Compose"
	@echo "  make clean        Remove local target directory"

install:
	$(CARGO) install --path crates/$(PACKAGE) --locked --force
	@echo "$(BIN) installed. Try: $(BIN) --help"

build:
	$(CARGO) build --release -p $(PACKAGE)

test:
	$(CARGO) test --workspace

lint:
	$(CARGO) fmt --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings

fmt:
	$(CARGO) fmt

check:
	$(CARGO) check --workspace

docker-test:
	docker compose run --rm test

docker-lint:
	docker compose run --rm lint

docker-build:
	docker compose run --rm dev cargo build --release -p $(PACKAGE)

clean:
	$(CARGO) clean
