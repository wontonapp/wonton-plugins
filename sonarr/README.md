# 云吞插件：Sonarr

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

云吞（Wonton）的 Sonarr 插件：把想看的剧加进片单，新一集一出就自动下载、改名归档进剧集库，配合 Jellyfin 直接观看。

这个插件使用上游安装文档指定的 [LinuxServer.io 镜像](https://docs.linuxserver.io/images/docker-sonarr/)（Docker Hub 名 `linuxserver/sonarr`，amd64 和 arm64），按清单列表摘要固定。网页经云吞网关访问：应用侧认证设为 External（不做应用内登录，登录页不出现在网页里），打开插件网页只需要云吞网关的登录。

## 共享文件夹

| 共享文件夹 | 容器内路径 | 用途 |
| --- | --- | --- |
| `downloads` | `/downloads` | 读取 qBittorrent 插件下载的文件（同一个共享文件夹，路径对得上） |
| `media` | `/media` | 剧集库；剧集根目录是 `media` 共享文件夹里的 `tv` 文件夹，安装时建好，已有就不动 |

Sonarr 导入时优先硬链接（下载区和剧集库共用一份数据）；两个共享文件夹是两个独立的挂载点，硬链接不通时会退回复制。

## qBittorrent 下载客户端

装了 qBittorrent 插件并在云吞里把 `torrent_client` 能力指向它时，插件会把 qBittorrent（Web API v2）登记成 Sonarr 的下载客户端，地址读自 `WONTON_TORRENT_CLIENT_URL`，免凭据，分类用默认的 `tv-sonarr`；已手动配过 qBittorrent 的不会被改动。能力还没接线时跳过这一步，其他功能照常。

## 通知邮件

装好插件时，插件在应用的 设置 → Connect 里建一个 "Wonton" 的 Email 通知连接：服务器是云吞的内部邮箱（不认证、不加密），收件人是主人的内部邮箱。下载完成（含导入）、剧集升级、健康问题三类通知默认打开，出现在收件人的 Wonton 私聊里；每次抓取发一封的信默认不发（量太大），要收在连接设置里打开"抓取"。信进不了外面的邮箱：把收件地址改成外部邮箱，云吞会拒收，应用报发送失败。已手动配过 Email 连接的不会被改动。

## API key

Sonarr 的 API key 由云吞生成、经插件的 secrets 注入（`SONARR__AUTH__APIKEY`，上游文档列出的环境变量）：一次性服务读同一个值登记下载客户端和通知连接，之后的插件（求片、字幕）和 Agent 也从这里拿，不用去设置页抄。应用不存第二份：换核心重新生成时以注入值为准。

## 图标来源与许可

`icon.svg` 使用 Sonarr 官方标志；来源、处理方式、许可证全文和商标说明见 [NOTICE](NOTICE)。

## 先跑起来

```bash
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞，打印它的地址。浏览器打开，用 `scripts/dev.sh token` 输出的用户令牌登录。`install` 先列出插件要的权限，确认后装上，打印 Sonarr 网页的地址。第一次打开直接进应用（应用侧没有登录页），先到 设置 → 系列 添加根目录前确认媒体库路径，再添加剧集。

元数据（剧集、集数）来自 Sonarr 自带的 skyhook 服务（TheTVDB 等），需要能访问外网；配置和数据库保存在插件数据目录，重装不丢。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：Compose 加上 `x-wonton` 清单 |
| `register-download-client.sh` | 一次性服务：把 torrent_client 能力登记成下载客户端 |
| `mail.sh` | 一次性服务：把应用的通知邮件接到云吞的内部邮箱 |
| `icon.svg`、`NOTICE` | 官方图标和它的许可声明 |
| `screenshots/` | 商店详情页里的截图 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `scripts/smoke-probes.sh` | 冒烟对这个插件逐项的探测 |
| `scripts/publish.sh` | 发布一个版本：打 tag、推送 |
| `AGENTS.md` | 给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE` | 插件包许可证、上游许可证和商标声明 |
| `.wonton/` | 开发环境的设置、密钥和数据，不提交 |
