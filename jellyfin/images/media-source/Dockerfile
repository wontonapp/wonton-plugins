# 媒体源适配器的镜像：Rust 编成静态二进制，最终镜像 scratch，没有 shell 和运行时。健康检查和凭据命令
# 都是这个二进制的子命令（清单里的 provides.media_source.credentials 也指向它）。
#
# 构建要访问 crates.io。Docker 在另一台机器上时把 http_proxy、https_proxy 传进来：
# docker build --network host --build-arg https_proxy=... -t wonton-jellyfin-media:dev .
FROM rust:1.99-alpine@sha256:7e9e2d068a107a2adafd19930011178d14a78326053cd4c3a9f7ad3fd049557d AS build

ARG http_proxy
ARG https_proxy

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM scratch

COPY --from=build /build/target/release/wonton-jellyfin-media /wonton-jellyfin-media

# 以插件的 PUID:PGID 运行（清单里写），只写挂进来的 /data。
EXPOSE 8090
ENTRYPOINT ["/wonton-jellyfin-media"]
