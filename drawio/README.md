# 云吞插件：画图（draw.io）

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

[draw.io](https://www.drawio.com)（[jgraph/drawio](https://github.com/jgraph/drawio)，Apache-2.0）是画流程图、
架构图、思维导图、平面布置图的事实标准工具。这个插件把它装进云吞：网页经核心网关打开（`sso: none`，应用没有
账号体系），装完打开就画，不用注册登录。

## 用法

- 打开插件网页就能画：左边形状库拖到画布，拉线连接，双击改文字。
- 图自动保存在**打开网页的这台设备的浏览器里**：换一台设备看不到，清除浏览器数据会丢。要留着或发给别人，
  用"文件"菜单另存为 `.drawio` 文件或导出 PNG/SVG；换设备时把文件带来，"文件 → 打开"即可。
- 界面语言默认中文；想换在编辑器里点"其他 → 语言"（选择存在浏览器里，优先于默认值）。
  上游自托管部署不跟随浏览器语言，这是上游的已知行为。

## 数据存在哪

- 服务器**不存任何图**：容器里没有挂卷，图和编辑器偏好都存在用户浏览器的 localStorage，导出的文件存在用户
  自己的设备上。备份用户的图 = 备份用户导出的 `.drawio` 文件。
- 卸载再重装插件，浏览器里的图还在（插件 id 不变，网页地址就不变）；删除数据重装也不影响浏览器里的图。

## 不出网

- 容器不申请 `network.egress`，本身连不了外网。
- 页面经 `DRAWIO_CSP_HEADER` 把浏览器的内容安全策略收紧到本站：脚本、接口（XHR/WebSocket）、内嵌框架只允许
  本页自身；云端集成（Google Drive、OneDrive、GitLab、Dropbox、GitHub、Trello）由上游入口脚本在未配 OAuth
  凭据时写死关闭（`PreConfig.js` 的 `urlParams`），实时协作固定为 manual，导出服务器固定为 null（PDF 走浏览器
  打印，PNG/JPEG/SVG 在浏览器本地渲染），图标搜索和日志上报由 `PostConfig.js` 关闭。
- 实测（Chromium 1034 起的 DevTools 资源清单）：打开页面产生的全部请求只有本站一个源；页面 HTML 里仅剩的
  两个站外字符串是 `<link rel="canonical">` 和 itemprop 图片 meta，属于元数据，浏览器不会去取。
- 图里嵌入的外部图片、音视频和自定义字体（用户自己贴的网址）仍由浏览器按网址加载（CSP 的 `img-src *`、
  `media-src *`、`font-src *`），这是用户主动加的内容；不需要就在画图时别贴外部网址。

## 已知限制

- 图存在浏览器里：浏览器隐私模式、清理缓存、换设备都拿不回来，重要图纸导出文件保存。
- 没有协作：多人各画各的，同一个浏览器里同一时间只有一份未保存状态。
- PDF 导出走浏览器的打印对话框（上游镜像不带导出服务器）。
- 手机竖屏（390×844）：见 `screenshots/`，编辑器可用但画布和工具栏拥挤，画正式的图建议用平板或电脑。

## 图标来源

`icon.svg` 使用 draw.io 的官方标志（上游镜像里网页用的同一枚），来源、版权、Apache-2.0 许可全文和商标说明见
[NOTICE](NOTICE)。

## 先跑起来

在合仓根目录运行：

```bash
export WONTON_PLUGIN=drawio
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞并打印它的地址，用 `scripts/dev.sh token` 输出的用户令牌登录；`install` 列出插件要的
权限（本插件没有），确认后装上并打印插件网页的地址，打开就是 draw.io 编辑器。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：compose 加上 `x-wonton` 清单 |
| `icon.svg` | 清单引用的图标，取自上游官方标志 |
| `screenshots/` | 商店详情页里的截图 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `drawio/smoke-probes.sh` | `scripts/dev.sh smoke` 对这个插件逐项的探测 |
| `scripts/publish.sh` | 发布一个版本：打 tag、推送 |
| `../AGENTS.md` | 合仓根目录给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE` | 插件许可证、上游 Apache-2.0 许可证和商标声明 |
