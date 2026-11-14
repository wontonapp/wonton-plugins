# shellcheck shell=bash
# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# scanservjs：应用无账号体系（sso: none），无 MCP；用户数据 = 扫描结果文件（/documents/扫描件，
# 即上游 outputDirectory）。没有扫描仪时应用没有写接口，data_write 用 docker exec 把标记写进
# 应用挂载的共享文件夹子目录，data_check 同时验证两件事：重装后文件还在（卷接线没丢），
# 且应用经 /api/v1/files 还能列出它（outputDirectory 重定向在新容器里生效）。

web_assert() { grep -q '<title>scanservjs</title>'; }

# 扫描仪地址：探针给一台 eSCL 扫描仪，冒烟每次安装都带上；地址是文档用地址，不会真连。
export SMOKE_SETTINGS=(airscan_devices='"Scanner" = "http://192.0.2.10/eSCL"')

# 页面是纯前端，身份由网关会话保证；应用自己没有“当前用户”接口。
export IDENTITY_NA=1

_container() { # <插件 id> <服务名>：本实例里这个服务的容器
  local inst="${SMOKE_INSTANCE:-}" id
  local base=(--filter "label=wonton.plugin=$1" --filter "label=wonton.service=$2")
  if [[ -n "$inst" ]]; then
    id="$(docker ps --format '{{.Names}}' "${base[@]}" --filter "label=wonton.instance=${inst//-/}" | head -1)"
  fi
  # SMOKE_INSTANCE 没导出或推导不准时退回只按插件和服务标签找。
  if [[ -z "${id:-}" && -n "${SMOKE_BASE:-}" ]]; then
    id="$(docker ps --format '{{.Names}}' "${base[@]}" | head -1)"
  fi
  printf '%s' "${id:-}"
}

data_write() {
  local id marker
  id="$(_container io.github.sbs20.scanservjs scanservjs)" || { echo '找不到 scanservjs 容器'; return 1; }
  [[ -n "$id" ]] || { echo '找不到 scanservjs 容器'; return 1; }
  marker="smoke-$(date +%s)-$RANDOM"
  docker exec "$id" sh -c "printf '$marker' > '/documents/扫描件/$marker.txt'" \
    || { echo "写标记文件失败"; return 1; }
  printf '%s' "$marker" > "$SMOKE_STATE"
}

data_check() {
  local id marker files
  marker="$(cat "$SMOKE_STATE")"
  [[ -n "$marker" ]] || { echo 'SMOKE_STATE 里没有标记'; return 1; }
  files="$(dweb /api/v1/files)" || { echo "文件列表失败：$files"; return 1; }
  grep -qF "$marker" <<<"$files" || { echo "应用文件列表里没有 $marker：$(head -c 300 <<<"$files")"; return 1; }
  id="$(_container io.github.sbs20.scanservjs scanservjs)"
  if [[ -n "$id" ]]; then
    docker exec "$id" cat "/documents/扫描件/$marker.txt" 2>/dev/null | grep -qF "$marker" \
      || { echo "共享文件夹里的 $marker.txt 读不回来"; return 1; }
  fi
}


# 设置值要真进到容器：airscan_devices 由 SMOKE_SETTINGS 给，saned_hosts 由调用方的 --setting 给（没给就跳过）。
extra_check() {
  local id airscan saned
  id="$(_container io.github.sbs20.scanservjs scanservjs)"
  [[ -n "$id" ]] || { extra_item FAIL 设置 "找不到 scanservjs 容器"; return 1; }
  airscan="$(docker exec "$id" printenv AIRSCAN_DEVICES)"
  if [[ "$airscan" == '"Scanner" = "http://192.0.2.10/eSCL"' ]]; then
    extra_item PASS 设置 "AIRSCAN_DEVICES=$airscan"
  else
    extra_item FAIL 设置 "AIRSCAN_DEVICES 是「$airscan」，不是 SMOKE_SETTINGS 给的地址"
    return 1
  fi
  saned="$(docker exec "$id" printenv SANED_NET_HOSTS)"
  if [[ -z "$saned" ]]; then
    extra_item N/A 设置-命令行 "没给 --setting saned_hosts"
  elif [[ "$saned" == 192.0.2.11 ]]; then
    extra_item PASS 设置-命令行 "SANED_NET_HOSTS=$saned"
  else
    extra_item FAIL 设置-命令行 "SANED_NET_HOSTS 是「$saned」，不是 --setting 给的地址"
    return 1
  fi
}
