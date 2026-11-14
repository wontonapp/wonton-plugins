# shellcheck shell=bash
# SillyTavern 的冒烟探针。
#
# 网页：dweb / -L 走完核心会话和应用的自动登录（唯一用户 default-user 无密码时打开即登录），
#       落到 index.html。
# 身份：/api/users/me 是应用自己的“当前用户”接口，断言认出的是 default-user 且是管理员。
# 工具：清单没有 mcp，套件按“清单没有 mcp”记 N/A。
# 数据：往应用的 settings.json 写一个随机标记（走应用自己的接口，带 CSRF 令牌），重装后读回；
#       settings.json 在插件数据目录里，重装不丢，顺带验证 seed-model 刷新网关连接后用户数据还在。

web_assert() { grep -q '<title>SillyTavern</title>'; }

identity_check() {
  dweb / -L >/dev/null || { echo "打开应用失败：自动登录没走通"; return 1; }
  local me
  me="$(web_get /api/users/me)" || { echo "调 /api/users/me 失败"; return 1; }
  jq -e '.handle == "default-user" and .admin == true' <<<"$me" >/dev/null ||
    { echo "应用没认出管理员 default-user：$(head -c 200 <<<"$me")"; return 1; }
}

# ST 的 CSRF 令牌绑定会话 Cookie（connect.sid）；web_get 只发不存 Cookie，会话建立不起来，
# POST 会被 CSRF 中间件 403。这里自己带 -b/-c 把会话回写进同一个 jar。
_curl() { curl_t "$SMOKE_WEB_MAX_TIME" "$SMOKE_WEB_URL$1" --noproxy '*' -b "$SMOKE_JAR" -c "$SMOKE_JAR" "${@:2}"; }

_csrf_token() {
  local token
  token="$(_curl /csrf-token | jq -re '.token // empty')" || return 1
  [[ -n "$token" ]] || { echo "CSRF 令牌为空"; return 1; }
  printf '%s' "$token"
}

data_write() {
  local marker token body saved
  marker="probe-$(date +%s)-$RANDOM"
  dweb / -L >/dev/null || { echo "打开应用失败：自动登录没走通"; return 1; }
  token="$(_csrf_token)" || return 1
  body="$(_curl /api/settings/get -X POST -H 'Content-Type: application/json' -H "X-CSRF-Token: $token" -d '{}')" ||
    { echo "读不到设置：$(head -c 200 <<<"$body")"; return 1; }
  saved="$(jq -r '.settings // empty' <<<"$body" | jq -c --arg m "$marker" '. + {probe_marker: $m}')" ||
    { echo "设置不是 JSON：$(head -c 200 <<<"$body")"; return 1; }
  _curl /api/settings/save -X POST -H 'Content-Type: application/json' -H "X-CSRF-Token: $token" -d "$saved" >/dev/null ||
    { echo "保存设置失败"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() {
  local marker token body got
  marker="$(cat "$SMOKE_STATE")"
  dweb / -L >/dev/null || { echo "打开应用失败：自动登录没走通"; return 1; }
  token="$(_csrf_token)" || return 1
  body="$(_curl /api/settings/get -X POST -H 'Content-Type: application/json' -H "X-CSRF-Token: $token" -d '{}')" ||
    { echo "读不到设置"; return 1; }
  got="$(jq -r '.settings // "{}"' <<<"$body" | jq -r '.probe_marker // empty')"
  [[ "$got" == "$marker" ]] || { echo "设置里找不到标记 $marker（读到：${got:-空}）"; return 1; }
}
