# 文件搜索

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

云吞（Wonton）插件：让 Bot 经 MCP 总线在用户授权的共享文件夹里按名字和内容找文件、列目录、读文字文件。
共享文件夹只读挂进容器，服务也以只读根文件系统运行。

找到的每个文件按[公开插件规范“工具与 MCP 总线”](https://github.com/wontonapp/wonton-docs/blob/main/plugin-spec.md#工具与-mcp-总线)的约定返回成 `resource_link`：`uri` 是 `wonton-share:<共享文件夹>/<相对路径>`，并带
`name`、`mimeType`、`size`。核心的 MCP 总线把引用换成编号，Agent 调 `wonton__share_file` 把文件交给用户。

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件包：compose 加上 `x-wonton` 清单 |
| `icon.svg` | 商店和主屏的图标，本仓库原创，许可见 [NOTICE](NOTICE) |
| `images/mcp/Dockerfile` | 插件的镜像：Rust 写的只读 MCP 服务，scratch 最终镜像 |
| `images/mcp/src/` | 服务：MCP 工具和共享文件夹里的路径访问 |
| `images/mcp/tests/` | 工具的行为测试 |

## 工具

| 工具 | 作用 |
| --- | --- |
| `search_files` | 按文件名（子串或 glob）找，可以限定共享文件夹、子目录、修改时间、类型；分页，每页最多 100 个 |
| `search_content` | 按内容找文字文件，超过大小上限和二进制文件跳过；分页 |
| `list_directory` | 列目录：文件是引用，子目录和软链接是文字 |
| `read_text_file` | 读文字文件，默认最多 64 KiB，超过就按 `offset` 分段 |

四个工具都标了 `readOnlyHint: true`，Agent 调用时不用用户确认。

`modified_after` 接受 RFC 3339 或 `YYYY-MM-DD`。只有日期时使用核心注入的 `TZ`（IANA 或 POSIX）；
scratch 镜像没有宿主的 `:/路径` 时区文件，日期过滤返回 `timezone_unavailable`，不影响其他工具和带时区的时间戳，
也不静默改用 UTC。字节偏移只有确认落在合法 UTF-8 字符中间时才向后对齐，非法前导字节报 `not_utf8`。

## 安全

- 只申请 `documents`、`photos`、`music`、`videos`、`downloads`、`transfers` 六个共享文件夹的 `:read`，都以 `:ro` 挂载。
- 路径不能是绝对路径，不能有空段、`.`、`..`。逐段 `openat` 加 `O_NOFOLLOW` 打开，路径上有软链接就拒绝；
  搜索和列目录不进入软链接目录，也只返回普通文件。
- 错误按路径非法、参数非法、时区不可用、不存在、没有权限、二进制和非 UTF-8 分开报在工具结果的 `isError`、`structuredContent.error` 里；内容搜索的跳过原因各自计数。
- 不返回宿主路径：引用第一段是共享文件夹的名字，不是容器里的挂载点。
- 服务不写任何地方，根文件系统只读，固定以非 root 用户运行。

## 为什么自己写

[rust-mcp-filesystem](https://github.com/rust-mcp-stack/rust-mcp-filesystem) v0.4.5 的查找结果是文字，媒体文件是
内联 base64，都不能交给核心的文件服务（把文件内容整个带回来，代价和风险都大）。官方的 MCP 文件系统服务不能关掉写工具，也不能
按内容搜。两者都只说 stdio。改用官方的 Rust SDK
[rmcp](https://github.com/modelcontextprotocol/rust-sdk) 写一个只读服务：它和核心用的是同一个 SDK，编译成一个静态
二进制，最终镜像是 scratch，没有 Node 运行时；只做找文件、读文字，代码短。

## 依赖与许可证

下表列直接依赖及 SDK 使用的 schema 库；传递依赖也须遵守各自的许可证，没有必须按 GPL/AGPL 授权的依赖。

| 依赖 | 许可证 |
| --- | --- |
| `rmcp` | Apache-2.0 |
| `axum`、`mime_guess`、`schemars`、`tokio`、`tokio-util` | MIT |
| `globset` | Unlicense OR MIT |
| `jiff` | Unlicense OR MIT |
| `percent-encoding`、`serde`、`serde_json` | MIT OR Apache-2.0 |
| `rustix` | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| `filetime`、`tempfile`（仅测试） | MIT OR Apache-2.0 |

服务只监听 Streamable HTTP 的 `/mcp`。`/healthz` 只读地打开每个共享文件夹并读一次目录，全部成功才回 200；
镜像里没有 curl，健康检查用二进制自己的 `healthcheck` 子命令。

## 构建与测试

镜像由合仓的 [发布工作流](../.github/workflows/publish.yml) 调用仓库根 `scripts/publish.sh`，以 `files/images/mcp` 为固定上下文构建 amd64 和 arm64，推到 `ghcr.io/wontonapp/wonton-plugins/files/mcp`。发布步骤见[合仓 README](../README.md#发布)。

发布前的清单保留上一版镜像摘要；发布工作流以插件版本构建镜像，再写回该次构建的摘要。安装和商店收录应使用工作流完成后的 tag。

以下命令均从合仓根目录运行，以 `WONTON_PLUGIN=files` 选择插件。本机试改动时构建一个本地镜像：

```bash
WONTON_PLUGIN=files docker build --network host \
  --build-arg https_proxy="$https_proxy" \
  -f files/images/mcp/Dockerfile -t wonton-files:dev files/images/mcp
```

`--network host` 让构建里的 cargo 能用宿主代理访问 crates.io；没有代理时去掉两个代理参数即可。基础镜像按摘要固定，amd64 和 arm64 都构建并冒烟过（arm64 用 `docker buildx --platform linux/arm64`）。

```bash
WONTON_PLUGIN=files cargo test --manifest-path files/images/mcp/Cargo.toml
```

测试在临时目录树里跑工具，覆盖软链接、`..`、非 UTF-8 文件名、超过 64 层的目录、大文件、二进制文件、中文文件名、分页、工作量上限、权限和修改时间。

开发环境用 `scripts/dev.sh`：

```bash
WONTON_PLUGIN=files WONTON_CORE_IMAGE=wonton-core:plugin-dev WONTON_PULL=never scripts/dev.sh up
WONTON_PLUGIN=files scripts/dev.sh install
WONTON_PLUGIN=files scripts/dev.sh status
```

合并前跑 `WONTON_PLUGIN=files scripts/dev.sh smoke`（探测在 `files/smoke-probes.sh`）：镜像没发布，本机没有清单里那个摘要时先按上面的方式构建。

云吞的开发索引按提交和每个文件的 sha256 收录插件，Bot 经 MCP 总线能找到、能装它。检查插件包用 `WONTON_PLUGIN=files scripts/check.sh --official`；版本发布和商店更新遵循[合仓通用流程](../README.md#发布)。
