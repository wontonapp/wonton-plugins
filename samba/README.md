# 云吞插件：Samba

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

这个插件把用户的共享文件夹导出成 SMB 共享：局域网里的电脑（macOS 的 Finder、Windows 的资源管理器、Linux 的文件管理器）
直接挂载 `photos`、`documents`、`music`、`downloads`、`transfers`，读写照片、文档、音乐和设备传进来的文件。插件网页显示
连接地址、用户名和密码。

## 许可证

插件包及连接信息页采用 [MIT License](LICENSE)，上游 Samba 标志不适用该许可，标志的许可与商标说明见 [NOTICE](NOTICE)。
独立容器中的应用及镜像包装代码各自遵循上游许可证；清单的 `license` 填 Samba 的许可证。

| 组件 | 许可证与来源 |
| --- | --- |
| Samba 4.23.10 | [GPL-3.0-or-later，发行源码头](https://github.com/samba-team/samba/blob/samba-4.23.10/source3/smbd/server.c#L10-L13) |
| dockurr/samba 镜像包装代码 | [MIT](https://github.com/dockur/samba/blob/fb02ffebb6c0f8f82e1136013961b4c2459d9933/license.md) |
| CPython 3.12.16（含 `http.server`） | [PSF-2.0 及随发行版保留的历史许可](https://github.com/python/cpython/blob/v3.12.16/LICENSE) |
| Python 官方镜像包装代码 | [MIT](https://github.com/docker-library/python/blob/3d7e95597e56a415d9e4abc0f15877e005d2cb2e/LICENSE) |

## 图标来源

`icon.png` 使用 Samba 官网的完整官方标志，原文件保存在 [assets/samba_logo_2010_grey.svg](assets/samba_logo_2010_grey.svg)；来源、版权、处理方式、上游 GPL-3.0 全文和商标说明见 [NOTICE](NOTICE)。
Samba 名称及标志是 Samba 项目的商标，权利归其权利人所有；此处仅用于标识插件运行的上游应用，
不表示本插件与 Samba 项目存在官方合作、隶属、赞助或认可关系。

重现栅格化：

```bash
rsvg-convert -w 388 -h 68 assets/samba_logo_2010_grey.svg -o /tmp/samba-wordmark.png
magick /tmp/samba-wordmark.png -background white -alpha remove -alpha off -gravity center -extent 512x512 -strip PNG24:icon.png
```

## 先跑起来

```bash
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞并打印它的地址，用 `scripts/dev.sh token` 输出的用户令牌登录；`install` 列出插件要的权限，确认后
装上并打印插件网页的地址，打开就是连接信息页（地址、用户名、密码）。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：compose 加上 `x-wonton` 清单 |
| `icon.png`、`web/`、`assets/` | 清单和 compose 引用的文件：图标、交给容器的配置、连接信息页；图标的原始素材 |
| `screenshots/` | 商店详情页里的截图 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `scripts/smoke-probes.sh` | `scripts/dev.sh smoke` 对这个插件逐项的探测 |
| `scripts/publish.sh` | 发布一个版本：构建自己的镜像、按摘要写回、打 tag、推送 |
| `AGENTS.md` | 给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE` | 插件许可证、上游 GPL-3.0 许可证和商标声明 |
