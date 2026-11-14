# 云吞插件：离线翻译（MTranServer）

把 [MTranServer](https://github.com/xxnuo/MTranServer)（Apache-2.0）包装成云吞插件：在家里服务器的 CPU 上跑
本地翻译模型，浏览器翻译扩展和带 DeepL / DeepLX / Google 翻译接口的软件都能连。

## 怎么用

- 打开网页就能翻译：选择语言方向、贴文本。网页不用做任何设置——网关登录后自动带上 API 令牌。
- 浏览器扩展（沉浸式翻译、简约翻译、划词翻译等）装好就能连：局域网端口在安装后就开放（地址在安装时的提示和
  云吞的局域网设置里，形如 `http://192.0.2.10:8989`），令牌在插件详情页的“API 令牌”里查看。按扩展要求填
  （不同扩展的路径见下表，令牌同一个）。

| 扩展 / 客户端 | 局域网地址 |
| --- | --- |
| 沉浸式翻译（Immersive Translate） | `http://<地址>:8989/imme?token=<令牌>` |
| 简约翻译（KISS Translator） | `http://<地址>:8989/kiss`，KEY 填令牌 |
| DeepL API 客户端 | `http://<地址>:8989/deepl`，`DeepL-Auth-Key` 或 `Bearer` |
| DeepLX 客户端 | `http://<地址>:8989/deeplx?token=<令牌>`，或 `Bearer` |
| 划词翻译 | `http://<地址>:8989/hcfy?token=<令牌>` |
| Google Translate API v2 客户端 | `http://<地址>:8989/google/language/translate/v2?key=<令牌>` |

- 翻译扩展连的是局域网端口，人不在家时用不了；网页翻译经云吞的远程访问照常能用。

## 家里的 AI 助手

装好后云吞里的 AI 助手（Bot）也能帮你翻译：翻译一段话、检测语言、列出支持的语言，都是只读查询，翻译在家里
完成。实现是 `mcp/openapi.json` 手写的接口规范加 [mcp-openapi-proxy](https://github.com/matthewhand/mcp-openapi-proxy)
0.4.0 适配器（上游没有发官方镜像，镜像由本合仓 `kiwix/images/mcp-openapi-proxy/` 构建，两个插件共用；MIT，见 [NOTICE](NOTICE)）。清单固定 `kiwix/v0.1.2` 发布的镜像摘要；本插件发版不重复构建适配器。
适配器用插件已有的 API 令牌连上游（`Authorization: Bearer`）。

开发命令在仓库根以 `WONTON_PLUGIN=mtranserver scripts/dev.sh <命令>` 执行，插件探针位于 `mtranserver/smoke-probes.sh`，发布流程见[合仓 README](../README.md#发布)。

## 数据在哪、怎么备份

- `${WONTON_APP_DATA}/config/`：模型索引 records.json、UI 保存的设置（server.json）、日志目录。
- `${WONTON_APP_DATA}/models/`：已下载的翻译模型文件。
- 换机器或迁移时这两个目录都要带走（与上游 README 的要求一致）。

## 已知限制

- 翻译扩展连的局域网端口装好后就开放，而且网页和设置接口也在同一个端口上：局域网里的设备不用登录就能读写
  设置——包括把 API 令牌改成自己的值（实测 v5.1.1：不带令牌 `POST /ui/api/settings/apply` 返回 200，清空
  令牌后翻译接口对所有人开放）。令牌本来就是给局域网设备用的，这是上游的设计；**只在可信的局域网里开这个
  端口**，不可信的网络里不要开。
- 第一次翻译某个语言方向时要联网下载模型文件（几十到一百多 MB，来自 Mozilla 的 Firefox 翻译模型源，
  需要出网权限），之后该方向离线可用；服务器断网时新方向翻不了。
- 翻译质量适合日常阅读和理解大意，重要内容请核对原文。
- 翻译速度和内存占用取决于服务器配置；英译中峰值内存约 400 MB。
- 网页里的 API 文档（`/docs`）和上游一致，不做改动。

## 图标

`icon.png` 来自上游官方标志（v5.1.1 `images/icon.png`，合成到白色底上），来源、版权和许可见
[NOTICE](NOTICE)。
