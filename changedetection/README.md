# 云吞插件：changedetection.io

盯网页的变化：把要监控的页面加进来，它定期去检查，变了就按你配的渠道提醒（邮件、Apprise、Webhook 等）。支持 "Playwright Chrome" 抓取方式——跑 JavaScript、看渲染后的页面、截图对比——也支持静态 HTML、RSS/JSON；差异在页面里逐条看。

装好后从云吞 App 打开直接可用：上游默认没有访问口令，网页只经云吞网关进来（想再加一道口令，在应用的设置里开）。"Playwright Chrome" 抓取由插件里的 sockpuppetbrowser 服务完成，无需额外配置。

应用发的信会出现在你的 Wonton 私聊里：装好时已把全局邮件通知配到云吞的内部邮箱，监控到变化就发信提醒；设置页的"添加邮件通知"也预填了内部服务器，填个地址就能加收件人。发往外部邮箱（家人以外、Kindle 这类）的信会被拒收，应用会报发送失败。要换渠道或改内容，在应用的"设置 → 通知"里改，插件不会改回来。

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)，清单规则见[插件规范](https://github.com/wontonapp/wonton-docs/blob/main/plugin-spec.md)。

## 图标

`icon.png` 原样取自上游 changedetection.io 仓库的网站图标（`android-chrome-256x256-maskable.png`）。changedetection.io 及其标志是其所有者的商标；本插件非官方出品，来源与许可见 [NOTICE](NOTICE)。
