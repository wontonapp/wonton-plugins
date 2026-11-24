# shellcheck shell=bash
# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# scrutiny：应用无账号体系（sso: none），无 MCP；用户数据 = 应用数据库里的设备记录。
# 要一块整盘：SMOKE_DEVICE=auto 让 dev.sh 从预览里挑（系统盘排后），探针再从容器里读实际交进来的节点。
# 盘只读交给容器，collector 在启动时跑一次；这里断言网页、健康接口、容器里的设备节点确实只读、collector
# 日志里有这块盘、/run/udev 只读挂着，并用“友好名称”验证重装后应用数据还在。

web_assert() { grep -qi '<title>scrutiny</title>'; }

export SMOKE_DEVICE=auto

# 页面纯前端，身份由网关会话保证；应用自己没有“当前用户”接口。
export IDENTITY_NA=1
export SMOKE_NOTE="应用无账号体系，身份由网关保证"

_container() { # <插件 id> <服务名>：本实例里这个服务的容器
  local inst="${SMOKE_INSTANCE:-}" id
  local base=(--filter "label=wonton.plugin=$1" --filter "label=wonton.service=$2")
  if [[ -n "$inst" ]]; then
    id="$(docker ps --format '{{.Names}}' "${base[@]}" --filter "label=wonton.instance=${inst//-/}" | head -1)"
  fi
  if [[ -z "${id:-}" && -n "${SMOKE_BASE:-}" ]]; then
    id="$(docker ps --format '{{.Names}}' "${base[@]}" | head -1)"
  fi
  printf '%s' "${id:-}"
}

# 容器里拿到的整盘节点（核心按勾选顺序命名，只有一块）。
_node() { # <容器 id>
  docker exec "$1" sh -c 'for n in /dev/sd? /dev/nvme?n1; do [ -b "$n" ] && echo "$n"; done' | head -1
}

data_write() {
  local id uuid marker summary
  id="$(_container io.github.analogj.scrutiny scrutiny)" || { echo '找不到 scrutiny 容器'; return 1; }
  [[ -n "$id" ]] || { echo '找不到 scrutiny 容器'; return 1; }
  # collector 在容器启动后跑一次，等它把盘登记进库。
  for _ in $(seq 1 90); do
    summary="$(dweb /api/summary)" || { echo "读设备列表失败：$summary"; return 1; }
    uuid="$(jq -r '(.data.summary // {}) | keys_unsorted | first // empty' <<<"$summary" 2>/dev/null)"
    [[ -n "$uuid" ]] && break
    sleep 2
  done
  [[ -n "${uuid:-}" ]] || { echo "180 秒内 collector 没登记任何盘，容器日志末尾：$(docker logs "$id" 2>&1 | tail -c 400)"; return 1; }
  marker="smoke-$(date +%s)-$RANDOM"
  dweb "/api/device/$uuid/friendly_name" -X POST -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg name "$marker" '{friendly_name:$name}')" >/dev/null || { echo "写友好名称失败"; return 1; }
  printf '%s %s' "$uuid" "$marker" >"$SMOKE_STATE"
}

data_check() {
  local uuid marker details
  read -r uuid marker <"$SMOKE_STATE" || true
  [[ -n "$uuid" && -n "$marker" ]] || { echo 'SMOKE_STATE 里没有 uuid 和标记'; return 1; }
  details="$(dweb "/api/device/$uuid/details")" || { echo "读设备详情失败：$details"; return 1; }
  jq -e --arg name "$marker" '.data.device.friendly_name == $name' <<<"$details" >/dev/null ||
    { echo "友好名称对不上：$(jq -c '.data.device.friendly_name' <<<"$details" 2>/dev/null || head -c 300 <<<"$details")"; return 1; }
}

extra_check() {
  local id node health
  id="$(_container io.github.analogj.scrutiny scrutiny)"
  [[ -n "$id" ]] || { extra_item FAIL 容器 "找不到 scrutiny 容器"; return 1; }

  health="$(dweb /api/health)" || health=''
  if jq -e '.success == true' <<<"$health" >/dev/null 2>&1; then
    extra_item PASS 健康接口 "/api/health 回了 success"
  else
    extra_item FAIL 健康接口 "/api/health 没回 success：$(head -c 200 <<<"$health")"
  fi

  node="$(_node "$id")"
  if [[ -z "$node" ]]; then
    extra_item FAIL 设备节点 "容器里没有 /dev/sd* 或 /dev/nvme*n1"
    return 1
  fi
  if docker exec "$id" sh -c "dd if='$node' of=/dev/null bs=512 count=1" >/dev/null 2>&1; then
    extra_item PASS 设备节点 "读得通 $node"
  else
    extra_item FAIL 设备节点 "读不通 $node"
  fi
  # 设备按只读交给容器：读出扇区再原样写回，写不进去说明是只读。
  if docker exec "$id" sh -c "dd if='$node' bs=512 count=1 2>/dev/null | dd of='$node' bs=512 count=1" >/dev/null 2>&1; then
    extra_item FAIL 只读 "$node 能写进去"
  else
    extra_item PASS 只读 "$node 写不进去"
  fi
  if docker exec "$id" sh -c 'mount | grep -q " /run/udev "' 2>/dev/null; then
    extra_item PASS udev "宿主 /run/udev 挂进了容器"
  else
    extra_item FAIL udev "容器里没挂 /run/udev"
  fi

  # collector 在启动时跑一次，日志里要有这块盘和它跑完的记录；最多等 120 秒。
  # smartctl 对 NVMe 写的是控制器名（nvme0），不是交付给容器的命名空间（nvme0n1），所以按控制器路径找。
  local ctrl="${node%n1}" logs
  for _ in $(seq 1 60); do
    logs="$(docker logs "$id" 2>&1)"
    if grep -qF -- "--device" <<<"$logs" && grep -qF "$ctrl" <<<"$logs" &&
      grep -qF "Publishing smartctl results for" <<<"$logs"; then
      extra_item PASS collector "日志里读了 $ctrl 并把结果交给了网页"
      return
    fi
    sleep 2
  done
  extra_item FAIL collector "120 秒内日志里没有 $ctrl 的采集结果，末尾：$(docker logs "$id" 2>&1 | tail -c 300 | tr '\n' ' ')"
  return 1
}
