# 云吞插件：全屋音乐（Music Assistant）

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

一个曲库管全家的音乐：共享文件夹 `music` 里的本地音乐和各种流媒体合成一个曲库，推送到家里的
AirPlay、Chromecast、Sonos、DLNA 音箱上播放，支持多房间同步。登录用应用自带的认证
（`sso: none`）：首次打开由用户自己注册管理员。

## 插件行为

- 上游官方要求 `network_mode: host`（mDNS/SSDP 组播发现音箱，AirPlay/Chromecast/DLNA/Sonos
  推流要直接在局域网上开随机端口，桥接网络属于上游明确不支持的形态），清单申请
  `network.host`。宿主网络的服务连不到插件网络，网页经核心的中继容器（`relay-webui`）进网关。
- 网页端口 8095 因此也直接开在本机网卡上，局域网设备可绕过云吞网关直接访问；2.10 起应用自带
  用户认证挡在外面，`about` 里已如实说明，介意可在防火墙拦 8095。
- 本地音乐：`storage.share.music:read` 只读挂载到 `/music`，用户在应用里加"本地文件夹"音乐源时
  路径填 `/music`。音乐文件不会被改动。
- 出网（流媒体、元数据、封面）走宿主自己的路由，不经云吞的出口容器，所以不声明
  `network.egress`（核心对宿主网络出网的定论：宿主网络不经出口容器）。
- 容器内挂 SMB/NFS 所需的 `SYS_ADMIN`、`DAC_READ_SEARCH`、`apparmor:unconfined` 一律不申请：
  宿主侧挂载再只读传入正是官方推荐的更安全做法。
- 以 `PUID:PGID` 运行：镜像按非 root 运行设计（上游 Dockerfile 把 `/app`、`/tmp` 放开给所有用户）。
  健康检查打 webserver 的公开端点 `/info`（初始设置前也可用），检查网页真的在应答。
- 上游带一个实验性的 MCP Server provider（`/mcp/v1`，要在应用里手动添加并逐客户端生成访问
  token），本插件清单不声明 `mcp`：总线拿不到安装时可注入的凭据，等底座支持应用侧 token 注入
  再接。
- 镜像用上游官方镜像 `ghcr.io/music-assistant/server` 的正式版本标签 2.10.9，按清单列表（index）
  的摘要固定（amd64 + arm64）。

## 先跑起来

```bash
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞并打印它的地址，用 `scripts/dev.sh token` 输出的用户令牌登录；`install`
列出插件要的权限（含最高级别的宿主网络），确认后装上并打印插件网页的地址。第一次打开是应用的
初始认证设置：自己设管理员账号和密码（没有找回，忘了只能删 `auth.db` 重来），然后添加音乐源。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：compose 加上 `x-wonton` 清单 |
| `icon.svg` | 插件图标 |
| `screenshots/` | 商店详情页里的截图 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `scripts/publish.sh` | 发布一个版本：构建自己的镜像、按摘要写回、打 tag、推送 |
| `AGENTS.md` | 给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE` | 插件许可证（MIT）、上游 Apache-2.0 许可证全文和商标声明 |

## 图标来源与授权

`icon.svg` 使用 Music Assistant 官网的品牌标志，未用图片模型生成；来源、版权、处理方式、上游
Apache-2.0 全文和商标说明见 [NOTICE](NOTICE)。Music Assistant 名称和标志是 Music Assistant 项目的
商标；本插件是独立维护的云吞插件包，不代表上游官方出品、合作或背书。
