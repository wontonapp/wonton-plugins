# 云吞插件：健身助手（wger）

训练计划、营养日记和体重记录，装在云吞上的 [wger](https://wger.de)（AGPL-3.0，[官方 Docker 部署](https://github.com/wger-project/docker)）。插件开发流程见[开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

## 登录

网页入口接云吞网关的代登录（`sso: proxy`）：打开就是已登录状态，第一次打开时 wger 自动按云吞用户名建号（`AUTH_PROXY_CREATE_UNKNOWN_USER`，身份头来自网关，信任地址只写核心地址）。上游预建的 `admin/{{固定密码}}` 账号在首次启动后被停用（一次性 init 服务），注册和游客号也已关闭，账号只能经代登录产生。

## 数据

数据都在 Docker 命名卷里：`postgres-data`（数据库）、`media`（同步下来的动作图和视频、上传的视频）、`static`、`redis-data`、`beat`（调度状态）。卸载时选择删除数据会一并删除；保留数据时重装沿用。

## 出网

动作库、食物库从 wger.de 同步（celery 每周后台跑，量大、首次要几个小时，不挡启动），条码扫到本地没有的食物时实时从 wger.de 取。这些都申请 `network.egress`，经核心的出网代理；不联网时应用照常可用，只是没有新内容和查不到新食物。

## 和家人名单的关系

家里每个人打开插件网页，就是自己的账号（按网关认出的用户建号），训练和饮食互不干扰；没有共享训练计划的说法，家人之间看不到彼此的数据。

## 已知限制

- 手机 App（wger 官方 App）连不上本插件：它用 JWT 密钥认证，本插件没有配（Web 全功能可用）。powersync（App 离线同步）也不在部署里。
- 动作库同步默认每周一次随机时间；想手动触发，在宿主上进 wger 的 web 容器跑 `python3 manage.py sync-exercises`（容器名以 `wonton-` 开头、以 `de-wger-workout--web` 结尾，用 `docker ps` 查）。

## 图标

`icon.png` 用 wger 官方标志（放在上游用的白色底上），它是 wger 项目的商标，来源、许可和商标说明见 [NOTICE](NOTICE)。
