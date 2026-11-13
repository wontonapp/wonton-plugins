# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
# shellcheck shell=bash
#
# Ollama 是本地模型服务：清单没有 webui 和 mcp，网页、身份、工具三项按清单记 N/A；
# 也没有应用用户体系（IDENTITY_NA=1）。真正的验收是模型经模型网关调通——那要先把
# 局域网端口注册进模型设置，不在探针里做。
export IDENTITY_NA=1
export SMOKE_NOTE="本地模型服务，没有自带网页和用户体系；模型经模型网关调通的验收在装好后的模型设置里做"

# 数据：Ollama 的用户数据就是模型（/root/.ollama，升级重装都保留）。data_write 先拉上游官方库里
# 最小的主流模型 all-minilm:22m（约 46 MB）做底，再 ollama cp 出一个带随机标记的引用；拉模型要出网
# （registry.ollama.ai），环境没配出网时这里如实失败。
# 容器按核心打的标签找，不拼名字：换 Bot 后缀、实例名导出时序都不影响。
_ollama_container() {
  local inst="${SMOKE_INSTANCE:-}" args=(--filter label=wonton.plugin=com.ollama.ollama --filter label=wonton.service=ollama)
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

data_write() {
  local marker container base
  [[ -n "${SMOKE_STATE:-}" ]] || { echo "SMOKE_STATE 没设"; return 1; }
  marker="wonton-smoke-$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
  container="$(_ollama_container)"
  [[ -n "$container" ]] || { echo "找不到 ollama 容器（wonton.instance 标签）"; return 1; }
  base="all-minilm:22m"
  docker exec "$container" ollama pull "$base" >/dev/null ||
    { echo "拉 $base 失败（容器 $container 出网不可用？）"; return 1; }
  docker exec "$container" ollama cp "$base" "$marker" ||
    { echo "ollama cp 成 $marker 失败"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() {
  local marker container listing
  [[ -n "${SMOKE_STATE:-}" ]] || { echo "SMOKE_STATE 没设"; return 1; }
  marker="$(cat "$SMOKE_STATE")" || { echo "读不到标记文件"; return 1; }
  container="$(_ollama_container)"
  [[ -n "$container" ]] || { echo "找不到 ollama 容器（wonton.instance 标签）"; return 1; }
  listing="$(docker exec "$container" ollama ls)" || { echo "ollama ls 失败"; return 1; }
  # ollama ls 的 NAME 列带 :tag（ollama cp 得到的是 :latest），只比模型名部分。
  awk -v m="$marker" '{ n = $1; sub(/:.*/, "", n); if (n == m) found = 1 } END { exit !found }' <<<"$listing" ||
    { echo "模型标记不在了：找了 $marker，ollama ls 现在是：$(head -c 200 <<<"$listing")"; return 1; }
}
