# 上游（comfyanonymous/ComfyUI）没有发布官方镜像，按任务要求自建：
# 基础镜像用 NVIDIA 官方的 CUDA 12.8 镜像（RTX 50 系需要 CUDA 12.8+ / sm_120），
# torch 用 PyTorch 官方 cu128 wheel（官方发布的构建），ComfyUI 用上游发布的 v0.44.0 tag，
# 不改上游源码。
FROM nvidia/cuda:12.8.1-base-ubuntu22.04@sha256:001469ea0f3dec85a1ca929aeea3b58ae369d4c11228b10aec1f642bb6ca7a6f

# python3.10：ComfyUI 要求 >=3.10；git：ComfyUI-Manager 安装自定义节点用。
# apt/pip 走清华镜像：本机到 archive.ubuntu.com、files.pythonhosted.org 的大传输会停滞
# （实测 69 KB/s / 超时，镜像 300-600 KB/s；下载内容与官方源一致，仅影响构建速度）。
RUN sed -i 's|http://archive.ubuntu.com/ubuntu|https://mirrors.tuna.tsinghua.edu.cn/ubuntu|g; s|http://security.ubuntu.com/ubuntu|https://mirrors.tuna.tsinghua.edu.cn/ubuntu|g; s|http://cn.archive.ubuntu.com/ubuntu|https://mirrors.tuna.tsinghua.edu.cn/ubuntu|g' /etc/apt/sources.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends python3 python3-pip git \
    && rm -rf /var/lib/apt/lists/*

# PyTorch 官方 cu128 构建（含 sm_120）。pip 的 nvidia-* 依赖自带 cudnn，base 镜像即够；
# torch 在 ComfyUI 的 requirements 里不锁版本，这里装好后不会被 pip 换成 CPU/其他 CUDA 构建。
RUN pip install --no-cache-dir \
    torch==2.7.1+cu128 torchvision==0.22.1+cu128 \
    --index-url https://download.pytorch.org/whl/cu128

# 上游发布版本：v0.44.0（2026-11-04）。依赖按上游 requirements 安装。
# tuna 为主索引（快），官方 PyPI 兜底（镜像源对新包有同步延迟）。
ARG COMFYUI_VERSION=v0.44.0
RUN git clone --depth 1 --branch ${COMFYUI_VERSION} https://github.com/comfyanonymous/ComfyUI /opt/ComfyUI \
    && pip install --no-cache-dir -i https://pypi.tuna.tsinghua.edu.cn/simple \
    --extra-index-url https://pypi.org/simple \
    -r /opt/ComfyUI/requirements.txt -r /opt/ComfyUI/manager_requirements.txt

# triton 运行时编译内核需要 C 编译器和 Python 头文件（有显卡时 torch 会触发）。放在依赖层之后：
# 这层只有 ~40 MB，改动不用重新下载 torch。
RUN apt-get update \
    && apt-get install -y --no-install-recommends gcc python3-dev \
    && rm -rf /var/lib/apt/lists/*

# 启动前补 libcuda.so 链接软链（见 entrypoint.sh）。
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]

WORKDIR /opt/ComfyUI
EXPOSE 8188
# 全部可变状态（models、custom_nodes、input、output、user）随 --base-directory 进数据目录；
# temp 指到容器本地 /tmp，重启即弃。
CMD ["python3", "main.py", "--listen", "0.0.0.0", "--port", "8188", "--base-directory", "/data", "--temp-directory", "/tmp/comfy", "--enable-manager"]
