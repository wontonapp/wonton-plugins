# 云吞插件：vLLM

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

在自己家里跑 vLLM 推理服务：官方 `vllm/vllm-openai` 镜像，加载一个本地大模型，提供
OpenAI 兼容接口（`/v1/chat/completions` 等）。使用显卡（NVIDIA），显存不够装不上。

## 模型与显存

| | |
| --- | --- |
| 默认模型 | `Qwen/Qwen3-8B`（BF16 权重，下载约 16.4 GB） |
| 最低显存 | 24 GB（权重建模约 16.4 GB 加运行开销） |
| 推荐显存 | 32 GB |

首次启动时从 Hugging Face 下载模型到插件数据目录（`models/huggingface/`），之后不再
重复下载；所以首次启动要等一段下载时间，删除数据目录重装会重新下载。直连
Hugging Face 超时的网络默认走 `hf-mirror.com` 镜像站（`HF_ENDPOINT`，可改回上游默认）；
用核心的出口代理时也可以直接连上游。模型缓存完整后，装载入口脚本检测到缓存齐全就
以离线模式加载（`web/entrypoint.sh`）：实测缓存命中时 hf-mirror 的远端校验仍会对
大分片发起新下载且长时间 0 字节，离线装载暖缓存 3 秒内完成。上下文长度默认
8192，`--gpu-memory-utilization 0.8`，都写在 `docker-compose.yml` 的 `command` 里。

## 接口地址

OpenAI 兼容接口就是插件网页地址加 `/v1`，例如
`https://ai-vllm-vllm.<你的域名>/v1`。它只经核心网关访问：浏览器里用登录会话，
打开插件网页（`/docs`）可以直接试接口；`/v1` 的程序请求同样要带云吞的登录会话，
网关会替已登录的请求注入 API 密钥（安装时生成的 `api_key`），应用侧不用配任何密钥。

以后接进云吞的模型网关：在“模型设置”（模型网关）里把本插件的 `/v1` 地址注册成
自定义 provider 即可，前提是网关的容器能访问插件网络（见插件规范“模型网关”），
这一步需要底座支持，见插件 PR 的说明。

## 图标

`icon.png` 取自 vLLM 官方标志，来源、许可和商标说明见 [NOTICE](NOTICE)。
