# OpenAPI→MCP 适配器：把本插件（和 mtranserver 插件）手写的 OpenAPI 规范包成 MCP 服务，
# 上游 mcp-openapi-proxy 0.4.0（MIT，https://github.com/matthewhand/mcp-openapi-proxy），
# 官方没有发镜像，这里只 pip 安装上游包，不改源码。kiwix 和 mtranserver 共用这一个镜像。
#
# 构建要访问 pypi.org。Docker 在另一台机器上时把代理传进来：
# docker build --network host --build-arg https_proxy=... -t wonton-mcp-openapi-proxy:dev .
FROM python:3.12-slim@sha256:5e2bdb1b0d542d5039cffb8181c65930093949194a65c365dfb1b0fe13a4fa5b

# 版本跟着上游走（compose 里的 tag 和 .github/workflows/publish-images.yml 的 tag 同步改）。
ARG MCP_OPENAPI_PROXY_VERSION=0.4.0

RUN pip install --no-cache-dir "mcp-openapi-proxy==${MCP_OPENAPI_PROXY_VERSION}"

# 规范文件以只读 configs 挂进来，进程不写任何地方；健康检查用镜像自带的 python（见 docker-compose.yml）。
USER 65532:65532
EXPOSE 8000
ENTRYPOINT ["mcp-openapi-proxy"]
