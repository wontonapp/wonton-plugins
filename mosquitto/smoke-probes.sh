# shellcheck shell=bash
# smoke 的数据钩子：用一条保留消息当数据标记（挂在 wonton/smoke 主题上，随 mosquitto.db 持久化）。
# 经 docker exec 进 broker 容器，用镜像里的 mosquitto_pub/sub 和 health 账号；没有网页和 MCP，那两项由套件记 N/A。

_container() { # 本实例里 broker 服务的容器：按服务名+实例标签找。钩子契约只有 SMOKE_STATE/JAR/WEB_URL/OWNER/BASE/TOKEN/INSTANCE
  local inst="${SMOKE_INSTANCE:-}" args=(--filter "label=wonton.service=mosquitto")
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

_mqtt() { # <pub|sub> <参数…>：带 health 账号连容器里的 1883
  local c name=$1; shift
  c="$(_container)" || return 1
  [[ -n "$c" ]] || { echo "找不到 mosquitto 容器"; return 1; }
  local pw
  pw="$(docker exec "$c" printenv WONTON_SECRET_HEALTH_PASSWORD)" || return 1
  docker exec "$c" "mosquitto_${name}" -h 127.0.0.1 -p 1883 -u health -P "$pw" "$@"
}

data_write() {
  local marker
  marker="smoke-$(date +%s)-$RANDOM"
  _mqtt pub -t wonton/smoke -r -m "$marker" || { echo "发布保留消息失败"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() {
  local marker want
  marker="$(cat "$SMOKE_STATE")"
  want="$(_mqtt sub -t wonton/smoke -C 1 -W 10)" || { echo "订阅保留消息失败"; return 1; }
  [[ "$want" == "$marker" ]] || { echo "保留消息不匹配：想要 $marker，读到 $want"; return 1; }
}
