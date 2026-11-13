# shellcheck shell=bash
# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# Node-RED 是 sso: none + 自带 adminAuth 账号登录：
#   网关页面（编辑器 HTML）不用应用登录就能打开，web_assert 直接认页面；
#   identity_check / data_write / data_check 走应用自己的 /auth/token 换令牌再调 Admin API。
#   密码不进探针进程的环境：从插件容器里 printenv 取（探针跑在宿主上，经网关请求应用）。
#
# 变量：SMOKE_WEB_URL 是应用的网页入口（套件已去尾斜杠）；SMOKE_WEB_ORIGIN 只有 scheme://host:port。
#       SMOKE_JAR 带用户会话；SMOKE_STATE 是数据标记文件；SMOKE_INSTANCE 供按实例过滤容器。
#       钩子拿不到插件 id，探针里写死本插件的 id。
# 工具：本插件不提供 MCP 工具（上游没有、也不做功能型包装），清单没有 mcp，套件自动记 N/A。

_PLUGIN_ID=org.nodered.node-red

_container() { # <服务名>：按核心打的标签找本插件这个服务的容器
  local inst="${SMOKE_INSTANCE:-}" args=(--filter "label=wonton.plugin=$_PLUGIN_ID" --filter "label=wonton.service=$1")
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

_login_token() { # 经网关登录 admin，输出访问令牌；失败输出错误
  local c pw body origin
  # 单跑 data_check 时套件还没备好网页地址：先取一次
  if [[ -z "${SMOKE_WEB_ORIGIN:-}" ]]; then
    probes_web_url || { echo "拿不到插件网页地址"; return 1; }
    origin="$SMOKE_WEB_ORIGIN"
  else
    origin="$SMOKE_WEB_ORIGIN"
  fi
  c="$(_container nodered)" || { echo "找不到 nodered 容器"; return 1; }
  pw="$(docker exec "$c" printenv WONTON_SECRET_ADMIN_PASSWORD)" || { echo "容器里没有 WONTON_SECRET_ADMIN_PASSWORD"; return 1; }
  body="$(curl -fsS -m 15 -b "$SMOKE_JAR" -X POST \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data-urlencode 'grant_type=password' \
    --data-urlencode 'client_id=node-red-editor' \
    --data-urlencode 'scope=*' \
    --data-urlencode 'username=admin' \
    --data-urlencode "password=$pw" \
    "$origin/auth/token")" || { echo "登录失败：$body"; return 1; }
  python3 -c 'import json,sys; print(json.load(sys.stdin)["access_token"])' <<<"$body" || { echo "没有 access_token：$body"; return 1; }
}

web_assert() { grep -q '<title>Node-RED</title>'; }

identity_check() { # 应用自己的登录换出令牌；/settings 回当前用户，断言认出的是 admin、全权限
  local token settings
  # $(_login_token) 在子壳里跑，套件的 export 带不回来：网页地址在本壳先备好
  [[ -n "${SMOKE_WEB_ORIGIN:-}" ]] || probes_web_url || { echo "拿不到插件网页地址"; return 1; }
  token="$(_login_token)" || { echo "$token"; return 1; }
  settings="$(curl -fsS -m 15 -b "$SMOKE_JAR" -H "Authorization: Bearer $token" "$SMOKE_WEB_ORIGIN/settings")" \
    || { echo "带令牌读 /settings 失败"; return 1; }
  grep -q '"username":"admin"' <<<"$settings" || { echo "/settings 没认出 admin：$(head -c 120 <<<"$settings")"; return 1; }
  grep -q '"permissions":"\*"' <<<"$settings" || { echo "/settings 权限不是 *：$(head -c 120 <<<"$settings")"; return 1; }
}

data_write() { # 建一个以随机标记命名的流程页（tab），这是 Node-RED 里用户数据的基本单位
  local token marker flow
  [[ -n "${SMOKE_WEB_ORIGIN:-}" ]] || probes_web_url || { echo "拿不到插件网页地址"; return 1; }
  token="$(_login_token)" || { echo "$token"; return 1; }
  marker="smoke-$(head -c 4 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  flow="$(printf '{"id":"%s","type":"tab","label":"%s","nodes":[]}' "$marker" "$marker")"
  curl -fsS -m 15 -b "$SMOKE_JAR" -X POST -H "Authorization: Bearer $token" \
    -H 'Content-Type: application/json' -d "$flow" "$SMOKE_WEB_ORIGIN/flow" >/dev/null \
    || { echo "POST /flow 失败"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() { # 读回流程列表，找当初写的那个标记
  local token marker flows
  [[ -n "${SMOKE_WEB_ORIGIN:-}" ]] || probes_web_url || { echo "拿不到插件网页地址"; return 1; }
  marker="$(cat "$SMOKE_STATE")" || { echo "标记文件读不出来"; return 1; }
  token="$(_login_token)" || { echo "$token"; return 1; }
  flows="$(curl -fsS -m 15 -b "$SMOKE_JAR" -H "Authorization: Bearer $token" "$SMOKE_WEB_ORIGIN/flows")" \
    || { echo "GET /flows 失败"; return 1; }
  grep -qF "$marker" <<<"$flows" || { echo "流程列表里没有 $marker：$(head -c 200 <<<"$flows")"; return 1; }
}
