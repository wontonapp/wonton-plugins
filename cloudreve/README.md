# 云吞插件：Cloudreve

把 [Cloudreve](https://cloudreve.org)（自托管网盘，GPL-3.0）装进云吞：上传下载、在线预览、分享外链、离线下载。开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

- 上游官方镜像 `cloudreve/cloudreve`，按多架构清单摘要固定；默认 SQLite，数据（数据库、配置、上传的文件）都在插件数据目录。
- 登录是 Cloudreve 自己的账号（`sso: none`）：第一次打开时注册的账号就是管理员，注册入口只在云吞网关后面。免费版没有 OIDC（付费 Pro 功能），也不支持信任反代头登录，见清单注释。建好管理员后，建议在 Cloudreve 后台关闭开放注册（设置 → 用户会话 → 注册），新账号只能由管理员邀请。
- 离线下载用镜像自带的 aria2（`CR_ENABLE_ARIA2=1`，首次建库时自动写好本机 RPC 配置），BT 监听端口 6888 未对局域网开放，只做出网连接。
- 站点地址在首次建库时写成插件的网页地址（`CR_SETTING_DEFAULT_siteURL`），之后在 Cloudreve 后台改。

## 图标

`icon.png` 是 Cloudreve 官方标志的栅格化版本，来源和处理方式、商标说明见 [NOTICE](NOTICE)。
