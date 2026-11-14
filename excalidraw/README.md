# 手绘白板（Excalidraw）

云吞（Wonton）插件：[Excalidraw](https://excalidraw.com) 手绘风白板。画布完全在浏览器里运行，
服务端只发静态页面；画布内容保存在浏览器的本地存储里，不经过、也不留在服务器上。

多人协作房间需要上游单独的 [excalidraw-room](https://github.com/excalidraw/excalidraw-room) 服务，
官方镜像不带，本插件不提供（与 excalidraw.com 的自托管说明一致）。

## 图标

`icon.svg` 用的是 Excalidraw 官方标志（上游仓库 v0.18.1 的 `public/favicon.svg`，excalidraw.com 同款），
只把圆角白底改成满幅白底、补了正方形 `viewBox`，图形未改动。来源、版权、许可全文和商标说明见 [NOTICE](NOTICE)。
Excalidraw 的名称与标志是 Excalidraw 项目及其持有者的商标，本插件与其无官方合作关系。

## 镜像

上游官方镜像 `excalidraw/excalidraw`。上游只发布 `latest` 一个标签（另有按提交构建的 `sha-*` 标签，
正式版本没有独立标签），所以按清单列表摘要固定：`excalidraw/excalidraw:latest@sha256:f7ee…`，
多架构，amd64 和 arm64 都有。

## 开发与验证

仓库自带开发套件（`scripts/`），在合仓根目录指定插件目录后使用：

```bash
export WONTON_PLUGIN=excalidraw
scripts/check.sh --official
```

合并前跑端到端冒烟（装得上、网页打得开、身份、工具、升级保数据、重装保数据）。
本插件没有服务端数据与用户体系，冒烟中身份和数据两项按 `excalidraw/smoke-probes.sh` 的声明记 N/A。

## 许可证

- 插件包自身：MIT，见 [LICENSE](LICENSE)。
- 上游 Excalidraw：MIT；图标等素材的声明见 [NOTICE](NOTICE)。
