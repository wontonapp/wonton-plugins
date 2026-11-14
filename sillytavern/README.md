# AI 角色扮演（SillyTavern）

[English](#english)

和 AI 扮演的角色聊天、玩文字冒险：自带角色可以直接聊，也能自己捏角色、写世界观。模型默认走云吞的模型网关，
不用填 key；多用户由应用自己管，家里每个人有自己的聊天记录。

![icon](icon.png)

## 这个插件做了什么

- 用 SillyTavern 的上游官方镜像（`ghcr.io/sillytavern/sillytavern:1.19.0`，按摘要固定），不改上游。
- 一次性服务 `seed-model` 把模型网关的连接（地址、模型名、凭据）写进默认用户（`default-user`）的
  `settings.json` 和 `secrets.json`，只在用户还没配置过模型接口时写入；重装插件会换网关凭据，
  脚本按自己写的值（`data/.wonton-model.json` 记录）刷新，用户改过的连接一律不动。细节见 `seed-model.js`。
- 多用户：没有用户时应用自动建 `default-user`（管理员、无密码，唯一用户且无密码时打开即自动登录）。
  白名单模式关掉（请求都来自核心网关，插件拿不到要信任的网关地址），加上上游给反向代理部署留的
  `securityOverride` 开关，首次启动的安全检查才放行无密码的管理员账号。
- 数据都落在插件数据目录：`config/`（config.yaml）、`data/`（用户数据）、`backups/`、`extensions/`（第三方扩展），
  升级重建容器不丢。

## 权限

- `models`：对话模型经云吞的模型网关。
- `network.egress`：角色卡、扩展、分词器和语音模型从网上下载；接别的模型服务商也要出网。

## 图标

`icon.png` 取自 SillyTavern 的官方图标，来源、许可和商标说明见 [NOTICE](NOTICE)。

## 许可证

- 插件包（清单、脚本、说明）：MIT，见 [LICENSE](LICENSE)。
- 上游应用 SillyTavern：AGPL-3.0（https://github.com/SillyTavern/SillyTavern）。

# English

Chat with characters played by AI or run text adventures, powered by the upstream official image of
SillyTavern 1.19.0. The model goes through Wonton's model gateway by default; a one-shot service
writes the connection into the default user's settings on first install and refreshes it when the plugin
is reinstalled (the gateway credential rotates), never touching connections the user configured.

Multi-user is handled by the app itself: the first account (`default-user`, admin) is created without a
password and auto-signed-in while it is the only user; the plugin runs with upstream's
`securityOverride` escape hatch for reverse-proxy deployments and whitelist mode off, because the trusted
gateway address is not available to plugins. See `docker-compose.yml` and `seed-model.js` for details.
