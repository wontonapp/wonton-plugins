# 云吞插件：Memos

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

Memos（[usememos.com](https://usememos.com)，MIT）是自托管的碎片笔记：速记、日记、待办和标签，全文搜索，笔记默认在
自己家里。这个插件把它装进云吞：网页经核心网关打开，登录走核心的身份提供方（`sso: oidc`），主人是 Memos 的管理员，
开放给家人后，家人第一次打开自动建各自的账号；Bot 经 Memos 自带的 MCP 服务搜索、新建、追加笔记。

## 插件行为

- 主人账号在安装时建好：一次性 `init` 服务等 Memos 健康后，用核心注入的用户名（`WONTON_OWNER_NAME`）和生成的密钥
  （`WONTON_SECRET_OWNER_PASSWORD`）注册管理员并接上核心身份提供方（`config/wonton.json`），之后打开网页就是自己的账号。
- MCP 经 nginx 侧车：核心的 MCP 总线不带凭据，侧车把用户账号的 PAT 加上再转发给 Memos（PAT 也是 `init` 建的）。
- 数据目录先由一次性 `chown` 服务统一属主再交给 Memos（以 `PUID:PGID` 非 root 运行）；每次创建容器都跑一遍，幂等。
- `network.egress` 只给 Webhook 推送、链接预览和 AI 语音转写用，笔记本身不出网也照常用。
- 通知邮件：有人评论、提及你或邀请你进 Space 时，Memos 发的通知信从云吞的内部邮箱寄出，出现在你的「Wonton」私聊里；收件人只能是家里的人，应用里也没有发信给外部地址的功能。
- 插件用上游官方镜像 `neosmemo/memos`，按摘要固定；数据在插件自己的数据目录里，不要额外的数据库和缓存。

- 核心升级时容器会按新的签发方自动重建，不用重装；账号按已有的身份记录认回原账号，数据不动。

## 家人账号

应用默认没开放给家人，家人的列表里也看不到。先邀请家人、给家人的设备配对，再在插件详情页打开「给家人用」，应用才会
出现在家人的列表里。家人首次通过云吞登录会自动建立自己的账号，不用手工注册或另设密码。`config/wonton.json` 使用 Memos
官方声明式配置 `instance/settings/GENERAL` 的两个字段：

- `generalSetting.disallowUserRegistration: false`：允许首次 SSO 登录开户。固定上游 26.11.0 的
  `server/api/auth_service_sso.go` 为新账号设置 `RoleUser`，不读取角色或组声明。
- `generalSetting.disallowPasswordAuth: true`：禁止匿名密码注册和普通用户密码登录。管理员密码登录仍可用，但开着这个
  开关时登录页不显示密码表单、会直接跳转 SSO；要用密码时在地址栏加上 `/auth/admin` 打开管理员登录页。实例仍为
  `PRIVATE`，身份提供方只有云吞。
- 这两个开关由声明式配置锁定，界面上改不动（会提示由配置管理），每次启动都会重新应用配置。

账号各自独立只针对网页：Bot 仍以主人的身份访问，搜到和改的是主人的笔记。

需要管理员权限的家人先通过云吞登录开户，再由主人在 Memos 的「设置 → 成员」里修改角色；改回普通用户也在这里操作。
上游对应 `UpdateUser` 的 `role` 字段只允许管理员修改，并保护最后一位管理员。Memos 不支持 `role_claim` 或
`admin_groups` 映射，应用内的授予、撤销不会随云吞 `plugin_admin` 自动同步；按应用原生方式分配，不预建家人账号或另做同步。

## 先跑起来

```bash
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞并打印它的地址，用 `scripts/dev.sh token` 输出的用户令牌登录；`install` 列出插件要的权限，确认后
装上并打印插件网页的地址，打开就是已登录的 Memos。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：compose 加上 `x-wonton` 清单 |
| `config/wonton.json`、`config/init.sh` | 声明式身份配置和建用户账号、MCP 令牌的一次性脚本 |
| `icon.png` | 插件图标 |
| `screenshots/` | 商店详情页里的截图 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `scripts/publish.sh` | 发布一个版本：构建自己的镜像、按摘要写回、打 tag、推送 |
| `scripts/smoke-probes.sh` | `scripts/dev.sh smoke` 的网页、OIDC 身份和 MCP 工具探测 |
| `AGENTS.md` | 给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE` | 插件许可证、上游 MIT 许可证和商标声明 |

## 图标来源与许可

`icon.png` 取自 Memos 26.11.0 的官方 PNG（web/public/android-chrome-512x512.png），未用图片模型生成；来源、版权、处理方式、上游 MIT 全文和商标说明见 [NOTICE](NOTICE)。

该标志为 Memos 项目及其权利人的商标，仅用于识别本插件所集成的应用；本插件不代表 Memos 官方，
也不表示获得 Memos 的合作、认可或背书。
