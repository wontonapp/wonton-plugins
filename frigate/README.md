# 云吞插件：Frigate

[Frigate](https://frigate.video) 是本地 NVR：接家里的摄像头，有动静才录，检测出的人、车、动物带框标出，事件在手机上回看。这个插件把它装在云吞里：网页经云吞网关打开（用云吞账号），显卡由云吞交给它，录像放在插件数据目录。开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

## 摄像头

装好后打开 Frigate 网页，在设置向导里添加摄像头：填摄像头的 RTSP 地址（一般形如 `rtsp://<摄像头地址>:554/...`，见摄像头说明书）。检测、录像区域、通知都在网页里配。

## 检测

用上游官方 `-tensorrt` 镜像（内置 onnxruntime-gpu，CUDA 12.8 运行库，RTX 50 系原生可用）。有 NVIDIA 显卡就自动用显卡：

| | 大小 | 显存 |
| --- | --- | --- |
| 默认模型 YOLOX nano（416×416） | 约 3.7 MB | 不到 0.5 GB |

模型由安装时的一次性服务下载（上游 [YOLOX](https://github.com/Megvii-BaseDetection/YOLOX) 发布的 ONNX，按 sha256 校验），放在插件数据目录 `config/model_cache/` 下，重装、升级不丢。选 nano 是为了让安装时的下载稳定落在安装时限内；要更高精度，把 YOLOX tiny/s 的 ONNX（同一上游仓库）放进同一目录，在 Frigate 网页里把模型路径换掉即可，[YOLOv9](https://docs.frigate.video/configuration/object_detectors) 等按上游文档导出后同样可用。

要改用 TensorRT 执行器（首次推理多花几分钟建引擎，缓存在数据目录），把 Frigate 网页里检测器的 `device` 从 `AUTO` 改成 `Tensorrt`。

## 局域网 RTSP

给 VLC、Home Assistant 这类软件用的 RTSP 回放流：在云吞的插件设置里打开局域网开关（默认关），地址是 `rtsp://<这台主机的局域网地址>:8554/<摄像头名>`。Frigate 网页里的实时画面不经这个端口。

回放流要账号：用户名 `wonton`，密码由云吞生成；地址和账号都在插件详情页里看、可以复制。Frigate 自己取流走 localhost，不受这个认证影响。

## 登录

Frigate 不实现 OIDC；插件用它的官方代理认证：云吞网关在请求上带 `Remote-User`（云吞用户名）和 `X-Proxy-Secret`（安装时生成的密钥），Frigate 关掉自带登录，只认带对密钥的请求，主人以 admin 进网页。

## 图标

`icon.svg` 来自 Frigate 的品牌标志，图形原样、底色按插件规范垫满。Frigate 名称和标志是 Frigate, Inc. 的商标；来源、许可和商标说明见 [NOTICE](NOTICE)。
