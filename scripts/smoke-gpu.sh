#!/usr/bin/env bash
# 只判断 Docker 主机是否有显卡硬件；厂商、显存、驱动和设备授予仍由核心安装检查。
set -uo pipefail

gpu_present() {
  # 标准输出只放核心的 JSON；标准错误（拉镜像的进度、“Unable to find image”）单独收进临时文件，
  # 混进 JSON 会让 jq 解析失败。失败时两路都打印，方便取证。
  local plugin_dir="$1" host_json host_err err_file rc=0 unmet
  err_file="$(mktemp)" || {
    echo "GPU 冒烟：无法创建临时文件"
    return 1
  }
  host_json="$(LC_ALL=C.UTF-8 docker run --rm --network none \
    -v "$plugin_dir:/plugin:ro" -v /dev:/host-dev:ro -v /sys:/host-sys:ro \
    -e WONTON_HOST_DEV=/host-dev "$(core_image)" plugin host /plugin 2>"$err_file")" || rc=$?
  host_err="$(cat "$err_file")"
  rm -f "$err_file"
  if ((rc != 0)); then
    printf '标准输出：\n%s\n' "$host_json"
    [[ -z "$host_err" ]] || printf '标准错误输出：\n%s\n' "$host_err"
    return "$rc"
  fi
  unmet="$(jq -er '[.requirements[]? | select(.status == "unmet" and ((.text // "") | test("GPU|gpu|显卡|NVIDIA|AMD|Intel")))] | length' \
    <<<"$host_json" 2>&1)" || {
    echo "核心 plugin host 输出不是有效 JSON：$unmet"
    printf '标准输出：\n%s\n' "$host_json"
    [[ -z "$host_err" ]] || printf '标准错误输出：\n%s\n' "$host_err"
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
  local plugin_dir="${1:-$plugin}" probe rc=0
  [[ -f "$plugin_dir/docker-compose.yml" ]] || { echo "GPU 冒烟：找不到清单 $plugin_dir/docker-compose.yml" >&2; return 1; }
  plugin_dir="$(cd "$plugin_dir" && pwd)" || { echo "GPU 冒烟：无法定位插件目录 $plugin_dir" >&2; return 1; }
  # 核心 plugin host 直接按清单的 requirements 判断主机；不展开 Compose 变量，避免清单里的 `:?` 被误解析。
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
