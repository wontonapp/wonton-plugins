# 云吞插件：Copyparty

[Copyparty](https://github.com/9001/copyparty)（MIT）做成云吞插件：网页网盘，上传、下载、分享文件，图片、视频、音乐、PDF 直接预览，支持分享链接收文件和带口令/限时的下载链接。

## 登录

上游没有原生 OIDC，但支持信任反向代理注入的用户名头（上游 README 的 identity providers 一节）。插件用云吞的 `sso: proxy`：网关代登录，打开就是自己的文件，不用第二次登录。

只信任网关，具体是三道闸，缺一不可（配置见 `docker-compose.yml` 里的内联 `copyparty.conf`）：

1. `xff-src` 只写核心网关的一个地址（`${WONTON_CORE_ADDRESS}/32`），其他来源送来的用户名头一律不信任，按匿名处理；
2. `idp-h-key` 要求请求里同时出现 `X-Wonton-Auth` 头；
3. `X-Wonton-User`、`X-Wonton-Auth` 两个头的名字都写在清单的 `webui.proxy.headers` 里，用户伪造的同名头会被网关先删掉再注入真实值。

上游容器镜像另有自带兜底：Docker 里检测不到配置定义的卷时，读权限默认关闭（`Read-access has been disabled due to failsafe`），配置文件解析失败则直接退出，不会退回"对目录全开放"的默认行为。

## 文件与目录

- 文件放在插件数据目录的 `files/` 下，索引和缩略图库在 `files/.hist/`，容器重建后都在。
- 卷地址 `/` 是文件根；登录用户有读写权限（copyparty 的 `@acct` 组，即全部已知用户），匿名没有权限。
- 上游文档推荐的视频/音频转码、缩略图依赖（ffmpeg、Pillow）都在官方 `copyparty/ac` 镜像里，本插件不改上游镜像。

## 图标

`icon.png` 用上游官方标志，来源和许可见 [NOTICE](NOTICE)。

## 开发

在合仓根目录运行以下命令：

```bash
export WONTON_PLUGIN=copyparty
scripts/check.sh --official   # 插件包检查
scripts/dev.sh up             # 起开发环境
scripts/dev.sh install        # 安装本插件
scripts/dev.sh probes         # 跑插件探针
```

流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。
