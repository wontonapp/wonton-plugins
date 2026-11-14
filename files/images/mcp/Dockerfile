# 只读文件服务的镜像。服务用 Rust 的官方 MCP SDK rmcp 写，编译成一个静态二进制；最终镜像用 scratch，只有这个
# 文件，没有 shell 和运行时。健康检查由二进制自己的 healthcheck 子命令做。
#
# 构建要访问 crates.io。Docker 在另一台机器上时把 http_proxy、https_proxy 传进来：
# docker build --network host --build-arg https_proxy=... -f images/mcp/Dockerfile -t wonton-files:dev .
FROM rust:1.99-alpine@sha256:7e9e2d068a107a2adafd19930011178d14a78326053cd4c3a9f7ad3fd049557d AS build

ARG http_proxy
ARG https_proxy

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM scratch

COPY --from=build /build/target/release/wonton-files /wonton-files

# 固定一个非 root 用户；共享文件夹只读挂载，进程不需要写任何地方。
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/wonton-files"]
