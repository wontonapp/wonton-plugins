# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
# shellcheck shell=bash disable=SC2034
#
# 变量：SMOKE_WEB_URL 是应用的网页入口；拼 API 路径用 SMOKE_WEB_ORIGIN。
# 页面：web_assert 从标准输入读页面，命中的确是应用的真实页面时返回 0。
# 身份：identity_check 用应用自己的“当前用户”接口断言认出的是用户管理员。
# 数据：data_write / data_check 成对，验证升级、重装后数据还在。
# 上游无 MCP，清单不声明 mcp：MCP=0。
# shellcheck disable=SC2034  # SSO/MCP 由冒烟套件读取
SSO=oidc
MCP=0

web_assert() { grep -qE '<title[^>]*>[Dd]onetick'; }   # 登录页的 <title>

_dt_origin() { # web_url 可能带路径和查询（清单 webui.path 是 /login），拼 API 前取 origin
  local u="${SMOKE_WEB_URL%%\?*}" rest="${SMOKE_WEB_URL#*://}"
  printf '%s://%s' "${u%%://*}" "${rest%%/*}"
}

# 走完浏览器式 OIDC：核心会话（SMOKE_JAR）先登进核心，再从应用的公开资源端点取 authorize 地址，
# 跟完 authorize → 回调拿 code，POST /api/v1/auth/oauth2/callback 换应用令牌打到标准输出。
# 只用冒烟契约里的 SMOKE_JAR/SMOKE_WEB_URL/SMOKE_BASE/SMOKE_TOKEN/SMOKE_OWNER。
dt_token() {
  local jar base_core auth_url redirect enc_redirect state final code body token_out
  jar="${SMOKE_JAR:-}"
  base_core="${SMOKE_BASE:-}"
  [[ -n "$jar" && -n "$base_core" && -n "${SMOKE_WEB_URL:-}" && -n "${SMOKE_TOKEN:-}" ]] ||
    { echo "缺冒烟变量：需要 SMOKE_JAR/SMOKE_BASE/SMOKE_TOKEN/SMOKE_WEB_URL" >&2; return 1; }
  # 核心会话补进 jar：-b 保留已有 cookie（只写 -c 会把 jar 里别的域的会话抹掉）
  curl -sS --noproxy '*' -o /dev/null -b "$jar" -c "$jar" --data-urlencode "token=${SMOKE_TOKEN}" \
    "$base_core/_wonton/login" || { echo "主人令牌登录不了核心" >&2; return 1; }
  # 资源端点免登录，回 identity_provider.auth_url/client_id/scopes
  local res
  res="$(curl -sS --noproxy '*' --fail-with-body -b "$jar" "$(_dt_origin)/api/v1/resource")" ||
    { echo "取 /api/v1/resource 失败：$res" >&2; return 1; }
  # 清单关掉的入口还在关：注册、密码登录（SSO-only），上游更新悄悄放开就在这里红
  jq -e '.is_user_creation_disabled == true and .disable_password_auth == true' <<<"$res" >/dev/null ||
    { echo "注册或密码登录被打开了：$(jq -c '{is_user_creation_disabled,disable_password_auth}' <<<"$res")" >&2; return 1; }
  auth_url="$(jq -r '.identity_provider.auth_url // empty' <<<"$res")"
  [[ -n "$auth_url" && "$auth_url" != null ]] || { echo "资源端点没有 OIDC auth_url：$res" >&2; return 1; }
  redirect="$(_dt_origin)/auth/oauth2"   # 前端固定的回调页（LoginView.jsx:443）
  enc_redirect="$(python3 -c 'import sys,urllib.parse;print(urllib.parse.quote(sys.argv[1],safe=""))' "$redirect")"
  state="$(head -c8 /dev/urandom | od -An -tx1 | tr -d ' \n')"   # authorize 要求 state ≥ 8 字符
  final="$(curl -sS --noproxy '*' -o /dev/null -L -b "$jar" -c "$jar" -H 'Accept: text/html' \
    -w '%{url_effective}' \
    "$auth_url?client_id=com.donetick.donetick&redirect_uri=$enc_redirect&response_type=code&scope=openid+profile+email+groups&state=$state")" ||
    { echo "authorize 链失败" >&2; return 1; }
  code="$(python3 -c 'import sys,urllib.parse;print((urllib.parse.parse_qs(urllib.parse.urlparse(sys.argv[1]).query).get("code") or [""])[0])' "$final")"
  [[ -n "$code" ]] || { echo "回调里没有 code：$final" >&2; return 1; }
  # 后端 binds {code, verifier, state, redirect_uri}；PKCE 没开，verifier 传空
  body="$(jq -nc --arg c "$code" --arg r "$redirect" '{code:$c,verifier:"",state:"",redirect_uri:$r}')"
  token_out="$(curl -sS --noproxy '*' -b "$jar" -H 'Content-Type: application/json' -d "$body" \
    "$(_dt_origin)/api/v1/auth/oauth2/callback")" || { echo "callback 换令牌失败：$token_out" >&2; return 1; }
  token_out="$(jq -r '.token // empty' <<<"$token_out")"
  [[ -n "$token_out" ]] || { echo "callback 没有返回令牌" >&2; return 1; }
  printf '%s' "$token_out"
}

identity_check() {
  local token me
  token="$(dt_token)" || return 1
  me="$(curl -sS --noproxy '*' --fail-with-body -b "${SMOKE_JAR:-}" -H "Authorization: Bearer $token" \
    "$(_dt_origin)/api/v1/users/profile")" || { echo "/api/v1/users/profile 失败：$me"; return 1; }
  jq -e --arg u "$SMOKE_OWNER" '.res.username == $u' <<<"$me" >/dev/null ||
    { echo "当前用户不是 $SMOKE_OWNER：$(jq -c '.res | {id,username,email}' <<<"$me" 2>/dev/null || head -c 200 <<<"$me")"; return 1; }
}

# 保数据：建一个名字带随机标记的一次性任务，标记和 id 存进 $SMOKE_STATE；升级/重装后读回核对。
marker_file() { # SMOKE_STATE 在套件里可能是状态目录（e2e 传的是标记文件本身），标记放对应路径
  if [[ -d "${SMOKE_STATE:-}" ]]; then printf '%s' "$SMOKE_STATE/donetick-marker"; else printf '%s' "${SMOKE_STATE}.marker"; fi
}

data_write() {
  local token marker chore chore_id
  [[ -n "${SMOKE_JAR:-}" && -n "${SMOKE_WEB_URL:-}" ]] ||
    { echo "缺冒烟变量：需要 SMOKE_JAR/SMOKE_WEB_URL" >&2; return 1; }
  token="$(dt_token)" || { echo "拿不到应用令牌，写不了数据" >&2; return 1; }
  marker="保数据-$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
  # ChoreReq 必填 name、frequencyType、assignStrategy（internal/chore/handler.go ChoreReq）
  chore="$(curl -sS --noproxy '*' --fail-with-body -b "$SMOKE_JAR" -X POST \
    -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d "{\"name\":\"$marker\",\"frequencyType\":\"once\",\"assignStrategy\":\"no_assignee\",\"notification\":false}" \
    "$(_dt_origin)/api/v1/chores/")" || { echo "建任务失败：$chore"; return 1; }
  chore_id="$(jq -r '.res // empty' <<<"$chore")"
  [[ -n "$chore_id" ]] || { echo "建任务没回 id：$chore"; return 1; }
  jq -nc --arg m "$marker" --arg c "$chore_id" '{marker:$m,chore_id:$c}' >"$(marker_file)"
}

data_check() {
  local token marker chore_id seen
  marker="$(jq -r .marker "$(marker_file)")"
  chore_id="$(jq -r .chore_id "$(marker_file)")"
  token="$(dt_token)" || { echo "拿不到应用令牌，查不了数据" >&2; return 1; }
  seen="$(curl -sS --noproxy '*' -b "$SMOKE_JAR" -H "Authorization: Bearer $token" \
    "$(_dt_origin)/api/v1/chores/$chore_id")" ||
    { echo "任务 $chore_id 读不到（数据丢了？）：$seen"; return 1; }
  [[ "$(jq -r '.res.name // empty' <<<"$seen")" == "$marker" ]] ||
    { echo "任务对不上：期待名字 $marker（id $chore_id），看到 $(jq -c '.res | {id,name}' <<<"$seen" 2>/dev/null || head -c 200 <<<"$seen")"; return 1; }
}
