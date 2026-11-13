# 云吞插件：Flood

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

云吞（Wonton）的 Flood 插件：现代界面的种子下载前端，添加磁力链接和种子、按标签整理任务、订阅 RSS 自动下载。Flood 自己不下载，它连接一个下载器：在云吞里就是 qBittorrent 插件。

这个插件使用上游官方镜像 `jesec/flood`。装好 qBittorrent 插件并在核心的能力设置里选它作下载器后，Flood 自动拿到连接，打开即用，不用再登录一次；下载器还没接上时，Flood 用自带账号，第一次打开在应用里注册（网关挡在前面，只有主人打得开注册页）。

## 连接 qBittorrent 插件

1. 安装 qBittorrent 插件（`org.qbittorrent.qbittorrent`）。
2. 在云吞的插件能力设置里，把 `torrent_client`（qBittorrent Web API v2）的提供方选成 qBittorrent 插件。
3. 打开 Flood：核心把 `torrent_client.wonton.internal` 转发到 qBittorrent，并注入 `WONTON_TORRENT_CLIENT_URL`。Flood 用上游自带的 `auth=none` 模式预置这条连接，打开就是任务列表，没有第二次登录。

不支持能力依赖的旧版核心不会注入 `WONTON_TORRENT_CLIENT_URL`，Flood 保留自带账号；此时它是一个没有下载器的界面，核心升级后自动切换。

## 图标来源与许可

`icon.svg` 使用 Flood 官方标志，只在透明的正方形画布下铺了纯白底色；来源、处理方式、许可证全文和商标说明见 [NOTICE](NOTICE)。

## 先跑起来

在合仓根目录运行：

```bash
export WONTON_PLUGIN=flood
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞，打印它的地址。浏览器打开，用 `scripts/dev.sh token` 输出的用户令牌登录。`install` 先列出插件要的权限，确认后装上，打印 Flood 网页的地址。第一次打开是 Flood 的注册页：建一个账号就能进（这是下载器没接上时的过渡形态）。

Chrome、Firefox 和 curl 把 `*.localhost` 直接解析到本机，不用改 hosts。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：Compose 加上 `x-wonton` 清单 |
| `icon.svg`、`NOTICE` | 官方图标和许可声明 |
| `screenshots/` | 商店详情页里的截图 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `flood/smoke-probes.sh` | `scripts/dev.sh smoke` 对这个插件逐项的探测 |
| `scripts/publish.sh` | 发布一个版本：打 tag、推送 |
| `../AGENTS.md` | 合仓根目录给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE` | 插件包许可证、上游许可证和商标声明 |
| `.wonton/` | 开发环境的设置、密钥和数据，不提交 |
