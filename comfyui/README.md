# 云吞插件：ComfyUI

[ComfyUI](https://www.comfy.org) 是节点式的生图工作流应用。这个插件把它装在云吞里：网页经云吞网关打开，显卡由云吞交给它，数据放在插件数据目录。开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)。

## 模型

模型不进镜像，放在插件数据目录的 `models/` 下（装好后对应宿主路径 `<云吞数据>/apps/` 里的 `data/`），重启后都在：

| 目录 | 放什么 |
| --- | --- |
| `models/checkpoints/` | 大模型（checkpoint） |
| `models/diffusion_models/`、`models/text_encoders/`、`models/vae/` | 分体式模型的各部分 |
| `models/loras/` | LoRA |
| `models/upscale_models/`、`models/controlnet/` | 放大、控制模型 |

装好的 ComfyUI-Manager 也能在网页里直接下载模型和自定义节点（要走出口代理或直连）。

推荐的起步模型：SDXL base 1.0（`sd_xl_base_1.0.safetensors`，约 6.9 GB），1024×1024 生成最低约 8 GB 显存，32 GB 显卡很宽裕；更大的模型按文件大小和官方说明自估显存。

## 镜像

上游（comfyanonymous/ComfyUI）没有发布官方镜像。NVIDIA 主机用 `images/comfyui/Dockerfile` 基于官方 PyTorch cu128 运行时镜像构建（RTX 50 系需要 CUDA 12.8+），ComfyUI 用上游发布的 v0.44.0，不改上游源码；AMD 主机用 AMD 官方发布的 `rocm/comfyui` 镜像（ROCm 7.2），里面的 ComfyUI 是 AMD 的 `amd-integration` 分支（0.18.1），和 NVIDIA 镜像的版本不同，界面和功能有差别。构建由 `scripts/publish.sh` 完成。

AMD 镜像自带的 PyTorch 只编译了数据中心卡（gfx942/950），Radeon 核显上第一次生成会崩。AMD 第一次启动会把官方 ROCm 7.2 的 PyTorch 轮子装进插件数据目录（约 6.4 GB，只需一次，之后重启复用）。

AMD 第一次启动的下载计入安装等待：`start_period` 30 分钟，再加约 1.5 分钟的判定窗，约 31 分钟下不完就会判不健康、安装失败；安装等健康的期限平台最多也只等到 60 分钟。6.4 GB 要稳定在 3.5 MB/s 以上，慢于此的网络第一次装会失败，重装会从头下载。只在 Radeon 8060S（gfx1151）上验证过，其他 Radeon 型号没有实测。

## 图标

`icon.png` 来自 ComfyUI 前端仓库的官方标志，ComfyUI 名称和标志是 Comfy Org 的商标；来源、许可和商标说明见 [NOTICE](NOTICE)。
