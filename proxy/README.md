# 代理 · 云吞插件

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

把上游 Mihomo 和 zashboard 接到云吞：订阅入口只负责保存链接和触发更新，节点、流量、连接由原生面板管理。
Mihomo 和面板分别运行上游官方镜像，不修改上游源码。

zashboard v3.30.0 没有订阅 URL 编辑器或外链导航，因此入口只增加一条订阅链接，原面板在同源框架中完整运行。
最小的 Python 标准库服务将订阅 URL 以 0600 存入数据目录，通过本地 provider 地址隔离上游请求错误中的 URL；
每小时更新由 Mihomo 的 `proxy-providers.interval` 执行。未配置时规则拒绝出网，不把直连伪装成代理可用。
免鉴权范围从插件容器的实际 IPv4 路由读取，不把整个私有地址空间（可能包括主人局域网）当成 Docker 网络。

局域网访问默认关闭。打开后，来自真实局域网源地址的连接需要账号密码；宿主本身或其他本机容器经发布端口回环时，
Docker 可能把来源改写成插件网桥内地址，命中免鉴权范围。这个密码不能隔离本机其他容器；只在接受这条边界时打开局域网访问。

## 图标

使用 [MetaCubeX 官方标志](https://github.com/MetaCubeX/mihomo/blob/v1.19.33/Meta.png)，名称和标志归原权利人所有，不表示合作或背书；来源、处理及许可见 [NOTICE](NOTICE)。

## 先跑起来

```bash
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞并打印它的地址，用 `scripts/dev.sh token` 输出的用户令牌登录；`install` 列出插件要的权限，确认后
装上并打印插件网页的地址，打开就是面板和订阅入口。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：compose 加上 `x-wonton` 清单 |
| `icon.svg`、`web/` | 清单和 compose 引用的文件：图标、交给容器的配置和网页 |
| `screenshots/` | 商店详情页里的截图 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `scripts/smoke-probes.sh` | `scripts/dev.sh smoke` 对这个插件逐项的探测 |
| `scripts/publish.sh` | 发布一个版本：打 tag、推送 |
| `.github/workflows/publish.yml` | 在 GitHub Actions 里执行 `publish.sh`，手动触发 |
| `AGENTS.md` | 给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE` | 插件许可证、上游标志的许可和商标声明 |
