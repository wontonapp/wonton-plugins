# 云吞插件：FileBrowser Quantum

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

[FileBrowser Quantum](https://github.com/gtsteffaniak/filebrowser) 的云吞插件：用户在 App 里浏览、预览、上传、
下载、整理共享文件夹里的文件。

用 Quantum 分支而不是原版 [filebrowser/filebrowser](https://github.com/filebrowser/filebrowser)：原版 2026-09-01 归档，不再发版和修安全问题，
也没有 OIDC 登录；Quantum 在持续发版，自带 OIDC，能把多个共享文件夹各作为一个源。

## 插件行为

- 登录用 `sso: oidc`：由核心的身份提供方登录，打开插件网页就是当前用户的文件页，不需要再登录一次。
- 插件自己的探活脚本 `healthcheck.sh` 在 FileBrowser 的运行 UID 下列出六个共享根目录；对三个读写共享及应用数据、
  缓存目录创建、写入、读回并删除临时探针，最后请求上游 `/health`。v2.0.2-stable 的 `/health` 只报告 HTTP 存活，
  不能单独证明存储可用。探针不递归检查全部文件，也不证明整个索引已扫描完成；检查不会修改已有文件或只读共享，失败
  原因进入 Docker 健康检查日志。
- 上游 v2.0.2-stable 在配置初始化的 `setupFs → testCacheDirSpeed` 中创建 `cacheDir`，并完成写入、读取、删除检查，
  早于 HTTP 监听（[上游源码](https://github.com/gtsteffaniak/filebrowser/blob/2c071d1e6eb48d4b88da2657885d345663a0a88b/backend/pkg/settings/config.go#L142-L244)）。
  插件 v0.1.0 和 v0.1.2 使用相同的镜像摘要与缓存路径；已有目录不会因重建容器自动改属主或权限。健康检查不创建缺失
  的目录，也不修权限；失败日志区分路径、目录类型、读、搜索和写权限，并附运行身份、目标及父目录的数字属主和权限，
  避免把目录缺失误诊成权限问题。网页能打开或 `/health` 返回 200 不代表缓存仍可用。

## 图标来源与许可

`icon.png` 使用 FileBrowser Quantum v2.0.2-stable 的官方 favicon.svg，未用图片模型生成；来源、版权、处理方式、
上游 Apache-2.0 全文和商标说明见 [NOTICE](NOTICE)。插件包自有代码的 MIT 许可见 [LICENSE](LICENSE)。

## 先跑起来

在合仓根目录运行：

```bash
export WONTON_PLUGIN=filebrowser
scripts/dev.sh up
scripts/dev.sh install
```

`up` 在本机起一套云吞，打印它的地址。浏览器打开，用 `scripts/dev.sh token` 输出的用户令牌登录；`install` 先列出
插件要的权限，确认后装上，打印插件网页的地址；打开就是用户的文件页。Chrome、Firefox 和 curl 把 `*.localhost`
直接解析到本机，不用改 hosts。

## 仓库里有什么

| 路径 | 内容 |
| --- | --- |
| `docker-compose.yml` | 插件本身：compose 加上 `x-wonton` 清单 |
| `healthcheck.sh` | 探活脚本：检查共享目录读写和上游 `/health` |
| `icon.png` | 清单引用的满幅 PNG 图标，来源与许可见上文 |
| `screenshots/` | 商店详情页里的截图 |
| `scripts/check.sh` | 检查插件包，规则和安装时一样 |
| `scripts/dev.sh` | 开发环境：起一套云吞，装、重装、卸载插件，看日志 |
| `filebrowser/smoke-probes.sh` | `scripts/dev.sh smoke` 对这个插件逐项的探测 |
| `scripts/publish.sh` | 发布一个版本：构建自己的镜像、按摘要写回、打 tag、推送 |
| `../AGENTS.md` | 合仓根目录给人和 AI 编程代理的规则 |
| `LICENSE`、`NOTICE` | 插件许可证、上游许可证和商标声明 |
| `.wonton/` | 开发环境的设置、密钥和数据，不提交 |
