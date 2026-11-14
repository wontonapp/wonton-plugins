# AI 搜索引擎（SearXNG）

一次搜索同时问好几家搜索引擎，把结果排在一起，不记录你搜了什么。家里的 AI 助手要查资料时，也用它搜网页、读网页。

像换了个更全面的搜索框：百度、必应、搜狗、360、B 站的结果排在一页，不用挨个开网站。在搜索框打字，回车看结果；上面的分类（图片、视频、社交）可以切换。

适合想要一个首页搜索、又不想被搜索记录跟踪的家庭。和 vane 这种“先搜再写答案”的 AI 搜索不同，这里给的是各家的原始结果，适合自己看，也适合给 AI 当资料来源。

## 特性

- 聚合搜索：预置国内可直连的引擎（百度、必应、搜狗、360、B 站），其他引擎（如 Google、DuckDuckGo）走平台出网代理。
- 不记录搜索：SearXNG 不建账号、不存搜索历史，偏好只存在你浏览器的 cookie 里。
- JSON 接口：`GET /search?q=…&format=json`，给其他插件当搜索能力用。其他插件按能力名 `x-wonton.search` 自动接入要等核心支持插件间依赖（wontonapp/wonton#28）；在那之前，别的插件可以直接调用这个接口，或经工具总线用搜索工具。
- 搜索工具：Bot 经云吞的工具总线调用 `searxng_web_search`（搜索）和 `web_url_read`（读网页），都是只读。

## 上游与许可

- 应用：[SearXNG](https://github.com/searxng/searxng)，AGPL-3.0-or-later，使用上游官方镜像。
- 搜索工具：[mcp-searxng](https://github.com/ihor-sokoliuk/mcp-searxng)，MIT，使用上游官方镜像。
- 图标：SearXNG 官方标志，来源、许可和商标说明见 [NOTICE](NOTICE)。

开发、检查和发布流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)，规则见[插件规范](https://github.com/wontonapp/wonton-docs/blob/main/plugin-spec.md)。
