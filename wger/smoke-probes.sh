# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
# shellcheck shell=bash disable=SC2034
#
# 变量别叫 state：shellcheck 把这份文件连着 probes.sh 一起分析，probes.sh 自己也读一个 dev.sh 提供的 state；
# 子壳体函数（fn() ( … )）里给 state 赋值，SC2031 就报在 probes.sh 上，CI 红。
#
# wger：sso=proxy（网关注入 Remote-User/Remote-Email/Remote-Name，wger 在登录页按头登录、
# 没有的用户自动建号）。清单没有 mcp。探针只用冒烟契约里的变量：
# SMOKE_JAR / SMOKE_WEB_URL / SMOKE_BASE / SMOKE_TOKEN / SMOKE_OWNER / SMOKE_STATE；
# 两套冒烟套件（dev.sh smoke 和 e2e harness）都这么给。网页入口可能带路径，拼 API 前
# 用 _wg_origin 现场取 origin，不依赖 SMOKE_WEB_ORIGIN（harness 没有这个变量，set -u 会挂）。
# 代登录下应用没有自己的登录页可走：核心会话 Cookie 就是全部。数据钩子可能跑在第一次
# 会话建立之前，每个钩子都先 _wg_session 建一次会话（幂等）。
# 数据走 wger 自己的 REST API（DRF 会话认证）：写操作要 CSRF，先取浏览版 API 页面
# 让 Django 发 csrftoken cookie，POST 时带上 X-CSRFToken。
SSO=proxy
MCP=0

_wg_origin() { # 网页入口可能带路径和查询（清单 webui.path），拼 API 前取 origin
  local u="${SMOKE_WEB_URL%%\?*}" rest="${SMOKE_WEB_URL#*://}"
  printf '%s://%s' "${u%%://*}" "${rest%%/*}"
}

_wg_session() { # jar 里备好核心会话和 wger 的登录态；已建立时开销是两次快速请求
  [[ -n "${SMOKE_WEB_URL:-}" && -n "${SMOKE_BASE:-}" ]] ||
    { echo "缺冒烟变量：需要 SMOKE_WEB_URL/SMOKE_BASE"; return 1; }
  # 核心会话：-b 保留 jar 里已有 cookie（只写 -c 会把 jar 里别的域的会话抹掉）
  if ! grep -q 'wonton' "${SMOKE_JAR:-/dev/null}" 2>/dev/null; then
    curl -sS --noproxy '*' --max-time 60 -o /dev/null -b "${SMOKE_JAR:-}" -c "${SMOKE_JAR:-}" \
      --data-urlencode "token=${SMOKE_TOKEN:-}" "${SMOKE_BASE%/}/_wonton/login" ||
      { echo "主人令牌登录不了核心"; return 1; }
  fi
  # 走进插件：wger 的代登录中间件只在登录页上按头登录（首页是公开落地页），
  # 显式走登录页。冒烟合同可能给带路径的入口（dev.sh）也可能剥成 origin
  #（e2e harness），两种都归一成 <origin>/user/login。
  # wger 的会话和 csrftoken 都落 jar；API 拼接仍用 origin。
  local entry="${SMOKE_WEB_URL%/}"
  entry="${entry%/user/login}/user/login"
  if ! grep -q 'sessionid' "${SMOKE_JAR:-/dev/null}" 2>/dev/null; then
    curl -sS --noproxy '*' --max-time 120 -o /dev/null -L -b "${SMOKE_JAR:-}" -c "${SMOKE_JAR:-}" \
      -H 'Accept: text/html' "$entry" ||
      { echo "插件网页登录跳转失败（$entry）"; return 1; }
  fi
}

web_assert() { # 已登录的页面：wger 自己的前端资源引用，别的应用不会有
  grep -q '@wger-project/react-components'
}

identity_check() { # 应用自己的“当前用户”接口认出的是用户管理员
  local out
  _wg_session || return 1
  out="$(curl -sS --noproxy '*' --max-time 120 --fail-with-body -b "${SMOKE_JAR:-}" \
    "$(_wg_origin)/api/v2/userprofile/")" ||
    { echo "GET /api/v2/userprofile/ 失败：$out"; return 1; }
  jq -e --arg u "$SMOKE_OWNER" '.username == $u' <<<"$out" >/dev/null ||
    { echo "接口认出的不是 $SMOKE_OWNER：$(head -c 200 <<<"$out")"; return 1; }
}

data_write() { # 建一个训练计划，name 写随机标记，标记覆盖写进 $SMOKE_STATE
  local marker out csrf today
  _wg_session || return 1
  # Routine.name 上限 25 字符：前缀加 12 位十六进制刚好 24
  marker="wonton-$(head -c6 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  # 浏览版 API 页面会带 CSRF 表单，Django 这时才发 csrftoken cookie；纯 JSON 请求不发
  out="$(curl -sS --noproxy '*' --max-time 120 -o /dev/null -b "${SMOKE_JAR:-}" -c "${SMOKE_JAR:-}" \
    -H 'Accept: text/html' "$(_wg_origin)/api/v2/")" ||
    { echo "取 API 页面失败：$out"; return 1; }
  csrf="$(awk '$6=="csrftoken"{print $7}' "${SMOKE_JAR:-}" | tail -1)"
  [[ -n "$csrf" ]] || { echo "会话里没有 csrftoken"; return 1; }
  today="$(date +%F)"
  out="$(curl -sS --noproxy '*' --max-time 120 --fail-with-body -b "${SMOKE_JAR:-}" -X POST \
    -H "X-CSRFToken: $csrf" -H "Referer: $(_wg_origin)/" -H 'Content-Type: application/json' \
    --data-raw "{\"name\":\"$marker\",\"start\":\"$today\",\"end\":\"$today\"}" "$(_wg_origin)/api/v2/routine/")" ||
    { echo "POST /api/v2/routine/ 失败：$out"; return 1; }
  grep -qF "$marker" <<<"$out" || { echo "写入没带回标记：$(head -c 200 <<<"$out")"; return 1; }
  printf '%s' "$marker" > "$SMOKE_STATE"
}

data_check() { # 读回标记去应用里找
  local marker out
  marker="$(cat "$SMOKE_STATE" 2>/dev/null)"
  [[ -n "$marker" ]] || { echo "SMOKE_STATE 是空的（data_write 没跑过？）"; return 1; }
  _wg_session || return 1
  out="$(curl -sS --noproxy '*' --max-time 120 --fail-with-body -b "${SMOKE_JAR:-}" \
    "$(_wg_origin)/api/v2/routine/")" ||
    { echo "GET /api/v2/routine/ 失败：$out"; return 1; }
  grep -qF "$marker" <<<"$out" ||
    { echo "没找到标记 $marker，看到的是：$(head -c 300 <<<"$out")"; return 1; }
}
