# 信息面板（Glance）

打开一个页面就能看到天气、日历、时间和新闻，像钉在墙上的信息板。装好后从侧边栏的“面板”打开就能用，不用注册登录。

默认页面左边是天气、时钟和日历，右边是资讯（少数派、阮一峰的网络日志、36氪、Hacker News）和 GitHub 仓库动态。

注意：天气城市要用英文写（如“Beijing, China”“Shanghai, China”）——上游查天气的 open-meteo 固定按英文名检索，中文地名查不到。

## 页面上放什么由你定

- **换部件**：面板上显示什么由配置文件决定。数据目录就是 Glance 的配置目录，里面有 `glance.yml`（页面结构），改完保存，一般马上生效；没生效就到详情页停止再启动。全部部件（股票、监控、待办、Docker 状态……）和写法见[上游配置文档](https://github.com/glanceapp/glance/blob/v0.8.8/docs/configuration.md)。
- **让 AI 助手改**：在云吞聊天里说一声“把天气城市换成上海”或“加一个股票部件”，助手会帮你改配置文件，改完自动生效；动配置前会先给你确认卡片。助手只能读和写这个插件的数据目录（文件工具来自 [njbrake/filesystem-mcp-server](https://github.com/njbrake/filesystem-mcp-server)，Apache-2.0 许可，来源和声明见 [NOTICE](NOTICE)）。
- **天气城市、小时制、温度单位**：这些常用项在插件里声明成了设置，详情页有“设置”时在那里改；没有的话编辑 `glance.yml`（天气的 `location` 用英文写“City, Country”，如“Shanghai, China”）。

## 数据在哪

数据目录：`.wonton/data/apps/<插件 id>/`。里面只有配置文件，面板显示的新闻、天气都是从网上现拉的，不存数据。卸载时选“删除数据”会删掉你改过的配置。

## 已知限制

- 天气城市等设置要靠云吞的插件设置页（核心 #242）；没有设置页的云吞按上面的办法改配置文件。
- 配置是文件，云吞 App 里还没有编辑配置文件的入口：改部件用上面的 AI 助手，或自己直接改数据目录里的文件。
- RSS 源列表做不成设置（设置没有自由列表的控件）：加、减 RSS 源要改 `glance.yml` 里的 `feeds`。
- 天气、新闻、GitHub 动态都要能访问外网。
- 想看知乎热榜、微博热搜这类没有 RSS 的内容，要再装一个 RSSHub（把没有 RSS 的网站变成 RSS）。本插件不自带 RSSHub，要商店里有提供这个能力的插件才能接；接上后，RSSHub 里加源的方式是在 `glance.yml` 的 `feeds` 里填 `http://<能力名>.wonton.internal:1200/<路由>` 一类的地址。
- GitHub 部件不填令牌时，GitHub 匿名限额是每小时 60 次（按出口 IP 算）。默认页只放了一个部件，一般够用；多加这类部件或把缓存调小可能碰到限额，那时可以用自己的 GitHub 只读令牌（部件的 `token` 项）。
- 界面没有中文选项：部件标题等文字都是你自己在配置里写的（默认页已用中文），应用自带的按钮等仍是英文。

## 图标

图标来自 [glanceapp/glance](https://github.com/glanceapp/glance) 官方标志，来源、版权和许可见 [NOTICE](NOTICE)。
