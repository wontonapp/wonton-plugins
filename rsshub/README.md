# 万物订阅源（RSSHub）

把没有订阅源的网站变成订阅源：B 站UP主、微博、知乎、GitHub 的更新，像订报纸一样送进你的 RSS 阅读器插件。

- 上游项目：[RSSHub](https://docs.rsshub.app)（AGPL-3.0-only，本插件以独立容器运行它的官方镜像）
- 本插件包许可证：MIT（见 [LICENSE](LICENSE)）

## 它是干什么的

很多网站没有订阅源，RSSHub 给它们造了一个：上千条现成的"路由"，每条对应一类更新——B 站UP主的投稿、GitHub 项目的 Release、微博账号、知乎动态、播客、期刊……路由清单在 RSSHub 首页（打开本插件的网页就能看）和 [docs.rsshub.app](https://docs.rsshub.app/routes)。

RSSHub 只负责"造订阅源"，看订阅要配 RSS 阅读器（比如本家的 RSS 订阅、Miniflux 插件）。

## 怎么用

1. 装好本插件和一个 RSS 阅读器插件。
2. 在本插件的详情页里复制**订阅密钥**（ACCESS_KEY）。
3. 阅读器插件要声明依赖本插件的能力，云吞才允许它访问本插件的内网别名（方法见文末"给阅读器插件作者"）。
4. 在阅读器里添加订阅地址，格式：

   ```
   http://x-wonton.rsshub.wonton.internal/<路由>?key=<订阅密钥>
   ```

   端口和云吞网关一致（阅读器容器的环境变量 `WONTON_X_WONTON_RSSHUB_URL` 里有完整地址，含端口，不要手写 1200）。例子（GitHub 上 DIYgod 的仓库动态）：

   ```
   http://x-wonton.rsshub.wonton.internal/github/repos/DIYgod?key=<订阅密钥>
   ```

5. 要登录的网站先按下一节填好凭据。

订阅地址带 `?key=` 是因为本插件开了访问控制（ACCESS_KEY）：家里的服务不该无门槛对外开放，拿到密钥的阅读器才能订阅。

## 要登录的网站（可选，默认不填）

在插件详情页的**密钥**里填，订阅才不会报错：

| 密钥 | 填什么 | 怎么拿 |
| --- | --- | --- |
| 微博 Cookie | 浏览器登录微博后，请求头里 `Cookie:` 的整串原文 | 浏览器开发者工具 → 网络 → 任一请求 → 请求标头 |
| 知乎 Cookie | 同上，登录知乎 | 同上 |
| GitHub 访问令牌 | 可选。不填也能订 GitHub 路由，但容易限流 | [github.com/settings/tokens](https://github.com/settings/tokens) |

这些凭据存在核心的密钥库里，App 里只有主人能填、能改；填错时对应的订阅会报鉴权错误。

## 数据在哪、要不要备份

没有要备份的数据：RSSHub 不存东西，抓到的内容缓存在内存里（几分钟），插件升级、重装都不影响阅读器里已有的订阅和文章（那些存在阅读器插件里）。

## 已知限制

- **跨插件订阅要阅读器插件配合**：插件之间网络互相隔离，阅读器要在自己的清单里声明 `wants: [x-wonton.rsshub]`，云吞才把本插件的能力别名开给它的网络（未声明的插件连别名域名都解析不到，实测如此）。自带"禁止访问私网地址"防护的阅读器还要在自己的 compose 里放行（Miniflux 是 `FETCHER_ALLOW_PRIVATE_NETWORKS: "1"`），详见文末。
- B 站要登录的内容订不了：上游按账号 UID 把 Cookie 键入环境变量名（`BILIBILI_COOKIE_<UID>`），UID 只能来自用户的输入，而云吞注入的环境变量名是固定的，表达不了"按 UID 拼名字"。公开的 B 站路由（投稿、UP主动态）不受影响。
- 少数路由要无头浏览器（上游另有带 chromium 的镜像，本插件没带），会如实报错。
- 目标网站的风控随时会变，个别路由抓不到时 RSSHub 会如实返回错误，阅读器里能看到原因。

## 给阅读器插件作者

阅读器要能订到本插件，三件事都要做：

1. 清单声明依赖：`wants: [x-wonton.rsshub]`（`spec` 写 `"0.11"` 以上）。声明后云吞在你的插件网络里开通能力别名，并把完整地址注入环境变量 `WONTON_X_WONTON_RSSHUB_URL`（未连接时是空串）。
2. compose 里把变量传给应用：`environment: { RSSHUB_URL: "${WONTON_X_WONTON_RSSHUB_URL}" }`。订阅地址 = `<RSSHUB_URL>/<路由>?key=<订阅密钥>`。
3. 应用自带"禁私网地址"防护的（Miniflux 2.3.5 是 `FETCHER_ALLOW_PRIVATE_NETWORKS`，默认关）：在自己 compose 里放行，否则它拒绝抓内网别名，订阅报 "refusing to access private network host"。

已在开发环境实测：Miniflux 声明 `wants` 后订阅 `…/github/repos/DIYgod?key=…`，文章正常入库；同一容器不声明 `wants` 时别名域名解析失败。

## 图标和商标

图标来自 RSSHub 的官方标志（来源、许可和商标说明见 [NOTICE](NOTICE)）。
