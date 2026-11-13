# shellcheck shell=bash disable=SC2034
# SSO/MCP/IDENTITY_NA/SMOKE_NOTE 由探针套件（scripts/probes.sh）读取。

# Frigate：sso=proxy（网关只给登录用户的请求带 X-Proxy-Secret，Frigate 校验 auth_secret）；
# 清单没有 mcp。
SSO=proxy
MCP=0

web_assert() { grep -qi '<title>Frigate</title>'; }

# Frigate 关闭自带登录（auth.enabled=false）后没有“当前用户”接口，也不建用户会话；
# 能打开网页的请求都过了两道门槛：网关会话 + X-Proxy-Secret（对不上 Frigate 直接 401）。
IDENTITY_NA=1
SMOKE_NOTE="Frigate 关闭自带登录后没有当前用户接口；网页只对网关认出的用户开放（X-Proxy-Secret）"

# 用户数据：Frigate 的配置（摄像头、检测、录像设置）存在数据库并写回 config.yml，
# 升级、重装都不能丢。data_write 经 PUT /api/config/set 写一条带随机标记的 logger 名
# （logger.logs 接受任意键，值是日志级别），data_check 从 GET /api/config 读回来核对。
data_write() {
  local marker body code
  [[ -n "${SMOKE_WEB_URL:-}" ]] || { echo "SMOKE_WEB_URL 未设置"; return 1; }
  marker="wonton$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
  body="$(jq -nc --arg k "frigate.wonton.$marker" '{"config_data":{"logger":{"logs":{$k:"info"}}}}')"
  code="$(curl -sS --noproxy '*' -o /dev/null -w '%{http_code}' -b "$SMOKE_JAR" \
    -X PUT -H 'Content-Type: application/json' --data "$body" "$SMOKE_WEB_URL/api/config/set")" ||
    { echo "PUT /api/config/set 请求失败"; return 1; }
  [[ "$code" == 200 ]] || { echo "PUT /api/config/set 返回 $code"; return 1; }
  printf '%s' "$marker" > "$SMOKE_STATE" || { echo "存不了标记：$SMOKE_STATE"; return 1; }
}

data_check() {
  local marker config
  [[ -n "${SMOKE_WEB_URL:-}" ]] || { echo "SMOKE_WEB_URL 未设置"; return 1; }
  marker="$(cat "$SMOKE_STATE" 2>/dev/null)" || { echo "读不到标记"; return 1; }
  # 读原始 YAML：写进 config.yml 的项要重启才进运行态，GET /api/config 看不到。
  config="$(curl -sS --noproxy '*' --fail-with-body -b "$SMOKE_JAR" "$SMOKE_WEB_URL/api/config/raw")" ||
    { echo "GET /api/config/raw 失败"; return 1; }
  grep -qF "frigate.wonton.$marker" <<<"$config" ||
    { echo "配置里找不到标记 frigate.wonton.$marker"; return 1; }
}
