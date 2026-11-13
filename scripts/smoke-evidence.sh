# shellcheck shell=bash
# extra_check 失败时保留同一窗口的证据；只读现场，不用空收件箱推断投递成功。

# 钩子在发信前调用；钩子跑在子 shell，用文件把发送时刻交回取证方。
probes_evidence_start() {
  date -u '+%Y-%m-%dT%H:%M:%SZ' >"${SMOKE_EXTRA_SINCE_FILE:?probes_evidence_start 只能在 extra_check 里用}"
}

probes_evidence_logs() { # <时间> <容器名>
  local since="$1" container="$2" rc=0
  echo "== 容器日志：$container（--since $since）=="
  timeout 30 docker logs --since "$since" --timestamps "$container" 2>&1 || rc=$?
  ((rc == 0)) || echo "读取 $container 日志失败（退出 $rc）"
  return 0
}

# 在核心自己的网络栈里请求，保留 HTTP 头区分 HTTP 错误和合法空邮箱；密钥不离开核心。
probes_mailpit_request() {
  # shellcheck disable=SC2016 # 单引号里是核心容器内的变量。
  timeout 20 docker exec "$SMOKE_CORE" timeout 15 bash -c '
    set -euo pipefail
    password="$(cat "$1")" || { echo "读不到 Mailpit API 密钥" >&2; exit 65; }
    [[ -n "$password" ]] || { echo "Mailpit API 密钥为空" >&2; exit 65; }
    auth="$(printf "core:%s" "$password" | base64 | tr -d "\n")"
    exec 3<>/dev/tcp/mailpit.wonton.internal/8025 || { echo "连不上 Mailpit（DNS 或连接失败）" >&2; exit 69; }
    printf "GET /api/v1/messages?limit=50 HTTP/1.0\r\nHost: mailpit.wonton.internal\r\nAuthorization: Basic %s\r\nConnection: close\r\n\r\n" "$auth" >&3
    cat <&3
  ' -- "$SMOKE_DATA_DIR/core/mailpit/api-password"
}

probes_mailpit_evidence() {
  local raw status code body summary reason rc=0
  if [[ -z "${SMOKE_CORE:-}" || -z "${SMOKE_DATA_DIR:-}" ]]; then
    echo "Mailpit 查询失败：缺少 SMOKE_CORE / SMOKE_DATA_DIR"
    return 0
  fi
  raw="$(probes_mailpit_request 2>&1)" || rc=$?
  if ((rc != 0)); then
    case "$rc" in
      65) reason="读取 API 密钥失败" ;;
      69) reason="连不上（DNS 或连接失败）" ;;
      124) reason="连接或读取超时" ;;
      *) reason="执行查询失败" ;;
    esac
    echo "Mailpit 查询失败：$reason（退出 $rc）：$raw"
    return 0
  fi
  IFS= read -r status <<<"$raw"
  status="${status%$'\r'}"
  if [[ ! "$status" =~ ^HTTP/[0-9.]+\ ([0-9]{3})(\ |$) ]]; then
    echo "Mailpit 查询失败：HTTP 响应解析失败：$status"
    return 0
  fi
  code="${BASH_REMATCH[1]}"
  if [[ "$code" != 2* ]]; then
    echo "Mailpit 查询失败：HTTP $code（$status）"
    return 0
  fi
  body="$(sed '1,/^\r\{0,1\}$/d' <<<"$raw")"
  if summary="$(jq -er '
    if type != "object" or (.messages | type) != "array" or (.total | type) != "number" then
      error("需要 total 数字和 messages 数组")
    elif .total == 0 and (.messages | length) == 0 then
      "收件箱为空；不能据此判断已投递，仍以私聊消息为证"
    elif .total > 0 then
      "收件箱共 \(.total) 封，以下为本次查询的 \(.messages | length) 封：",
      (.messages[] | "\(.From.Address) → \((.To // []) | map(.Address) | join(","))｜\(.Subject)")
    else error("total 与 messages 不一致") end' <<<"$body" 2>&1)"; then
    printf 'Mailpit 查询成功：%s\n' "$summary"
  else
    echo "Mailpit 查询失败：JSON 解析失败（HTTP $code）：$summary"
  fi
  return 0
}

probes_plugin_log_evidence() { # <时间>
  local since="$1" names container rc=0
  if [[ -z "${SMOKE_INSTANCE:-}" || -z "${SMOKE_PLUGIN_ID:-}" ]]; then
    echo "插件应用日志：缺少 SMOKE_INSTANCE / SMOKE_PLUGIN_ID，未查询其他实例"
    return 0
  fi
  names="$(timeout 20 docker ps -a --filter "label=wonton.instance=$SMOKE_INSTANCE" \
    --filter "label=wonton.plugin=$SMOKE_PLUGIN_ID" --format '{{.Names}}' 2>&1)" || rc=$?
  if ((rc != 0)); then
    echo "列出插件应用容器失败（退出 $rc）：$names"
    return 0
  fi
  if [[ -z "$names" ]]; then
    echo "插件应用日志：没有找到实例 $SMOKE_INSTANCE 的插件 $SMOKE_PLUGIN_ID 容器（含已停止）"
    return 0
  fi
  while IFS= read -r container; do
    probes_evidence_logs "$since" "$container"
  done <<<"$names"
}

probes_extra_failure_evidence() { # <时间>
  local since="$1"
  echo "== extra_check 失败取证（从 $since）=="
  if [[ -n "${SMOKE_CORE:-}" ]]; then
    probes_evidence_logs "$since" "$SMOKE_CORE"
  else
    echo "核心日志：缺少 SMOKE_CORE"
  fi
  probes_mailpit_evidence
  probes_plugin_log_evidence "$since"
}
