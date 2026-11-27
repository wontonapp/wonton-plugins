# Navidrome 的媒体源适配器：把 media_source 的路径翻成 OpenSubsonic，编译成一个静态二进制；最终镜像用 scratch，
# 只有这个文件，没有 shell 和运行时。健康检查和凭据命令都是这个二进制的子命令。
#
# 构建要访问 crates.io。Docker 在另一台机器上时把 http_proxy、https_proxy 传进来：
# docker build --network host --build-arg https_proxy=... -t navidrome-media:dev navidrome/images/navidrome-media
FROM rust:1.99-alpine@sha256:7e9e2d068a107a2adafd19930011178d14a78326053cd4c3a9f7ad3fd049557d AS build

ARG http_proxy
ARG https_proxy

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM scratch

COPY --from=build /build/target/release/wonton-navidrome-media /wonton-navidrome-media

# compose 里按 PUID:PGID 运行（凭据文件要归用户）；这里给一个非 root 的默认值。
USER 65532:65532
EXPOSE 8090
ENTRYPOINT ["/wonton-navidrome-media"]
