# 云吞插件：家务打卡（Donetick）

[Donetick](https://donetick.com)（AGPL-3.0）把家里要做的活变成一张全家的打卡表：家务设好频次自动轮值，
到期打卡、逾期标出、做家务攒积分，积分按家里定的奖励换。开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)，
规范见[插件规范](https://github.com/wontonapp/wonton-docs/blob/main/plugin-spec.md)。

## 登录

`webui.sso: oidc`：登录全走核心的身份提供方。清单里把上游的本地注册和密码登录全关了
（`disable_password_auth`、`is_user_creation_disabled`），打开网页落在登录页，点一下「云吞」就进去了。
上游的 OIDC 不读发现文档、要逐个填端点，正好用核心注入的 `WONTON_OIDC_AUTH_URL/TOKEN_URL/USERINFO_URL`。

- 第一次用云吞账号登录时应用自动建号：用户名取核心的 `preferred_username`，主人的角色由 OIDC 组
  `owner` 每次登录同步成圈管理员，家人默认成员。
- `single_circle_instance` 开着：全家共享同一个家庭圈（家庭任务表只有一张）。
- 上游登录页没有「自动跳转 OIDC」的选项，打开后要点一下按钮，这是上游交互。

## 数据

- SQLite 和上传的文件（头像、任务照片）都在 `${WONTON_APP_DATA}/data/`：`donetick.db` 加 `files/`，
  `--purge` 卸载能删干净。
- 上游配置用 compose `configs` 的内联内容写在 `docker-compose.yml` 里（`/config/selfhosted.yaml`），
  会话签名密钥由核心生成保管（`secrets.jwt_secret`）。

## 要出网的功能

无。OIDC 端点、数据、提醒都在家里；清单不申请 `network.egress`。

## 提醒和通知的已知限制

上游的提醒渠道是 Telegram、Pushover、Discord、手机推送（FCM/上游托管中继）和每圈一个 Webhook，
都不是云吞的通知通道；Webhook 的载荷是上游自有格式（没有 title/body 字段），直接指向核心的 notify
地址会被拒（400）。所以提醒只在应用内（任务列表的逾期标记、登录后的通知中心）和用户自己配的
渠道里，不会出现在云吞聊天里。等上游提供可自定义载荷的 Webhook 或通用 HTTP 渠道再接。

## Agent

上游有 REST API（chores/users/circles 等），没有 MCP。清单不声明 `mcp`；Agent 暂时只能等底座支持
REST/OpenAPI 接入。

## 图标

`icon.svg` 用上游官方标志（白色底原样缩放），来源、版权和商标说明见 [NOTICE](NOTICE)。
