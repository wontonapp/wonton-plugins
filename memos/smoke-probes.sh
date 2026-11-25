# shellcheck shell=bash disable=SC2034
# memos：sso=oidc（应用自己接核心的 OIDC），MCP 经 nginx 侧车带着主人账号的令牌。
# 页面是 SPA，登录跳转在 JS 里，探针不执行 JS：核心会话建立后直接走 authorize 链换 code，
# 再用应用自己的 POST /api/auth/signin 换成应用会话。每个钩子都要令牌，各自走一遍这条链。
# 数据：经应用自己的 REST API 写一条便签（内容带随机标记），data_check 按内容过滤读回。

_memos_core_session() { # 核心会话就绪（已建立时原地返回）
  grep -q 'wonton' "$SMOKE_JAR" 2>/dev/null && return 0
  curl -sS --noproxy '*' --max-time "$SMOKE_WEB_MAX_TIME" -o /dev/null -c "$SMOKE_JAR" \
    --data-urlencode "token=$SMOKE_TOKEN" "$SMOKE_BASE/_wonton/login" || { echo "主人令牌登录不了核心"; return 1; }
}

# 走完应用自己的 OIDC 链（Hydra authorize 走核心会话自动登录 → 应用回调拿 code → signin），打印访问令牌。
_memos_token() {
  local idps auth_url redirect enc_redirect state final code resp at body
  _memos_core_session || return 1
  idps="$(web_get /api/identity-providers)" || { echo "取 identity providers 失败：$idps"; return 1; }
  auth_url="$(jq -r '.identityProviders[0].config.oauth2Config.authUrl // empty' <<<"$idps")"
  [[ -n "$auth_url" ]] || { echo "没有 authUrl：$idps"; return 1; }
  redirect="$SMOKE_WEB_URL/auth/callback"
  enc_redirect="$(jq -rn --arg r "$redirect" '$r | @uri')"
  # Hydra 要求 state 至少 8 个字符
  state="$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
  final="$(curl -sS --noproxy '*' --max-time "$SMOKE_WEB_MAX_TIME" -o /dev/null -L -b "$SMOKE_JAR" -c "$SMOKE_JAR" \
    -H 'Accept: text/html' -w '%{url_effective}' \
    "$auth_url?client_id=$SMOKE_PLUGIN_ID&redirect_uri=$enc_redirect&response_type=code&scope=openid+profile+email&state=$state")" ||
    { echo "authorize 链失败"; return 1; }
  code="$(jq -rn --arg u "$final" '($u | split("?")[1] // "") | split("&") | map(split("=")) |
    map(select(.[0] == "code")) | .[0][1] // empty' | jq -rR '@uri | .' | xargs -r printf '%b')"
  [[ -n "$code" ]] || { echo "回调里没有 code：$final"; return 1; }
  body="$(jq -nc --arg c "$code" --arg r "$redirect" \
    '{ssoCredentials:{identityProvider:"identity-providers/wonton",code:$c,redirectUri:$r,codeVerifier:""}}')"
  resp="$(curl -sS --noproxy '*' --max-time "$SMOKE_WEB_MAX_TIME" -b "$SMOKE_JAR" -c "$SMOKE_JAR" \
    -H 'Content-Type: application/json' --data-raw "$body" "$SMOKE_WEB_URL/api/auth/signin")" ||
    { echo "signin 失败：$resp"; return 1; }
  at="$(jq -r '.accessToken // empty' <<<"$resp")"
  [[ -n "$at" ]] || { echo "signin 没有 accessToken：$resp"; return 1; }
  code="$(python3 -c 'import sys,urllib.parse;print((urllib.parse.parse_qs(urllib.parse.urlparse(sys.argv[1]).query).get("code") or [""])[0])' "$final")"
  printf '%s\n' "$at"
}

web_assert() { grep -qi '<title>memos</title>'; }   # 应用真实页面的标题

identity_check() { # 应用自己的“当前用户”接口认出的是用户管理员
  local at me
  at="$(_memos_token)" || return 1
  me="$(web_get /api/auth/me -H "Authorization: Bearer $at")" || { echo "/api/auth/me 失败：$me"; return 1; }
  jq -e --arg u "$SMOKE_OWNER" '.user.username == $u and .user.role == "ADMIN"' <<<"$me" >/dev/null ||
    { echo "/api/auth/me 不是 $SMOKE_OWNER 的管理员：$(head -c 200 <<<"$me")"; return 1; }
}

MCP_TOOL=auth_get_current_user

mcp_assert() { # 只读的当前用户工具，返回的同样是用户管理员。
  # 调用的标准输出前面可能混着套件的进度行（确认卡片那行），从第一行 JSON 开始断言。
  local json
  json="$(grep -m1 '^{' <<<"$(cat)")"
  [[ -n "$json" ]] || { echo "调用结果里没有 JSON"; return 1; }
  jq -e --arg u "$SMOKE_OWNER" \
    '.content[0].text | fromjson | .user | (.username == $u and .role == "ADMIN")' <<<"$json" >/dev/null ||
    { echo "工具认出的不是 $SMOKE_OWNER 的管理员：$(head -c 200 <<<"$json")"; return 1; }
}

data_write() { # 写一条带随机标记的便签，标记存进 $SMOKE_STATE
  local at marker resp
  at="$(_memos_token)" || return 1
  marker="wonton-smoke-$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
  resp="$(web_get /api/memos -X POST -H "Authorization: Bearer $at" -H 'Content-Type: application/json' \
    --data-raw "{\"content\":\"$marker\"}")" || { echo "建便签失败：$(head -c 200 <<<"$resp")"; return 1; }
  jq -e --arg m "$marker" '.content == $m' <<<"$resp" >/dev/null ||
    { echo "便签没建成：$(head -c 200 <<<"$resp")"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() { # 按标记过滤便签，读回去
  local at marker resp
  marker="$(cat "$SMOKE_STATE" 2>/dev/null)" || true
  [[ -n "$marker" ]] || { echo "SMOKE_STATE 是空的（data_write 没跑过？）"; return 1; }
  at="$(_memos_token)" || return 1
  resp="$(web_get /api/memos -H "Authorization: Bearer $at" --get \
    --data-urlencode "filter=content.contains(\"$marker\")")" ||
    { echo "列便签失败：$(head -c 200 <<<"$resp")"; return 1; }
  jq -e --arg m "$marker" '[.memos[]?.content] | index($m) != null' <<<"$resp" >/dev/null ||
    { echo "标记便签不见了，看到的是：$(head -c 300 <<<"$resp")"; return 1; }
}
