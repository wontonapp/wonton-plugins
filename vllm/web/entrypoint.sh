#!/bin/sh
# vLLM 插件的装载入口：模型缓存完整时离线装载（跳过 hf-mirror 的远端校验，
# 实测缓存命中时远端校验仍会发起大文件下载并挂死），缓存缺失时走在线首跑下载。
# 挂载点见 docker-compose.yml 的 entrypoint 配置；参数原样交给 vllm serve。
set -eu
hub="${WONTON_MODEL_HUB:-/data/models/huggingface/hub}"
# 完整=要装载的那个模型权重分片齐全：HF 下载先写 config.json 再下大分片，只看 config
# 存在会把半截下载判成完整，离线模式缺分片直接 LocalEntryNotFoundError，unless-stopped
# 反复重启；不完整时走在线装载，在线能续传。只看 --model 指定的模型——换模型升级时
# 数据目录里还留着旧的完整模型，不能拿它顶替半截的新模型。判据测试见
# scripts/tests/entrypoint-test.sh。
model_id=
prev=
for arg in "$@"; do
  if [ "$prev" = "--model" ]; then
    model_id=$arg
    break
  fi
  prev=$arg
done
complete=0
# hub 只管 HF 仓库 id（org/name）；本地路径不经 HF 缓存，无需离线开关
case "$model_id" in
  ''|/*) ;;
  *)
    case "$model_id" in
      */*) dir_name="models--${model_id%%/*}--${model_id#*/}" ;;
      *) dir_name="models--$model_id" ;;
    esac
    if [ -d "$hub/$dir_name/snapshots" ]; then
      for snapshot in "$hub/$dir_name/snapshots"/*/; do
        [ -d "$snapshot" ] || continue
        [ -f "${snapshot}config.json" ] || continue
        if [ -f "${snapshot}model.safetensors.index.json" ]; then
          # 有索引的模型：weight_map 列出的每个分片都得在（缺一个就走在线续传）
          if python3 - "$snapshot" <<'PY'
import json, os, sys
model = sys.argv[1]
index = json.load(open(os.path.join(model, "model.safetensors.index.json")))
ok = all(os.path.isfile(os.path.join(model, f))
         for f in set(index.get("weight_map", {}).values()))
sys.exit(0 if ok else 1)
PY
          then
            complete=1
          fi
        elif [ -f "${snapshot}model.safetensors" ]; then
          # 单文件权重模型：权重在即完整
          complete=1
        fi
        if [ "$complete" = 1 ]; then
          break
        fi
      done
    fi
    ;;
esac
if [ "$complete" = 1 ]; then
  echo "entrypoint: 模型缓存完整，离线装载（跳过远端校验）"
  HF_HUB_OFFLINE=1 exec vllm serve "$@"
fi
echo "entrypoint: 无完整模型缓存，在线首跑下载"
exec vllm serve "$@"
