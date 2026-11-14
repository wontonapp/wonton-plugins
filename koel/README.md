# 云吞插件：Koel

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

把共享文件夹 `music` 里的音乐变成用户自己的流媒体音乐库：网页里按歌手、专辑浏览，建歌单，看收听统计，手机浏览器也好用。
数据库用插件自带的 PostgreSQL；音乐目录只读，歌曲放进去后在 Koel 设置里点扫描。

## 插件行为

- 登录用 Koel 自带的邮箱账号（`sso: none`），登录邮箱是主人的内部邮箱（启动脚本把上游写死的默认管理员
  `admin@koel.dev` 改成它，已是内部邮箱时不做任何事）。Koel 免费版的单点登录（Google / OIDC）和反向代理头认证
  都是付费 Koel Plus 的功能，这里用不了；云吞网关挡在前面，未登录的请求到不了 Koel。
- 应用发的信（邀请家人、找回密码）经核心的 SMTP 转进收件人的 Wonton 私聊：`notify` 权限给的
  `WONTON_SMTP_HOST`、`WONTON_SMTP_PORT`、`WONTON_MAIL_FROM` 由 `wonton-start.sh` 写进 `.env` 的 `MAIL_*`，
  不认证、不加密；收件人只能是家里的人，外部地址在 SMTP 阶段被拒收，Koel 会报发送失败。
- 上游镜像的初始化要求配置写在 `/var/www/html/.env`（9.15.0 的 `koel-init` 不认环境变量）：
  `wonton-start.sh` 每次启动用注入的变量生成 `.env`（幂等），然后复刻上游 entrypoint 的步骤（镜像按摘要固定）：
  `php artisan koel:init --no-scheduler` 跑迁移、种默认管理员、写应用设置——不走镜像的 `koel-init` 是因为
  它把装不上 crontab 的主机当致命错误（`schedule:run` 的 cron 对 www-data 不可用时整个安装崩掉），
  而本插件要定时任务的功能（播客、电台同步）本来就关着；接着用应用自带的 `php artisan tinker` 把写死的
  默认管理员（`admin@koel.dev`，无配置项可改）的邮箱改成主人的内部邮箱，再让 Apache 开门——改名失败
  Apache 就不启动。装好后请在 Koel 设置里改密码。
- PHP 以镜像内置的 `www-data`（uid 33）运行，核心预建的挂载目录属主是 `PUID:PGID`：`wonton-start.sh`
  每次启动把挂载目录本身的属主改回 `www-data`（幂等，只改目录）。
- 音乐目录只读（`storage.share.music:read`）：Koel 的网页上传要写媒体目录，用不了；歌曲放进共享文件夹后点扫描。
  封面图、搜索索引、转码缓存分别在 `WONTON_APP_DATA` 的 `images`、`search-indexes`、`artifacts` 里。
- 不申请 `network.egress`，需要外网的集成显式关掉：`USE_MUSICBRAINZ=false`、`PODCASTS_ENABLED=false`、
  `RADIO_ENABLED=false`；Last.fm、Spotify、YouTube 没配密钥本来就是关的。
- APP_KEY 用插件密钥生成（32 字节 base64，即 Laravel 格式），跨升级保持同一把；数据库密码同理。
- 镜像用上游官方镜像 `phanan/koel`（amd64/arm64），按 `docker buildx imagetools inspect` 输出的清单列表摘要固定；
  PostgreSQL 是上游文档推荐的两个数据库之一（koel/docker README：镜像不含数据库，需要单独的 MariaDB/MySQL 或 PostgreSQL 容器）。

## 先跑起来

```bash
# 在仓库根目录执行
export WONTON_PLUGIN=koel
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞并打印它的地址，用 `scripts/dev.sh token` 输出的用户令牌登录；`install` 列出插件要的权限，
确认后装上并打印插件网页的地址。打开后用主人的内部邮箱 / `KoelIsCool` 登录（首次），随后改密码、点扫描。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：compose 加上 `x-wonton` 清单 |
| `icon.svg` | 插件图标（Koel 官方标志，白底） |
| `screenshots/` | 商店详情页里的截图 |
| `../scripts/check.sh` | 仓库根目录共用的插件包检查 |
| `../scripts/dev.sh` | 仓库根目录共用的开发环境，按 `WONTON_PLUGIN` 选择插件 |
| `../scripts/publish.sh` | 仓库根目录共用的发布脚本 |
| `smoke-probes.sh` | 网页、登录和数据探针 |
| `../AGENTS.md` | 仓库共用的规则 |
| `LICENSE`、`NOTICE` | 插件许可证、上游 MIT 许可证和商标声明 |

## 图标来源与授权

`icon.svg` 使用 Koel 官方 logo.svg（黑白矢量鸟形标志）等比放在白色底上，未用图片模型生成；来源、版权、处理方式、上游 MIT 全文和商标说明见 [NOTICE](NOTICE)。
Koel 名称和标志是 Koel 项目及其权利人的商标；本插件是独立维护的云吞插件包，不代表上游官方出品、合作或背书。
