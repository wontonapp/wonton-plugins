# shellcheck shell=bash
# Open WebUI：sso=oidc；清单没有 mcp。

web_assert() { grep -qi 'open webui'; }   # 标准输入：应用真实页面

# 带用户会话的应用接口请求：SMOKE_JAR 里是网关会话 + 应用会话，发到应用自己的 origin（SMOKE_WEB_ORIGIN）。
# 拼路径不用 SMOKE_WEB_URL：它可能带入口路径，直接拼 /api/... 会落进应用前端的兜底路由（GET 回页面、POST 405）。
api_get() (
  set -eu
  local path="$1"; shift
  curl -sS --noproxy '*' --fail-with-body --max-time 120 -b "${SMOKE_JAR:-}" "$@" "$SMOKE_WEB_ORIGIN$path"
)

api_post() (
  set -eu
  local path="$1" body="$2"
  curl -sS --noproxy '*' --fail-with-body --max-time 120 -b "${SMOKE_JAR:-}" \
    -H 'Content-Type: application/json' -d "$body" "$SMOKE_WEB_ORIGIN$path"
)

identity_check() { # 走应用自己的 OIDC 登录端点，再调自己的“当前用户”接口：用户且是管理员
  local me
  app_login || { echo "OIDC 登录链失败"; return 1; }
  me="$(api_get /api/v1/auths/ -H 'Accept: application/json')" || { echo "/api/v1/auths/ 失败：$me"; return 1; }
  jq -e --arg u "$SMOKE_OWNER" '.role == "admin" and (.name == $u or (.email | startswith($u + "@")))' <<<"$me" >/dev/null ||
    { echo "当前用户不是用户管理员：$me"; return 1; }
}

# 登录分两层：核心会话由 dweb 写进 SMOKE_JAR（dev.sh web 每次重新走核心登录），应用自己的会话
# 要再走 /oauth/oidc/login 换——应用根路径是前端 SPA，curl 跟不到它的登录跳转。
app_login() (
  set -eu
  local landing
  dweb / >/dev/null
  landing="$(curl -sS --noproxy '*' -o /dev/null -L -b "$SMOKE_JAR" -c "$SMOKE_JAR" \
    -w '%{url_effective}' "$SMOKE_WEB_ORIGIN/oauth/oidc/login")" || { echo "OIDC 登录链失败"; return 1; }
  # 没走完会落在核心登录页（别的主机），落点不在应用 origin 下就说明会话没建立。
  case "$landing" in
    "$SMOKE_WEB_ORIGIN"*) : ;;
    *) echo "OIDC 登录链没走完，落在 $landing"; return 1 ;;
  esac
)

# 数据：对话是 Open WebUI 的用户数据，存在 /data 里，升级、重装都保留。
# data_write 登录后建一条带随机标记的对话，标记写进 SMOKE_STATE；data_check 读回标记，在对话列表里找回它。
data_write() (
  set -eu
  local mark out
  app_login || { echo "登录链失败"; return 1; }
  mark="$(date +%Y%m%d%H%M%S)-$RANDOM"
  out="$(api_post /api/v1/chats/new "{\"chat\":{\"title\":\"wonton-smoke-$mark\",\"messages\":[]}}")" ||
    { echo "建对话失败：$out"; return 1; }
  jq -e '.id' <<<"$out" >/dev/null || { echo "chats/new 返回没有 id：$out"; return 1; }
  printf '%s' "$mark" >"$SMOKE_STATE"
  echo "已建对话 wonton-smoke-$mark"
)

data_check() (
  set -eu
  local mark out
  mark="$(cat "$SMOKE_STATE" 2>/dev/null)" || mark=""
  [[ -n "$mark" ]] || { echo "没有 data_write 写下的标记"; return 1; }
  app_login || { echo "登录链失败"; return 1; }
  out="$(api_get /api/v1/chats/)" || { echo "读对话列表失败：$out"; return 1; }
  jq -e --arg t "wonton-smoke-$mark" 'any(.[]; .title == $t)' <<<"$out" >/dev/null ||
    { echo "找不到对话 wonton-smoke-$mark，看到的是：$(head -c 200 <<<"$out")"; return 1; }
  echo "对话 wonton-smoke-$mark 还在：升级/重装保住了数据"
)
