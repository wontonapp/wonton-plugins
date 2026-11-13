# 家庭书架（Calibre-Web Automated）

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)，规范见[插件规范](https://github.com/wontonapp/wonton-docs/blob/main/plugin-spec.md)。

把 [Calibre-Web Automated](https://github.com/crocodilestick/Calibre-Web-Automated)（GPL-3.0）装进云吞：电子书自动入库（转格式、补元数据）、网页读书、阅读器 App 经 OPDS 连上来读。上游镜像按清单列表摘要固定（`crocodilestick/calibre-web-automated:v4.0.8`，amd64 + arm64），未修改上游、未另发镜像。

## 权限

| 权限 | 用途 |
| --- | --- |
| `storage.share.books:write` | 书库放在 `books` 共享文件夹（容器内 `/calibre-library`），应用要写 `metadata.db` 和转换后的书 |
| `network.egress` | 抓封面、简介等元数据（应用内置源：Google Books、Douban 等） |
| `network.lan` | 阅读器 App 在局域网直连 8083（OPDS，应用自带的账号密码认证）；详情页开关默认关 |
| `owner.identity` | 按用户名同步 OPDS 账号口令（账号由 OIDC 首次登录自动建） |

## 登录

`webui.sso: oidc`，打开网页走云吞的 OIDC 登录（`webui.path` 直接指向应用的 OIDC 起点，应用按 `groups` 里的 `admin` 组给管理员角色，清单不声明额外的组时没人带这个组）。Calibre-Web 的 OAuth 提供方配置（generic 行）和登录类型存在 `/config/app.db`（`oauthProvider`、`settings` 表），没有环境变量；镜像内的 s6 一次性服务（`wonton-sso`，经 `configs` 注入，排在应用服务之前）在应用启动前直接写库：generic 提供方指向核心注入的 `WONTON_OIDC_*` 端点（scope 带 `groups`）、登录类型切到 OAuth、家人自动建号的默认角色设为"下载 + 网页阅读"。按规范"OIDC"一节，每次创建容器都整体重写（保留数据重装后 `WONTON_OIDC_CLIENT_SECRET` 是新的）。

内置的 `admin` 账号是上游用公开默认口令建的，一次性服务把它换成核心生成的口令停用；OPDS 账号就是用户自己的账号（应用在首次 OIDC 登录时自动建，口令为空），一次性服务在每次创建容器时把它同步成 `client_password`，详情页开关里显示的账号密码才连得上；首次网页登录前口令还没生效。

## 截图

`screenshots/` 里是插件网页在手机宽度（390×844）下的截图。

## 图标

`icon.svg` 用 Calibre-Web Automated 官方标志（viewBox 平移到原点，图形未改），来源、许可与商标说明见 [NOTICE](NOTICE)。
