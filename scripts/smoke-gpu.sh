#!/usr/bin/env bash
# 只判断 Docker 主机是否有显卡硬件；厂商、显存、驱动和设备授予仍由核心安装检查。
set -uo pipefail

gpu_present() {
  local plugin_dir="$1" host_json rc=0 unmet
  host_json="$(LC_ALL=C.UTF-8 docker run --rm --network none \
    -v "$plugin_dir:/plugin:ro" -v /dev:/host-dev:ro -v /sys:/host-sys:ro \
    -e WONTON_HOST_DEV=/host-dev "$(core_image)" plugin host /plugin 2>&1)" || rc=$?
  if ((rc != 0)); then
    echo "$host_json"
    return "$rc"
  fi
  unmet="$(jq -er '[.requirements[]? | select(.status == "unmet" and ((.text // "") | test("GPU|gpu|显卡|NVIDIA|AMD|Intel")))] | length' \
    <<<"$host_json" 2>&1)" || {
    echo "核心 plugin host 输出不是有效 JSON：$unmet"
    return 1
  }
  if ((unmet > 0)); then
    return 77
  fi
  return 0
}

main() {
  # shellcheck source=scripts/lib.sh
  . "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
  need docker jq
  [[ $# -le 1 ]] || { echo "用法：smoke-gpu.sh [插件目录]" >&2; return 2; }
  local plugin_dir="${1:-$plugin}" config gpu_need probe rc=0
  [[ -f "$plugin_dir/docker-compose.yml" ]] || { echo "GPU 冒烟：找不到清单 $plugin_dir/docker-compose.yml" >&2; return 1; }
  config="$(docker compose -f "$plugin_dir/docker-compose.yml" config --no-interpolate --format json)" || {
    echo "GPU 冒烟：docker compose config 解析失败" >&2
    return 1
  }
  gpu_need="$(jq -r '
    .["x-wonton"] as $x | $x.requirements.gpu as $gpu
    | if $gpu != null then
        if ($gpu | type) != "object" then error("requirements.gpu 必须是对象")
        else ($gpu.need // "required") end
      elif (($x.permissions // []) | index("devices.gpu")) != null then "required"
      else "" end' <<<"$config")" || return 1
  case "$gpu_need" in
    ""|optional) printf 'run=true\n' >>"${GITHUB_OUTPUT:-/dev/null}"; return 0 ;;
    required) ;;
    *) echo "GPU 冒烟：requirements.gpu.need 值非法：$gpu_need" >&2; return 1 ;;
  esac
  probe="$(gpu_present "$plugin_dir" 2>&1)" || rc=$?
  case "$rc" in
    0) printf 'run=true\n' >>"${GITHUB_OUTPUT:-/dev/null}" ;;
    77)
      echo "跳过：需要显卡，没跑（核心判定本机没有可用显卡）"
      printf 'run=false\n' >>"${GITHUB_OUTPUT:-/dev/null}"
      return 77
      ;;
    *) echo "GPU 冒烟：核心 plugin host 检查失败（退出 $rc）：$probe" >&2; return 1 ;;
  esac
}

main "$@"
