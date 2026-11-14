# shellcheck shell=bash
# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# Homebridge：sso=none。登录是 config-ui-x 自己的：首次打开是上游设置向导，建了管理员才有得登录。
# 探针走上游同一条向导接口（GET /api/setup-wizard/get-setup-wizard-token → POST /create-first-user）
# 在冒烟实例里建管理员（一次性实例，用后即弃），再用应用自己的 /api/auth/login 拿令牌断言身份；
# 数据断言经 /api/config-editor 写读 config.json。请求一律经核心网关（curl 不带 Origin，
# 网关按 Fetch Metadata 放行）。清单没有 mcp。
export SSO=none
export MCP=0

web_assert() { grep -qi '<title>Homebridge</title>'; }

# SMOKE_STATE 按套件契约是数据标记文件（data_write 覆盖写、data_check 读）；管理员凭据是另一份
# 状态，放在它旁边的文件里，不混进标记（e2e 传的是文件，dev.sh probes 可能传目录）。
creds_file() {
  if [[ -d "$SMOKE_STATE" ]]; then
    printf '%s/homebridge-admin.json' "$SMOKE_STATE"
  else
    printf '%s.homebridge-admin' "$SMOKE_STATE"
  fi
}

uix_token() { # <用户名> <密码>：应用自己的登录，输出 access_token
  local body
  body="$(web_get /api/auth/login -H 'Content-Type: application/json' \
    --data "$(jq -nc --arg u "$1" --arg p "$2" '{username:$u,"password":$p}')" --fail-with-body)" ||
    { echo "登录失败：${body:-（无响应）}"; return 1; }
  jq -er '.access_token // empty' <<<"$body" || { echo "登录响应里没有令牌：$(head -c 200 <<<"$body")"; return 1; }
}

# 建管理员（向导未完成时走上游向导接口，用户名用冒烟的主人名），输出 {user,password} 的 JSON。
uix_create_admin() {
  local body wizard token pw
  body="$(web_get /api/auth/settings --fail-with-body)" ||
    { echo "读 /api/auth/settings 失败：${body:-（无响应）}"; return 1; }
  if jq -e '.env.setupWizardComplete == false' <<<"$body" >/dev/null; then
    wizard="$(web_get /api/setup-wizard/get-setup-wizard-token --fail-with-body)" ||
      { echo "取向导令牌失败：${wizard:-（无响应）}"; return 1; }
    token="$(jq -er '.access_token // empty' <<<"$wizard")" ||
      { echo "向导令牌响应不对：$(head -c 200 <<<"$wizard")"; return 1; }
    pw="$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
    body="$(web_get /api/setup-wizard/create-first-user -X POST -H 'Content-Type: application/json' \
      -H "Authorization: Bearer $token" \
      --data "$(jq -nc --arg u "$SMOKE_OWNER" --arg p "$pw" '{username:$u,"password":$p,name:$u}')" \
      --fail-with-body)" ||
      { echo "建管理员失败：${body:-（无响应）}"; return 1; }
    jq -nc --arg u "$SMOKE_OWNER" --arg p "$pw" '{user:$u,"password":$p}'
  else
    echo "向导已完成但没有本地凭据（换实例跑了冒烟？）：删掉 $(creds_file) 重跑" >&2
    return 1
  fi
}

uix_session() { # 凭据文件里没有就建一个并存下；输出 access_token
  local f creds user pw token
  f="$(creds_file)"
  if [[ -s "$f" ]]; then
    creds="$(cat "$f")"
  else
    creds="$(uix_create_admin)" || return 1
    printf '%s' "$creds" >"$f" || { echo "凭据写不进 $f"; return 1; }
  fi
  user="$(jq -er '.user // empty' <<<"$creds")" || { echo "凭据文件 $f 不对"; return 1; }
  pw="$(jq -er '."password" // empty' <<<"$creds")" || { echo "凭据文件 $f 不对"; return 1; }
  if token="$(uix_token "$user" "$pw")"; then
    printf '%s' "$token"
    return 0
  fi
  # 登录不上＝凭据是别的实例留下的（冒烟套件的工作区跨次保留，插件数据却是新卷）：
  # 向导还没完成就按它重建管理员再试一次；向导已完成则如实报错，不猜密码。
  echo "存的凭据登录不上，重走向导重建管理员" >&2
  rm -f "$f"
  creds="$(uix_create_admin)" || return 1
  printf '%s' "$creds" >"$f" || { echo "凭据写不进 $f"; return 1; }
  user="$(jq -er '.user // empty' <<<"$creds")" || { echo "凭据文件 $f 不对"; return 1; }
  pw="$(jq -er '."password" // empty' <<<"$creds")" || { echo "凭据文件 $f 不对"; return 1; }
  uix_token "$user" "$pw"
}

uix_web_ready() { # 单项跑（dev.sh probes data_check）时 SMOKE_WEB_URL 没人设，自己解析一次
  [[ -n "${SMOKE_WEB_URL:-}" ]] || probes_web_url "数据" || return 1
}

identity_check() { # 应用自己的登录认出管理员：登录 + GET /api/users（AdminGuard）里有这个管理员
  local token users
  uix_web_ready || return 1
  token="$(uix_session)" || return 1
  users="$(web_get /api/users -H "Authorization: Bearer $token" --fail-with-body)" ||
    { echo "GET /api/users 失败：${users:-（无响应）}"; return 1; }
  jq -e --arg u "$SMOKE_OWNER" \
    'type == "array" and any(.[]; .username == $u and .admin == true)' <<<"$users" >/dev/null ||
    { echo "用户列表里没有管理员 $SMOKE_OWNER：$(head -c 300 <<<"$users")"; return 1; }
}

data_write() { # 在 config.json 的 UI 块里写一条带随机标记的字段（重装/升级后由 data_check 验证）
  local token config new marker
  [[ -n "${SMOKE_STATE:-}" ]] || { echo "SMOKE_STATE 没设"; return 1; }
  uix_web_ready || return 1
  token="$(uix_session)" || return 1
  marker="wonton$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
  config="$(web_get /api/config-editor -H "Authorization: Bearer $token" --fail-with-body)" ||
    { echo "读 config.json 失败：${config:-（无响应）}"; return 1; }
  new="$(jq -c --arg m "$marker" '.platforms[0].wontonMarker = $m' <<<"$config")" ||
    { echo "config.json 不是合法 JSON：$(head -c 200 <<<"$config")"; return 1; }
  config="$(web_get /api/config-editor -X POST -H 'Content-Type: application/json' \
    -H "Authorization: Bearer $token" --data "$new" --fail-with-body)" ||
    { echo "写 config.json 失败：${config:-（无响应）}"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE" || { echo "标记写不进 $SMOKE_STATE"; return 1; }
  echo "已写数据标记：config.json platforms[0].wontonMarker=$marker"
}

data_check() { # config.json 里还有那条标记（重装/升级没丢）
  local token config marker
  [[ -n "${SMOKE_STATE:-}" ]] || { echo "SMOKE_STATE 没设"; return 1; }
  uix_web_ready || return 1
  marker="$(cat "$SMOKE_STATE" 2>/dev/null)" || { echo "读不到 $SMOKE_STATE（data_write 没跑过？）"; return 1; }
  [[ -n "$marker" ]] || { echo "SMOKE_STATE 里没有标记"; return 1; }
  token="$(uix_session)" || return 1
  config="$(web_get /api/config-editor -H "Authorization: Bearer $token" --fail-with-body)" ||
    { echo "读 config.json 失败：${config:-（无响应）}"; return 1; }
  jq -e --arg m "$marker" '.platforms[0].wontonMarker == $m' <<<"$config" >/dev/null ||
    { echo "重装/升级后 config.json 里找不到标记 $marker：$(head -c 300 <<<"$config")"; return 1; }
}
