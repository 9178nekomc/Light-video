# syntax=docker/dockerfile:1

# ---------- 构建 ----------
FROM rust:slim-bookworm AS builder
WORKDIR /app

# 先只拷 Cargo 清单，利用缓存层预编译依赖
COPY server/Cargo.toml server/Cargo.lock ./
RUN mkdir -p src && printf 'fn main(){}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src target/release/deps/lightvideo* target/release/lightvideo

COPY server/src ./src
COPY server/migrations ./migrations
RUN touch src/main.rs && cargo build --release --locked

# ---------- 运行 ----------
FROM debian:bookworm-slim
ARG WITH_FFMPEG=true

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && if [ "$WITH_FFMPEG" = "true" ]; then apt-get install -y --no-install-recommends ffmpeg; fi \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /app app

WORKDIR /app
COPY --from=builder /app/target/release/lightvideo /usr/local/bin/lightvideo
COPY web ./web

RUN mkdir -p /data && chown -R app:app /data /app
USER app

ENV DATA_DIR=/data \
    WEB_DIR=/app/web \
    BIND_HOST=0.0.0.0 \
    PORT=3000 \
    RUST_LOG=lightvideo=info,tower_http=warn

EXPOSE 3000
VOLUME ["/data"]

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:3000/api/health || exit 1

CMD ["lightvideo"]
