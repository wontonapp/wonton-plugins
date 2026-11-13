# 云吞插件：Navidrome

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

把共享文件夹 `music` 里的音乐做成用户的流媒体音乐库，登录交给云吞的网关（`sso: proxy`，`Remote-User` 请求头代登录）。
第三方 Subsonic 客户端连局域网端口：详情页的开关默认关，打开后显示地址和账号。

## 插件行为

- 装上就建好用户的账号：`provision` 服务等主服务健康后，用 `ND_DEVAUTOCREATEADMINPASSWORD` 建的内置 admin 登录，
  建出用户名等于注入用户名的管理员，再用 Subsonic 接口验一遍，最后删掉 admin；`/data/.owner-ready` 在就跳过。
  这是 Navidrome 0.64.2 的实测行为。账号在第一次打开网页之前就有，Subsonic 接口不依赖网页先打开。
- 音乐目录只读（`storage.share.music:read`）；持久数据和缓存在 `/data`，根文件系统只读、以 `PUID:PGID` 运行。
- 不申请 `network.egress`，外部服务（Deezer、Last.fm、ListenBrainz、insights）显式关掉；分享链接指向插件子域名、
  收件人没有云吞会话，也关掉。
- 代登录只信任核心的地址（`ND_REVERSEPROXYWHITELIST`，Navidrome 的白名单只认 CIDR，要带 `/32`）。
- 镜像用上游官方镜像 `deluan/navidrome`，按清单列表（index）的摘要固定。

## 先跑起来

```bash
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞并打印它的地址，用 `scripts/dev.sh token` 输出的用户令牌登录；`install` 列出插件要的权限，确认后
装上并打印插件网页的地址，打开就是用户自己的音乐库，不用再登录一次。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：compose 加上 `x-wonton` 清单 |
| `provision.sh` | 一次性 `provision` 服务的脚本：建用户账号、删内置 admin |
| `icon.png` | 插件图标 |
| `screenshots/` | 商店详情页里的截图 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `scripts/publish.sh` | 发布一个版本：构建自己的镜像、按摘要写回、打 tag、推送 |
| `scripts/smoke-probes.sh` | `scripts/dev.sh smoke` 的网页、Subsonic 身份探测 |
| `AGENTS.md` | 给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE` | 插件许可证、上游 GPL-3.0 许可证和商标声明 |

## 图标来源与授权

`icon.png` 使用 Navidrome 官方 1024×1024 PNG 标志，未用图片模型生成；来源、版权、处理方式、上游 GPL-3.0 全文和商标说明见 [NOTICE](NOTICE)。
Navidrome 名称和标志是 Navidrome 项目的商标，标志的使用另按上游 Name and Logo Guidelines 的 Packages 条款；本插件是独立维护的云吞插件包，不代表上游官方出品、合作或背书。
