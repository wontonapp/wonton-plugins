# ZCode 的 CLI 取自官方桌面 .deb：只留 CLI 本体（glm/）和它要读的内置 provider 表（config/），Electron 运行时不要。
# 官方没有 Linux CLI 发行包，deb 也不是底座 [install] 能认的 binary/tgz，所以在构建时下载、核对 sha256 后解开。
# 许可证 Apache-2.0，允许随镜像分发。ACP 桥 zcode-acp-server 由底座 [install] 在首次启动时装。
ARG BASE_IMAGE=ghcr.io/wontonapp/wonton-agent-base:0.1.0@sha256:5cba94459910395888a969190420492bd58f88a2928a2f8f9a7deb8b849519e9
FROM ${BASE_IMAGE}

ARG TARGETARCH
ARG ZCODE_VERSION=3.17.2
ARG ZCODE_X64_SHA256=86ec7b2a795c46ae56e7313504300a337c33c68267c28f96a03726a4f5aff575
ARG ZCODE_ARM64_SHA256=1f9c3d94439ab70a047e24be98297e4d31c560f5660e83c14a5997afde51c11b

USER root
RUN case "$TARGETARCH" in \
      amd64) deb_arch=x64; deb_sha="$ZCODE_X64_SHA256" ;; \
      arm64) deb_arch=arm64; deb_sha="$ZCODE_ARM64_SHA256" ;; \
      *) echo "不支持的架构 $TARGETARCH" >&2; exit 1 ;; \
    esac \
    && curl --fail --silent --show-error --location \
         "https://cdn-zcode.z.ai/zcode/electron/releases/${ZCODE_VERSION}/linux-${deb_arch}/ZCode-${ZCODE_VERSION}-linux-${deb_arch}.deb" \
         --output /tmp/zcode.deb \
    && echo "$deb_sha  /tmp/zcode.deb" | sha256sum --check --strict \
    && mkdir -p /tmp/zcode /opt/zcode \
    && dpkg-deb -x /tmp/zcode.deb /tmp/zcode \
    && mv /tmp/zcode/opt/ZCode/resources/glm /tmp/zcode/opt/ZCode/resources/config /opt/zcode/ \
    && mkdir -p /opt/zcode/glm/provider \
    && cp /opt/zcode/config/provider/zcode-builtin.json /opt/zcode/glm/provider/zcode-builtin.json \
    && chmod -R a+rX /opt/zcode \
    && rm -rf /tmp/zcode /tmp/zcode.deb

# 用户和 ENTRYPOINT 继承底座；这里只放配置、钩子和包装脚本。
COPY agent.toml /etc/wonton-agent/
COPY --chmod=0755 gateway /usr/local/lib/wonton-agent/gateway
COPY --chmod=0755 model-source login-status /usr/local/lib/wonton-agent/
COPY --chmod=0755 zcode-acp zcode-cli /usr/local/bin/
COPY LICENSE.zcode /usr/share/doc/wonton-plugin-zcode/
COPY prompt.md /usr/local/share/wonton-agent/prompt.md
COPY wonton-help/ /usr/local/share/wonton-agent/skills/wonton-help/

ENV ZCODE_BIN=/opt/zcode/glm/zcode.cjs \
    ZCODE_ACP_MODE=guarded

USER 65532:65532
