# 仓库规则

本文件对人和 AI 编程代理（Claude Code、Codex 等）同样适用。通用开发流程见[开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)，规则见[插件规范](https://github.com/wontonapp/wonton-docs/blob/main/plugin-spec.md)。

## 布局

- 一个仓库放多个官方插件，每个插件一个顶层 `<id>/` 子目录，每个子目录是一个独立插件包：插件 id、版本、
  许可证和商店条目都分开。仓库只是一起维护，核心仍把每个子目录当一个插件。
- 开发套件（`scripts/`）、CI 工作流和检查规则只有仓库根这一份，全部插件共用；某插件独有的文件只放在
  它自己的目录里。套件按 `WONTON_PLUGIN=<id>` 指到插件目录运行。
- 并发工作时只改自己的 `<id>/`；要改 `scripts/`、工作流或 `.leak-allow` 的，单独一个 PR 先合——这些改动
  会触发全部插件的检查。
- `scripts/dev.sh kit-update` 按模板仓库的套件版本更新 `scripts/`；本仓库是"一个仓库多家插件"的布局
  （仓库根没有 docker-compose.yml），套件会保留本仓库自己的 `check.yml` 和 `publish.sh` 不覆盖——检查
  工作流按子目录分作业、发布按 `<目录名>/v<版本>` 打 tag，都是仓库自己写的。

## 插件包

- 插件就是子目录里的 `docker-compose.yml` 和它引用的文件。规则以核心的检查为准：
  `WONTON_PLUGIN=<id> scripts/check.sh` 通过、`scripts/dev.sh install` 装得上，才算写对了；不靠猜规范，
  也不为了通过检查改动核心的规则。
- 核心不支持插件要的东西（一项权限、一个 compose 字段、宿主设备这类能力）时，不绕过核心的规则，也不
  悄悄去掉这项功能：在 PR 里写明缺什么、为什么要，向云吞提 issue。
- 只申请用得到的权限：要出网才申请 `network.egress`，只读的共享文件夹申请 `:read` 并以 `:ro` 挂载。
- 新插件默认用应用自带的登录（`sso: none`）：装好后第一次打开，用户在应用里自己注册管理员。插件网页
  始终在核心网关的登录后面，别另建绕过网关的入口；已经接了 `sso: oidc` 或 `sso: proxy` 的保留。
  不把用户令牌交给插件。
- 要用大模型就申请 `models`，经模型网关访问。插件里不放任何第三方 API key，也不让用户把 key 填进插件。
- 插件自己的密钥写在清单的 `secrets` 里，由核心生成或保管；不写进 compose、配置文件和镜像。
- 镜像：
  - 用上游官方镜像，按清单列表（index）的摘要固定：`名字:标签@sha256:…`。
  - 不改上游镜像，不另发修改版。确实要自己构建的，Dockerfile 放在子目录的 `images/<名字>/`，由
    `scripts/publish.sh` 构建多架构镜像、推送并按摘要固定。
  - 新用一个镜像或依赖时写明它的许可证。清单的 `license` 写应用的许可证（SPDX），插件包自己的许可证
    写在子目录的 LICENSE 里。
- 图标用上游标志时，来源、版权、处理方式、许可全文和商标说明写进子目录的 `NOTICE`，清单用
  `notices: [NOTICE]` 引用；图标是自己画的就写原创和本仓库许可证。README 的图标一节只留一句来源和
  商标说明，指向 NOTICE。
- 子目录的 README 只写这个插件的说明；通用流程和规则链接开发指南与插件规范，不在仓库里复制。

## 质量

- 代码和配置短、清楚、不冗余。每个部分只做一件事（Unix 哲学）：能用上游应用现成的配置或功能做到的，
  不写脚本，也不在中间加一层。
- 源码文件只数代码行（不算空行和注释）：超过 800 行警告、超过 2000 行失败；函数 ≤ 100 行；没有用到的
  代码、配置和依赖删掉。
- 注释只写读代码得不到的信息：为什么这样写（包括没选的写法）、约束、上游镜像和应用的实测行为。
  不写会变的当前状态（“现在”“还没”“目前”）。

## 运行时原则：失败即停

- 不用 mock、预览数据、静默降级或兜底行为掩盖缺失能力。缺了配置、连不上依赖时启动失败或如实报错，
  不假装正常。
- 错误要说清原因：权限缺失、依赖的服务没就绪、上游出错、输入非法，分开说。
- 状态不确定时如实显示为不确定，不伪装成功。
- 健康检查检查的是真正可用，不只是进程还活着。

## 界面

- 不加多余的副标题和说明文字，让布局说话。分组、顺序、位置和图标已经说明了的，不再加副标题、说明
  文字或标签重复一遍。插件自己的网页同样适用。
- 手机竖屏（390×844）要好看。
- 错误、权限和来源的提示不算多余，照常写清楚（见“失败即停”）。
- 商店信息照规范写：`description` 一句话说清插件做什么，`about`、`release_notes` 写清楚内容；怎么显示
  由 App 决定。

## 共用一台 Docker

几个人或几个 Agent 可能共用一台 Docker，各自的开发环境靠实例名分开（见[开发指南的“开发环境”](https://github.com/wontonapp/wonton-docs/blob/main/develop.md#开发环境)）。

- 开发环境的起停、安装和清理只用 `scripts/dev.sh`，插件目录用 `WONTON_PLUGIN` 指定；状态放在
  `.wonton/<插件名>/`，几个插件可以同时各起各的。
- 不执行 `docker system prune`、`docker container prune`、`docker volume prune`、`docker network prune`、
  `docker image prune -a` 这类针对整台主机的清理。
- 只动名字里带着自己实例名（`.wonton/` 下对应目录的 `WONTON_INSTANCE`）的容器、网络和卷；别人的不停、
  不删、不进去改。
- 自己构建或拉来试用的镜像，用完按名字删掉；不删别人的镜像。不再开发时执行
  `WONTON_PLUGIN=<id> scripts/dev.sh purge`。

## 验证

插件仓库没有单元测试，改了插件包就在开发环境里走一遍（`WONTON_PLUGIN=<id>`）：

1. `scripts/check.sh` 没有不合格，提醒都有原因。
2. `scripts/dev.sh reinstall --yes` 装得上，`scripts/dev.sh status` 里是 `running healthy`。
3. 浏览器打开插件网页：`sso: oidc` 或 `proxy` 验证自动登录；`sso: none` 验证首次注册管理员和应用自己的
   登录。把插件的主要功能用一遍。没有浏览器时用 `scripts/dev.sh web <路径> [<curl 参数>…]` 通过核心
   网关请求网页和接口，应用自己的登录仍需单独验证。
4. 写进数据的操作做一次，`scripts/dev.sh reinstall --yes` 以后数据还在；`scripts/dev.sh uninstall --purge`
   以后数据删干净了（没有 `.wonton/data/apps/<slug>/`，`docker volume ls` 里没有它的卷）。

PR 描述写清改了什么、为什么、上面哪几步做了、结果是什么；每个结论附上执行的命令和结果，没做的写明没做。

## 文档与注释

- 文档和注释只记录读代码得不到的信息：为什么这样写（包括没选的写法）、约束、上游镜像和应用的实测行为。
- 不写会变的当前状态（“现在”“还没”“目前”）；验证结果写明验证时的版本和条件。
- compose 里的注释写给审核的人看：为什么要这项权限、为什么挂这个目录、为什么用这个用户运行。

## Git 与发布

- 只有 `main` 一个长期分支。一件事一个分支、一个 PR。
- 提交信息用 Conventional Commits：`type(scope): 摘要`，`type` 取 `feat`、`fix`、`docs`、`refactor`、
  `chore`、`build`、`ci`；摘要说清行为变化。
- 不提交 `.wonton/`、密钥和调试产物（截图、日志）。截图只有商店用的放在子目录的 `screenshots/`。
- 发布：改子目录清单的 `version`、写好 `release_notes`、提交进 `main`，再推 `<id>/v<版本>` 的 tag，
  发布工作流会检查插件包并把 tag 按摘要固定。发布过的 tag 不移动、不删除；发错了就发一个新版本。
  仅用上游镜像的插件没有摘要提交，tag 直接打在清单版本的提交上。
- 提交作者用 GitHub 的 noreply 邮箱：`git config user.email "<数字 ID>+<用户名>@users.noreply.github.com"`。

## 在公开仓库开发

本仓库是公开的，开发就在这里：从 `main` 切分支，提 PR 合回 `main`，不另设私有仓库同步。

- 提交信息、PR、issue 和截图都是公开的，里面不写真实的局域网地址、主机名、账号和密钥。写验证结果时
  只写做了什么、结果如何；地址换成 `192.0.2.10` 这样的示例。
- CI 的“公开仓库泄漏检查”扫整个仓库里的机器名、内网地址和状态词；确属误报的写进根目录的
  `.leak-allow`，每行 `路径 | 行内片段 | 原因`（路径支持通配，片段是行里原文包含的一段）。
- 官方索引收录由向 `wontonapp/wonton-store` 提 PR 完成：把条目的来源写成
  `wontonapp/wonton-plugins/<id>@<id>/v<版本>`。
