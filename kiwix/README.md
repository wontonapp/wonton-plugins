# 离线百科（Kiwix）

把维基百科、维基词典、可汗学院这类内容整个下载到家里，断网也能查。插件用 [kiwix-serve](https://github.com/kiwix/kiwix-tools) 把 .zim 内容文件变成一个可全文检索的网页，家里人打开就能用，不需要登录。

## 用法

1. 从 [library.kiwix.org](https://library.kiwix.org/) 下载想要的 .zim 文件。整部中文维基的精简版（`wikipedia_zh_all_mini`）约 4.8 GB，单个学科的迷你版只要十几 MB（如「Wikipedia 中文·化学 迷你版」约 15 MB），建议先小后大。
2. 把 .zim 文件放进共享文件夹「文档」（documents）的「离线百科」子文件夹（第一次自己新建这个文件夹）。
3. 一分钟内文件自动上架，打开插件网页就能读和搜索。放进去的新文件也一样，不用重启插件（见下）。

## 家里的 AI 助手

装好后云吞里的 AI 助手（Bot）也能帮你查这套离线百科：查词条、全文搜索、随机读一篇，都是只读查询。实现是
`mcp/openapi.json` 手写的接口规范加 [mcp-openapi-proxy](https://github.com/matthewhand/mcp-openapi-proxy)
0.4.0 适配器（`images/mcp-openapi-proxy/`，MIT，见 [NOTICE](NOTICE)），上游没有发官方镜像，镜像由本仓库
构建发布。

## 数据存在哪

- 内容 .zim 文件只在共享文件夹里，插件以只读方式挂载，不会改动它们；备份照共享文件夹的常规做法。
- 插件自己的数据目录里只有两样东西：kiwix 的库索引文件（`library.xml`）和扫描标记。它们都能随时重建——删掉也没关系，扫描服务一分钟内会重新生成。

## 新放入的 .zim 怎么生效

上游 kiwix-serve 只认启动时给定的库文件，没有扫描目录的功能。插件带一个扫描服务（用的也是上游镜像里的 kiwix-manage），每 30 秒看一眼「离线百科」文件夹，有变化就用 kiwix-manage 重建库文件；kiwix-serve 开了 `--monitorLibrary`，库文件一换就自动重载——实测运行中放入或删除 .zim，不用重启插件，一分钟内生效。

## 没有内容时

库为空时插件照常运行：首页显示「还没有内容」，写明把 .zim 放进哪个文件夹。这不算故障（服务本身是好的，等的是内容），健康检查显示健康；放了读不了的损坏文件时，扫描日志会写明跳过了哪个文件，其余文件照常上架。

## 已知限制

- 只读：插件不下载、不管理内容，下载要去 library.kiwix.org（或任何能拿到 .zim 的地方），再放进共享文件夹。经出网代理下载大文件的需求，用有下载能力的插件（如 metube）接共享文件夹是现成做法。
- 搜索效果取决于 .zim 自带的全文索引（文件名带 `_ftindex` 的才有）；扫描按文件名认 `*.zim`，文件名带换行的不支持。
- 扫描只在「离线百科」这一个子文件夹里找，放别处不生效。

## 资源占用

本机实测（3 MB 的测试 ZIM，一本书，读容器 cgroup 的 memory.current）：四个容器常驻内存合计约 145 MB（kiwix-serve 40 MB、扫描服务 15 MB、nginx 33 MB、MCP 适配器约 55 MB），清单按这个量级取 256M。内存随内容文件的打开量增长（kiwix-serve 用内存映射读文件，映射页计入容器内存、紧张时可回收），整部中文维基精简版这种大文件多开一些查询时会更高。

## 许可证与商标

- 上游 kiwix-serve：GPL-3.0，镜像 `ghcr.io/kiwix/kiwix-serve`（上游官方）。
- 插件包本身：MIT，见 [LICENSE](LICENSE)。
- 图标用 Kiwix 官方标志，来源、处理和许可见 [NOTICE](NOTICE)。Kiwix 名称与标志是 Association Kiwix 的商标，本插件与 Kiwix 项目无隶属关系。
