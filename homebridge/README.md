# 云吞插件：家庭桥接器（Homebridge）

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

把不支持苹果 HomeKit 的设备（米家、涂鸦等）接进苹果"家庭"App 和 Siri：插件跑上游官方
`homebridge/homebridge` 镜像，设备接入插件在 config-ui-x 网页里搜索安装。

## 插件行为

- 首次打开网页是上游的首次设置向导，用户自己创建管理员账号（Homebridge UI 5.x 没有默认账号，
  `auth.json` 不存在时向导强制先建管理员），登录走 `sso: none`，不预建账号。
- 首次启动前由一次性服务种一份 `config.json`，把 config-ui-x 的监听地址绑到 `127.0.0.1`
  （config.json 的 UI platform 块支持 `host` 覆盖）：网页只经云吞网关访问，局域网绕不过登录；
  已有配置不覆盖。HomeKit 桥的 username/port/PIN 由 hb-service 首次生成，不预置。
- `network_mode: host` 是上游对 HomeKit 的硬要求（mDNS 发现 + 桥接端口），对应 `network.host`
  权限；config-ui-x 网页因此由核心经回环中继接入网关，并按宿主网络 webui 的要求写了健康检查
  （`GET /api/auth/settings`）。宿主网络服务出网走宿主自己的路由，不经平台出网代理，也不申请
  `network.egress`：镜像首次启动要从 npm 装 Homebridge，UI 里装插件也要 npm 出网。
- 插件下载走 npm 官方源。上游 config-ui-x 网页没有 npm 源设置，镜像每次启动还会用自带默认覆盖
  数据目录里的 `.npmrc`，所以没有提供换源选项。
- 数据（config.json、auth.json、插件 node_modules）在插件数据目录，重装、升级保留。

## 先跑起来

```bash
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞并打印它的地址，用 `scripts/dev.sh token` 输出的用户令牌登录；`install`
列出插件要的权限，确认后装上并打印插件网页地址。打开网页 → 向导里建管理员 → "插件"页装设备
插件 → 在"家庭"App 里用网页上显示的配对码添加桥。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：compose 加上 `x-wonton` 清单 |
| `config/config.json` | 首次启动种子的 UI 配置（回环绑定） |
| `icon.svg` | 插件图标 |
| `screenshots/` | 商店详情页里的截图 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `scripts/publish.sh` | 发布一个版本 |
| `scripts/smoke-probes.sh` | 冒烟探测：网页、向导建号、配置写读 |
| `AGENTS.md` | 给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE` | 插件包许可证（MIT）、原创图标与上游许可声明 |

## 图标来源与授权

`icon.svg` 为本仓库原创线条图标，未用图片模型生成，按本仓库 MIT 许可证分发；见 [NOTICE](NOTICE)。
上游品牌指南不允许未经许可把 Homebridge 标志用作第三方应用图标，故未使用；Homebridge 名称是
Homebridge 项目的商标，本插件与上游没有隶属关系。
