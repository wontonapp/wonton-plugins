# shellcheck shell=bash
# 冒烟探针，契约按真实 e2e 套件（e2e/smoke.sh）：探针只有 SMOKE_STATE、SMOKE_JAR、SMOKE_WEB_URL、
# SMOKE_OWNER、SMOKE_BASE、SMOKE_TOKEN、SMOKE_INSTANCE 和 web_get、dweb，在 set -u 下跑——
# 引用其他套件变量或函数就是 unbound/not found，会整项红。要源地址从 SMOKE_WEB_URL 切；
# 容器一律按 wonton.instance / wonton.plugin / wonton.service 标签找，不拼容器名。
# 本插件套自己的常量放在 _ 开头的变量里，不占套件命名。

_PROBE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_PLUGIN_ID=io.esphome.esphome
_TOOL_PREFIX=io-esphome-esphome__ # 总线工具名前缀：插件 id 的点换成 -
_HERMES_ID=com.nousresearch.hermes

# SMOKE_WEB_URL 的 scheme://host:port（剥掉入口路径）
_origin() { printf %s "$SMOKE_WEB_URL" | sed -E 's#^(https?://[^/]+).*$#\1#'; }

# <插件id> <服务名>：本实例里这个服务的容器；实例名标签去连字符（核心按归一化名打标），
# 实例名没导出时按 SMOKE_BASE 的主机名第一段推（插件自带 smoke 用）。
_container() {
  local inst="${SMOKE_INSTANCE:-}" args=(--filter "label=wonton.plugin=$1" --filter "label=wonton.service=$2")
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

web_assert() { grep -qE '<esphome-app|ESPHome Device Builder'; }

identity_check() {
  # 上游是单管理员 Basic 登录：没有"当前用户"接口，也不回显 Wonton 用户，回显本就不存在。
  # 如实证明两层边界：1) 只带网关会话打应用的 MCP 接口，应用自己的登录边界挡住（401）；
  # 2) 再带主人的管理员凭据（网页密码，主人可在插件详情页查看）就放行（200）。
  local pw code body
  body='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"smoke","version":"1"}}}'
  code="$(curl -sS --max-time 30 --noproxy '*' -b "$SMOKE_JAR" -o /dev/null -w '%{http_code}' \
    -H 'Content-Type: application/json' -d "$body" "$(_origin)/api/mcp")" ||
    { echo "只带网关会话的请求失败"; return 1; }
  [[ "$code" == 401 ]] || { echo "只带网关会话没被应用的登录边界挡住：HTTP $code"; return 1; }
  pw="$(curl -sS --max-time 30 --noproxy '*' -H "Authorization: Bearer $SMOKE_TOKEN" \
    "$SMOKE_BASE/_wonton/api/plugins/$_PLUGIN_ID/secrets/dashboard_password" | jq -r '.value // empty')" ||
    { echo "取不到网页密码（核心的主人密钥接口）"; return 1; }
  [[ -n "$pw" ]] || { echo "网页密码取出来是空"; return 1; }
  code="$(curl -sS --max-time 30 --noproxy '*' -b "$SMOKE_JAR" -o /dev/null -w '%{http_code}' \
    -u "wonton:$pw" -H 'Content-Type: application/json' -d "$body" "$(_origin)/api/mcp")" ||
    { echo "带应用凭据的请求失败"; return 1; }
  [[ "$code" == 200 ]] || { echo "应用没认主人的管理员凭据：HTTP $code"; return 1; }
}

data_write() {
  # 一个最小设备 YAML 写进 /config，标记在设备名和文件名里，标记存 $SMOKE_STATE。
  local marker c name file
  marker="$(tr -dc 'a-f0-9' </dev/urandom | head -c 6)"
  [[ -n "$marker" ]] || { echo "生成不了标记"; return 1; }
  c="$(_container "$_PLUGIN_ID" esphome)"
  [[ -n "$c" ]] || { echo "找不到 esphome 容器"; return 1; }
  name="smoke-marker-$marker"
  file="$name.yaml"
  docker exec -i "$c" sh -c "cat > /config/$file" <<EOF || { echo "写不进 /config/$file"; return 1; }
esphome:
  name: $name
  friendly_name: SMOKE-MARKER $marker
EOF
  printf %s "$marker" >"$SMOKE_STATE" || { echo "写不进 SMOKE_STATE"; return 1; }
}

data_check() {
  # 升级/重装后标记文件还在、内容对得上。
  local marker c file out
  marker="$(cat "$SMOKE_STATE" 2>/dev/null)"
  [[ -n "$marker" ]] || { echo "SMOKE_STATE 里没有标记"; return 1; }
  file="smoke-marker-$marker.yaml"
  c="$(_container "$_PLUGIN_ID" esphome)"
  [[ -n "$c" ]] || { echo "找不到 esphome 容器"; return 1; }
  out="$(docker exec "$c" cat "/config/$file" 2>&1)" || { echo "标记文件不在了：$out"; return 1; }
  grep -qF "$marker" <<<"$out" || { echo "文件在但内容对不上：$(head -c 200 <<<"$out")"; return 1; }
}

mcp_check() {
  # 探针顺序在 data_write 之前（装→网页→身份→工具→数据），不能依赖 data_write 留的标记：
  # 自己写一个标记设备 YAML 进 /config，经总线 get_config 读回来断言，读完删掉。
  # 上游的 MCP 工具没标 readOnlyHint，总线对它们一律先出确认卡：探针用主人令牌回答，
  # 等价主人在 App 里点允许。
  local marker c file gw url out rc n=0 card payload cid reply
  marker="$(tr -dc 'a-f0-9' </dev/urandom | head -c 6)"
  [[ -n "$marker" ]] || { echo "生成不了标记"; return 1; }
  c="$(_container "$_PLUGIN_ID" esphome)"
  [[ -n "$c" ]] || { echo "找不到 esphome 容器"; return 1; }
  file="smoke-mcp-marker-$marker.yaml"
  docker exec -i "$c" sh -c "cat > /config/$file" <<EOF || { echo "写不进 /config/$file"; return 1; }
esphome:
  name: smoke-mcp-marker-$marker
EOF
  gw="$(_container "$_HERMES_ID" gateway)"
  [[ -n "$gw" ]] || { echo "找不到 Hermes gateway 容器"; return 1; }
  url="$(docker exec "$gw" printenv WONTON_MCP_URL 2>/dev/null)"
  [[ -n "$url" ]] || { echo "gateway 容器里没有 WONTON_MCP_URL"; return 1; }
  timeout 180 docker exec -i "$gw" python3 - call "$url" "${_TOOL_PREFIX}get_config" \
    "{\"configuration\":\"$file\"}" <"$_PROBE_DIR/../scripts/mcp_client.py" \
    >"$(dirname "$SMOKE_STATE")/.esphome-mcp.out" 2>&1 &
  local pid=$!
  while kill -0 "$pid" 2>/dev/null && ((n < 30)); do
    card="$(curl -sS --max-time 15 --noproxy '*' -H "Authorization: Bearer $SMOKE_TOKEN" \
      "$SMOKE_BASE/_wonton/api/confirmations" 2>/dev/null)" || { sleep 2; n=$((n + 1)); continue; }
    payload="$(CARD_JSON="$card" python3 - <<'PY' 2>/dev/null
import json, os
ALLOW = {"approve", "approve_wider", "allow_window"}
for card in json.loads(os.environ["CARD_JSON"]).get("confirmations", []):
    choices = card.get("choices", [])
    chosen = next((c for c in choices if c.get("choice_id") and c.get("answer") in ALLOW), None)
    if chosen is not None:
        print(card["id"], json.dumps({"answer": chosen["answer"], "choice_id": chosen["choice_id"]}), sep="\t")
    elif not any(c.get("choice_id") for c in choices):
        print(card["id"], '{"answer":"approve"}', sep="\t")
    else:
        continue
    break
PY
)" || { sleep 2; n=$((n + 1)); continue; }
    [[ -n "$payload" ]] || { sleep 2; n=$((n + 1)); continue; }
    cid="${payload%%$'\t'*}"; reply="${payload#*$'\t'}"
    curl -sS --max-time 15 --noproxy '*' -o /dev/null -X POST \
      -H "Authorization: Bearer $SMOKE_TOKEN" -H 'Content-Type: application/json' -d "$reply" \
      "$SMOKE_BASE/_wonton/confirmations/$cid" || { sleep 2; n=$((n + 1)); continue; }
    break
  done
  wait "$pid" && rc=0 || rc=$?
  docker exec "$c" rm -f "/config/$file" >/dev/null 2>&1 || true
  out="$(cat "$(dirname "$SMOKE_STATE")/.esphome-mcp.out" 2>/dev/null)"
  rm -f "$(dirname "$SMOKE_STATE")/.esphome-mcp.out"
  [[ "$rc" == 0 ]] || { echo "工具调用失败（rc=$rc）：$(head -c 200 <<<"$out")"; return 1; }
  grep -qF "$marker" <<<"$out" || { echo "工具结果里没有写入的标记，开头：$(head -c 200 <<<"$out")"; return 1; }
}
