# shellcheck shell=bash
# scripts/smoke.sh / e2e/smoke.sh source 本文件，不单独执行。
#
# 思源笔记：sso=none + 思源自带锁屏（访问授权码由云吞生成）。思源是单用户工作区，没有用户体系，
# 锁屏之后的会话不分身份，没有当前用户接口可断言。清单没有 mcp。
#
# 真实登录链：dweb / 落在 /welcome 引导页（nginx），页面带着访问授权码（只有经网关的主人拿得到）；
# 从页面取出授权码 POST /api/system/loginAuth 换工作区会话（Cookie 存回 SMOKE_JAR），之后的应用
# 接口都带着它会话。只依赖模板约定的变量（SMOKE_STATE/SMOKE_JAR/SMOKE_WEB_URL）和助手 web_get、dweb。
export IDENTITY_NA=1
export SMOKE_NOTE="思源是单用户工作区，没有用户体系：锁屏（访问授权码）之后会话不分身份，无当前用户接口可断言；授权码经 /welcome 引导页交给主人。"
export E2E_NOTE="思源是单用户工作区：锁屏后的会话不分身份，应用内无用户身份可断言。"

web_assert() { grep -q '的访问授权码'; }

# 登录：取引导页里的授权码（页面上 data-code 属性，避免正则抠正文），POST loginAuth 建工作区会话。
# 会话 Cookie 存进 SMOKE_JAR，之后 dweb / web_post 都带着它。
siyuan_login() (
  set -eu
  local page code out
  [[ -n "${SMOKE_JAR:-}" && -n "${SMOKE_WEB_URL:-}" ]] || { echo "缺 SMOKE_JAR 或 SMOKE_WEB_URL"; return 1; }
  page="$(dweb /)" || { echo "引导页失败：$page"; return 1; }
  code="$(sed -n 's/.*data-code="\([^"]*\)".*/\1/p' <<<"$page" | head -1)"
  [[ -n "$code" ]] || { echo "引导页没有 data-code：$(head -c 200 <<<"$page")"; return 1; }
  out="$(curl -sS --noproxy '*' -b "$SMOKE_JAR" -c "$SMOKE_JAR" --max-time 30 -w '\n%{http_code}' \
    -H 'Content-Type: application/json' -d "{\"authCode\":\"$code\",\"rememberMe\":true}" \
    "${SMOKE_WEB_URL%/}/api/system/loginAuth")" || { echo "loginAuth 请求失败：$out"; return 1; }
  [[ "${out##*$'\n'}" == 200 && "$out" == *'"code":0'* ]] ||
    { echo "loginAuth 没通过（HTTP ${out##*$'\n'}）：$(head -c 200 <<<"${out%$'\n'*}")"; return 1; }
)

# 数据：笔记本是思源的工作区持久数据（在插件数据目录里，升级、重装都保留）。
# data_write 登录后建一个带随机标记的笔记本，标记写进 SMOKE_STATE；data_check 读回标记，
# 在笔记本列表里找回它。
marker_file() { # 套件新契约：SMOKE_STATE 是预建的空标记文件，data_write 覆盖写、data_check 读；
  # 旧套件里它是状态目录，标记放目录内。两种形态都能跑。
  if [[ -d "$SMOKE_STATE" ]]; then printf '%s' "$SMOKE_STATE/siyuan-marker"; else printf '%s' "$SMOKE_STATE"; fi
}

data_write() (
  set -eu
  local mark out
  [[ -n "${SMOKE_STATE:-}" ]] || { echo "缺 SMOKE_STATE"; return 1; }
  siyuan_login
  mark="$(date +%Y%m%d%H%M%S)-$RANDOM"
  out="$(web_post /api/notebook/createNotebook "{\"name\":\"wonton-smoke-$mark\"}")" ||
    { echo "创建笔记本失败：$out"; return 1; }
  jq -e '.code == 0' <<<"$out" >/dev/null || { echo "createNotebook 返回非 0：$out"; return 1; }
  printf '%s' "$mark" >"$(marker_file)"
  echo "已建笔记本 wonton-smoke-$mark"
)

data_check() (
  set -eu
  local mark out
  [[ -n "${SMOKE_STATE:-}" ]] || { echo "缺 SMOKE_STATE"; return 1; }
  mark="$(cat "$(marker_file)" 2>/dev/null)" || true
  [[ -n "$mark" ]] || { echo "没有 data_write 写下的标记"; return 1; }
  siyuan_login
  out="$(web_post /api/notebook/lsNotebooks '{}')" || { echo "读笔记本列表失败：$out"; return 1; }
  jq -e --arg n "wonton-smoke-$mark" \
    '.code == 0 and any(.data.notebooks[]; .name == $n)' <<<"$out" >/dev/null ||
    { echo "找不到笔记本 wonton-smoke-$mark，看到的是：$(head -c 200 <<<"$out")"; return 1; }
  echo "笔记本 wonton-smoke-$mark 还在：升级/重装保住了数据"
)

# POST 助手：带用户会话（SMOKE_JAR 里的网关会话 + 应用会话）向应用自己的接口发 JSON。
web_post() (
  set -eu
  local path="$1" body="$2" out
  [[ -n "${SMOKE_WEB_URL:-}" ]] || { echo "缺 SMOKE_WEB_URL"; return 1; }
  out="$(curl -sS --noproxy '*' -b "${SMOKE_JAR:-}" --max-time 30 -w '\n%{http_code}' \
    -H 'Content-Type: application/json' -d "$body" "${SMOKE_WEB_URL%/}$path")" ||
    { echo "请求失败：$out"; return 1; }
  [[ "${out##*$'\n'}" == 200 ]] || { echo "HTTP ${out##*$'\n'}：$(head -c 200 <<<"${out%$'\n'*}")"; return 1; }
  printf '%s' "${out%$'\n'*}"
)
