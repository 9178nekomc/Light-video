# Light-Video 常用命令
SHELL := /bin/bash
SERVER := server
WEB := web

.PHONY: help dev build run test smoke fmt lint clean docker

help:
	@echo "make dev      # 开发模式启动（debug 构建 + 自动读取 server/.env）"
	@echo "make build    # release 构建"
	@echo "make run      # 运行 release 二进制"
	@echo "make test     # 后端集成测试（真实 SQLite + Router）"
	@echo "make smoke    # 端到端 UI 冒烟（无头 Chromium + 截图）"
	@echo "make fmt      # 格式化 Rust 代码"
	@echo "make clean    # 清理构建产物与临时数据"

dev:
	cd $(SERVER) && cargo run

build:
	cd $(SERVER) && cargo build --release

run:
	cd $(SERVER) && ./target/release/lightvideo

test:
	cd $(SERVER) && cargo test -- --nocapture

smoke:
	bash scripts/ui-smoke.sh 3011 ./data-demo

fmt:
	cd $(SERVER) && cargo fmt

lint:
	cd $(SERVER) && cargo clippy --all-targets -- -D warnings

clean:
	cd $(SERVER) && cargo clean && rm -rf data data-demo ../.smoke

docker:
	docker compose build && docker compose up -d
