# Penpot 云吞插件

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

将 Penpot 2.19.0 原生网页设计与协作工具打包为云吞（Wonton）插件，使用上游官方 frontend、backend、exporter 镜像，以及 PostgreSQL 和 Valkey。

Penpot 的原生 OIDC 需要点击登录按钮，首次登录还需确认姓名；通用 OIDC 按邮箱而非 `sub` 匹配账号，修改 Wonton 邮箱后不能保证进入原账号。它不满足云吞“打开即进入”和稳定身份的接入要求；没有用脚本预建账号、自动提交或改库绕过。依据见 2.19.0 的[账号匹配](https://github.com/penpot/penpot/blob/2.19.0/backend/src/app/auth/oidc.clj#L673-L678)、[登录按钮](https://github.com/penpot/penpot/blob/2.19.0/frontend/src/app/main/ui/auth/login.cljs#L262-L266)和[注册确认](https://github.com/penpot/penpot/blob/2.19.0/frontend/src/app/main/ui/auth/register.cljs#L276-L337)。

界面语言跟随浏览器，不强制中文；本包不接 MCP，上游提供[官方 MCP 服务](https://help.penpot.app/mcp/)。

## 升级到 2.19.0

- 三个 Penpot 镜像须一起升级，Dependabot 将 `penpotapp/*` 放在同一组。
- 2.19 默认启用 Admin Console；本包按[官方升级指南](https://help.penpot.app/technical-guide/getting-started/docker/#upgrade-to-2.19)显式设置 `disable-admin-console`，保留不部署管理台的配置。
- 升级前备份 PostgreSQL 和素材目录。旧文件与快照数据会在启动时自动迁入 `file_data`，并清理孤立修订；小中型实例不需要手动迁移。拥有数百万快照或数百 GB TOAST 数据的大型实例，必须在升级前按[上游迁移手册](https://github.com/penpot/penpot/blob/staging/operations/2.19.0-migrate-legacy-file-data.md)分批迁移，不能直接等启动迁移完成。详见[发布说明](https://github.com/penpot/penpot/releases/tag/2.19.0)。

## 图标

图标来自 Penpot 官方源码；Penpot 名称及标志归其权利人所有，不表示合作或背书，来源、处理方式和许可见 [NOTICE](NOTICE)。

## 先跑起来

```bash
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞，打印它的地址。浏览器打开，用 `scripts/dev.sh token` 输出的用户令牌登录。`install` 先列出插件要的权限，确认后装上，打印 Penpot 网页的地址。网页经 `sso: oidc` 接入：打开后点 Penpot 登录页的登录按钮由云吞的身份提供方登录；首次登录会停在 Penpot 官方的姓名确认表单，确认后进入。Chrome、Firefox 和 curl 把 `*.localhost` 直接解析到本机，不用改 hosts。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：compose 加上 `x-wonton` 清单 |
| `icon.svg` | 清单引用的文件：Penpot 官方图标 |
| `screenshots/` | 商店详情页里的截图 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `scripts/smoke-probes.sh` | `scripts/dev.sh smoke` 对这个插件逐项的探测 |
| `scripts/publish.sh` | 发布一个版本：打 tag、推送 |
| `AGENTS.md` | 给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE` | 插件包许可证、上游许可证和商标声明 |
| `.wonton/` | 开发环境的设置、密钥和数据，不提交 |
