# 家庭影院（Jellyfin）

共享文件夹里的电影、剧集和音乐排成一面墙，手机、电脑上打开网页就能看。开发流程和规则见仓库根的 [AGENTS.md](../AGENTS.md) 与[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

本插件使用 Jellyfin 上游官方 `jellyfin/jellyfin:12.4` 镜像（按清单列表摘要固定），不改镜像，经云吞网关打开网页。配置、缓存和海报信息放在插件自己的数据目录；共享文件夹以只读方式挂载。

## 图标与许可

`icon.svg` 使用 Jellyfin UX 仓库的官方 `jellyfin-icon--color-on-light.svg`，仅按插件规范加白色满幅底色；来源、Creative Commons BY-SA 4.0 许可全文、Jellyfin GPL-2.0-only（含源码地址）和商标说明见 [NOTICE](NOTICE)。本插件不代表 Jellyfin 官方出品、合作或背书。

## 开发

在仓库根目录执行，`WONTON_PLUGIN=jellyfin` 指定插件目录：

```bash
WONTON_PLUGIN=jellyfin scripts/check.sh
WONTON_PLUGIN=jellyfin scripts/dev.sh up
WONTON_PLUGIN=jellyfin scripts/dev.sh install
```

`up` 在本机起一套云吞并打印地址；浏览器打开后先用 `WONTON_PLUGIN=jellyfin scripts/dev.sh token` 输出的用户令牌登录网关，再 `install`。`install` 先列出权限，确认后打印 Jellyfin 网页地址。`scripts/dev.sh web <路径>` 可以不经过浏览器、带着网关会话抓页面和接口。

## 首次启动与登录

第一次打开是 Jellyfin 上游的初始化向导：创建管理员、把媒体目录填成 `/media`。插件在第一次创建数据目录时只用同一个官方镜像复制一份 `config/system.xml`，预置中文海报信息和中国地区，已有配置不会覆盖；管理员账号和密码由向导创建，插件不写默认密码。

之后的家人各有各的 Jellyfin 账号。Jellyfin 的 Quick Connect 让家人第一次登录时不用输密码：登录页给出配对码，管理员在自己的 Jellyfin 里批准。Quick Connect 是服务器级开关（控制台 → 服务器 → 常规 →「在此服务器上启用快速连接」，默认开），`setup` 里写了这一步。

Jellyfin 12.4 没有原生 OIDC，反向代理请求头也不是 Jellyfin 核心支持的登录方式，所以清单用 `sso: none`：云吞网关挡住网页，Jellyfin 保留自己的登录。不做自研认证侧车，也不把用户凭据交给插件。

## 目录里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件：Jellyfin 服务加上 `x-wonton` 清单 |
| `config/system.xml` | 首次创建配置目录时复制的中文海报信息默认配置 |
| `icon.svg` | Jellyfin 官方图标 |
| `NOTICE` | 图标、上游软件许可和商标声明 |
| `smoke-probes.sh` | `WONTON_PLUGIN=jellyfin scripts/dev.sh smoke` 对这个插件逐项的探测 |
| `screenshots/` | 商店截图 |

插件包是 `docker-compose.yml` 加上它引用的文件：`icon`、`settings`、`bot.avatar`、`notices` 和顶层 `configs` 的 `file`。README、截图不在插件包里。引用的路径相对插件目录，不能是绝对路径、不能含 `..`、不能用变量，也不能经过软链接；单个文件不超过 2 MiB，整个包不超过 16 MiB。安装时插件包是 tar 归档，里层最多 1024 项。
