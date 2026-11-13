# 云吞插件：Open WebUI

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

[Open WebUI](https://openwebui.com) 的云吞插件：在自己的浏览器里和模型聊天，上传文档做问答。模型经云吞的模型网关使用，
登录走核心的身份提供方，文档向量用镜像里内置的模型算，容器不出网。

## 插件行为

- 登出时，上游前端请求 OIDC end-session，并把回跳地址设为网页地址加 `/auth`。清单用
  `webui.oidc.post_logout_paths: [/auth]` 登记；`state=logout` 让登录页不立即自动登录。
  发布或安装此清单前，核心必须支持该字段；已有安装升级插件清单后生效，不需要删除数据。
- Open WebUI 0.13.0 会把默认连接设置存进数据库，数据库值优先于环境变量。保留数据重装时核心会更换网关凭据，所以
  初始化服务在容器重建时清除网关地址、网关密钥、默认模型和网页地址的数据库覆盖值，以本次注入值为准；管理面板中这四项
  的修改不会跨重建保留，其余设置不清除。用户名是固定的 OIDC sub，不按邮箱合并账号。
- 网页服务使用只读根文件系统；上游启动时需要复制的静态资源放在 `/tmp` 的 tmpfs，数据库和上传保存在 `/data`。
  初始化服务要接收核心写入的 `configs`，按插件规范不能使用只读根文件系统。两者都以 `${PUID}:${PGID}` 非 root 运行。
- Open WebUI 0.13.0 的聚合模型列表会把连接或鉴权错误丢弃，返回成功的空列表；`/ready` 也不检查模型服务。
  插件健康检查先请求 `/ready`，再只读查询应用数据库中当前生效的连接设置，直接以该地址、密钥请求模型网关的 `/models`。
  鉴权失败、连接/超时、非法响应或关闭连接均以非零退出码报错；错误日志不包含密钥和上游响应正文，不跟随重定向。
  单次探针最多 8 秒，30 秒一次，连续两次失败后 Docker 标为 unhealthy；有效的 `200 {"data":[]}` 表示网关可用但尚无模型，
  不伪造模型，也不把它当成鉴权失败。这项检查不发付费对话，不能证明具体模型的推理服务可用。
- 没有修改上游镜像或 API：聚合接口吞错仍是上游限制。核心插件列表按安装/恢复时的结果显示，不持续同步 Docker 健康变化
  （见插件规范“健康检查”）；运行中故障的即时原因需看容器健康日志。本地开发核心镜像 `sha256:66376ac20a10…` 的实测中，
  恢复检查将插件判为 degraded 后，恢复连接配置仍需保留数据重装才能重新接好路由。上游吞错、核心缓存及恢复限制均记录在 PR，
  不能把缓存的 healthy 当作模型网关此刻可用的证明。

## 许可证与图标

Open WebUI 的许可证是改版 BSD-3 加品牌条款：滚动 30 天内不超过 50 个终端用户时可以改动名称和 logo，这里两者都原样
保留，任何部署里都显示 “Open WebUI” 的名字和 logo。镜像用上游官方的 `ghcr.io/open-webui/open-webui`，不改镜像。

`icon.png` 取自 Open WebUI v0.12.5 的官方平面标志，未用图片模型生成；来源、版权、处理方式、上游 Open WebUI License 全文和商标说明见 [NOTICE](NOTICE)。
Open WebUI 的名称和标志是 Open WebUI Inc. 的商标，仅用于识别上游应用；本插件不代表上游，也不暗示官方合作、背书或隶属关系。
插件代码按 [LICENSE](LICENSE) 的 BSD-3-Clause 授权，不是 Open WebUI License。

## 先跑起来

```bash
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞并打印它的地址，用 `scripts/dev.sh token` 输出的用户令牌登录；`install` 先列出插件要的权限，确认后
装上，打印插件网页的地址；打开就是自己的聊天页，模型列表来自云吞的模型网关。Chrome、Firefox 和 curl 把 `*.localhost`
直接解析到本机，不用改 hosts。

插件走应用自己的 OIDC 登录链：没有浏览器时先用 `scripts/dev.sh session` 建立用户会话，`scripts/dev.sh web` 默认带着
这个会话请求网页。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：compose 加上 `x-wonton` 清单 |
| `icon.png` | 清单引用的满幅 PNG 图标，来源与许可见上文 |
| `screenshots/` | 商店详情页里的截图 |
| `init/owner.py` | 一次性 `init` 服务的脚本：清除网关设置的数据库覆盖值，预建 OIDC 用户 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `scripts/smoke-probes.sh` | `scripts/dev.sh smoke` 对这个插件逐项的探测 |
| `scripts/publish.sh` | 发布一个版本：构建自己的镜像、按摘要写回、打 tag、推送 |
| `AGENTS.md` | 给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE`、`LICENSES/` | 插件许可证、上游 Open WebUI License 全文和商标声明 |
| `.wonton/` | 开发环境的设置、密钥和数据，不提交 |
