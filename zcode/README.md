# ZCode

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

ZCode 是智谱的编程 Agent（`zai-org/ZCode` 官方 CLI，经社区的 `zcode-acp-server` 说 ACP）：以核心分配的 Matrix 身份和主人聊天，
经 Wonton MCP 总线调用工具。默认使用模型设置；网关未就绪时不会自动改用已有账号或密钥。要用智谱账号，
在插件页的登录网页或聊天里发 `/wonton login`，明确选择账号并完成浏览器授权；切回网关也在登录页选择。
选好的来源不会因网关的状态变化自动切换。

条款：Coding Plan 的额度和加速只对官方工具和官方端点有效。这里跑的是官方 CLI 本体，但社区 ACP 桥拉起的会话算不算官方工具，
智谱没写明。账号登录的令牌过期后不会自动刷新，要重新登录。

## 登录

在插件页（App 的插件详情 → 打开，默认 Bot 的登录网页）或聊天里发 `/wonton login`，选择账号登录或模型网关。
账号登录时把打印的链接在浏览器打开并同意，登录进程自己轮询完成。API key 和自建地址在模型设置里配置。

## 构建镜像

```bash
docker build -t ghcr.io/wontonapp/wonton-plugins/zcode/wonton-plugin-zcode:0.1.4 -f zcode/images/wonton-plugin-zcode/Dockerfile zcode/images/wonton-plugin-zcode
```

在仓库根目录执行。构建时从官方 CDN 下载 ZCode 桌面包并核对 sha256，只留下 CLI；底座使用官方 `wonton-agent-base:0.1.0`，桥使用官方 `wonton-acp-bridge` 镜像。

发布入口是仓库根的 [发布工作流](../.github/workflows/publish.yml)：推 `zcode/v<版本>` 后构建 Agent 镜像并把新摘要写回清单，不构建共享底座和桥。发布前的清单保留上一版 Agent 镜像摘要；安装和商店收录应使用工作流完成后的 tag。流程见[合仓 README](../README.md#发布)。

## 图标

`icon.png` 取自上游 `packages/desktop/build/icons/512x512.png`，只把透明圆角补成本图的背景色、转成不含透明通道的图片。
ZCode 和智谱是智谱的商标；本插件不是智谱的官方出品。来源和许可证见 [NOTICE](NOTICE)。
