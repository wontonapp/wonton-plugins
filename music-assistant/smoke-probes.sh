# shellcheck shell=bash
# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# Music Assistant 2.10 起自带用户认证（sso: none，网关不代登录）：探针用应用自己的首启 setup 和
# builtin 登录拿 token，再走它的当前用户接口和 JSON-RPC API。冒烟实例是一次性的，这里用固定的
# 探针账号；真人的首次注册流程（浏览器向导）由安装验证人工走过，不靠冒烟。
web_assert() { grep -q "Music Assistant"; }

MA_PROBE_USER="smokeadmin"
MA_PROBE_PASS="smoke-admin-pass-1"

# 登录应用（setup 没做时先建管理员），token 存 MA_TOKEN。幂等：重装后已有用户就直接登录。
_ma_login() {
  local body resp
  [[ -n "${SMOKE_WEB_URL:-}" ]] || probes_web_url "数据写入" "数据还在"
  body=$(printf '{"provider_id":"builtin","credentials":{"username":"%s","password":"%s"}}' "$MA_PROBE_USER" "$MA_PROBE_PASS")
  resp="$(web_get /auth/login -X POST -H 'Content-Type: application/json' -d "$body" 2>&1)"
  if grep -q "Setup required" <<<"$resp"; then
    resp="$(web_get /setup -X POST -H 'Content-Type: application/json' \
      -d "$(printf '{"username":"%s","password":"%s"}' "$MA_PROBE_USER" "$MA_PROBE_PASS")" 2>&1)"
  fi
  MA_TOKEN="$(jq -r '.token // empty' <<<"$resp" 2>/dev/null)"
  [[ -n "$MA_TOKEN" ]] || { echo "应用登录失败：$(head -c 200 <<<"$resp")"; return 1; }
}

_ma_api() { # <command> [args JSON]
  local cmd="$1" args="${2:-{\}}"
  [[ -n "${SMOKE_WEB_URL:-}" ]] || probes_web_url "数据写入" "数据还在"
  web_get /api -X POST -H 'Content-Type: application/json' \
    -H "Authorization: Bearer $MA_TOKEN" \
    -d "$(jq -cn --arg c "$cmd" --argjson a "$args" '{message_id:"probe",command:$c,args:$a}')" 2>&1
}

# 身份：应用自己的当前用户接口认出的必须是 admin（MA 的账号体系独立于云吞用户，冒烟账号见上）。
identity_check() {
  _ma_login || return 1
  local me
  me="$(web_get /auth/me -H "Authorization: Bearer $MA_TOKEN" 2>&1)"
  jq -re 'select(.role == "admin" and .username == "smokeadmin")' <<<"$me" >/dev/null 2>&1 ||
    { echo "当前用户不是预期的 admin：$(head -c 200 <<<"$me")"; return 1; }
}

# 数据：建一个以随机标记命名的歌单（应用层用户数据），标记写进 SMOKE_STATE。
data_write() {
  _ma_login || return 1
  local marker resp
  marker="smoke-$(date +%s%N | tail -c 10)"
  resp="$(_ma_api music/playlists/create_playlist "{\"name\":\"$marker\"}")"
  jq -e 'has("error") | not' <<<"$resp" >/dev/null 2>&1 ||
    { echo "建歌单失败：$(head -c 200 <<<"$resp")"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

# 数据：重装后按标记找回那个歌单。
data_check() {
  _ma_login || return 1
  local marker resp
  marker="$(cat "$SMOKE_STATE")"
  resp="$(_ma_api music/playlists/library_items "{\"search\":\"$marker\"}")"
  # /api 的成功响应就是结果本身（裸数组），不带信封。
  jq -e --arg m "$marker" 'length > 0 and .[0].name == $m' <<<"$resp" >/dev/null 2>&1 ||
    { echo "找不到标记歌单 $marker：$(head -c 300 <<<"$resp")"; return 1; }
}
