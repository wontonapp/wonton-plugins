# 自动追番（AutoBangumi）

按蜜柑计划（Mikan Project）等 RSS 源自动追新番：番剧更新后自动下载、按官方标题和季集格式改名整理，
可选硬链接进媒体库给 Jellyfin 看。

云吞插件包：[插件规范](https://github.com/wontonapp/wonton-docs/blob/main/plugin-spec.md) ·
[开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

## 用法

1. 装好插件后打开网页，用账号 `admin`、密码 `adminadmin` 登录，**马上在 设置 → 用户与访问控制
   里改密码**（上游应用在首次启动时固定建这个默认账号，没有改初始密码的配置项）。
2. 装了 [下载工具（qBittorrent）](https://github.com/geraldpeng6/wonton-plugin-qbittorrent) 插件的，
   下载器地址已经自动配好；否则在设置向导里自己填下载器。
3. 右上角 **+** → 勾选「聚合 RSS」，粘贴蜜柑计划的 RSS 链接（登录 mikanani.me 后右下角 RSS 按钮复制）。
4. 想让 Jellyfin 直接看到：在 设置 → 插件 里启用「硬链接到媒体库」，下载根目录填 `/downloads`，
   媒体库目录填 `/media` 下的子目录。

机器人（聊天里 @ 它）能查追番列表、搜番、订阅/退订、看下载进度，经 AutoBangumi 的 MCP 接口。

## 权限

| 权限 | 用途 |
| --- | --- |
| `network.egress` | RSS 刷新、TMDB/Bangumi 元数据和海报下载 |
| `storage.share.downloads:read` | 硬链接到媒体库时读取下载文件 |
| `storage.share.media:write` | 硬链接写进媒体库共享文件夹 |

依赖 `torrent_client` 能力（由 qBittorrent 插件提供）：没有提供方时装不上。

## 图标

`icon.svg` 使用 AutoBangumi 官方标志，来源、处理方式和商标说明见 [NOTICE](NOTICE)。

## 上游

- 应用：https://github.com/EstrellaXD/Auto_Bangumi （MIT），文档 https://www.autobangumi.org
- 镜像：上游官方镜像 `ghcr.io/estrellaxd/auto_bangumi`，按清单列表摘要固定（见 `docker-compose.yml`）。
- 一次性服务 `curlimages/curl`（MIT）只用来调应用接口签发 MCP 令牌。
