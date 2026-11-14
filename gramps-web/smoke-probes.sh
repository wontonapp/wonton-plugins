# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# 变量别叫 state：shellcheck 把这份文件连着 probes.sh 一起分析，probes.sh 自己也读一个 dev.sh 提供的 state；
# 子壳体函数（fn() ( … )）里给 state 赋值，SC2031 就报在 probes.sh 上，CI 红。
#
# 变量：两套冒烟环境（dev.sh smoke、正式 e2e 套件）都给的只有 SMOKE_STATE、SMOKE_JAR、SMOKE_WEB_URL、
#       SMOKE_OWNER、SMOKE_BASE、SMOKE_TOKEN、SMOKE_INSTANCE 和 web_get、dweb。SMOKE_WEB_URL 的含义两边不同
#       （dev.sh smoke 是带路径的网页入口，正式套件剥成只有 origin），拼 API 一律用 _gw_origin 现场取 origin，
#       不依赖 SMOKE_WEB_ORIGIN/SMOKE_PLUGIN_ID（正式套件的钩子约定里没有这两个）。
# 页面：web_assert 从标准输入读页面，命中的确是应用的真实页面时返回 0。
# 身份：identity_check 用应用自己的"当前用户"接口断言认出的是用户管理员，不碰应用里的数据。
# 数据：data_write / data_check 写一条带随机标记的数据、重装后读回。
# 没有 MCP（上游没有，规范也不让自造），不设 MCP_TOOL。
#
# Gramps 的 API 认的是 Bearer JWT（前端存 localStorage，不走 Cookie），所以这里自己走一遍
# OIDC 换令牌，再带 Bearer 调应用接口；核心会话 Cookie（SMOKE_JAR）仍然全程带着——Hydra
# authorize 靠它认出主人。
# /api/oidc/login/ 和 callback 有限速（5 次/分钟），同一个 shell 里缓存令牌，别反复走登录链。
# shellcheck shell=bash disable=SC2034  # SSO/MCP 由冒烟套件读取
SSO=oidc
MCP=0

GW_SLUG=org-gramps-project-grampsweb   # 清单 id 里的点换成 -，网页地址的主机名第一段

_gw_origin() { # web_url 可能带路径，拼 API 前取 origin；单跑数据探针时没有 SMOKE_WEB_URL，
               # 按固定 slug 和核心地址推（插件网页是 <slug>.<实例域名>）。
  local url="${SMOKE_WEB_URL:-}"
  if [[ -z "$url" ]]; then
    [[ -n "${SMOKE_BASE:-}" ]] || { echo "缺 SMOKE_BASE" >&2; return 1; }
    url="http://${GW_SLUG}.${SMOKE_BASE#http://}"
  fi
  local u="${url%%\?*}" rest="${url#*://}"
  printf '%s://%s' "${u%%://*}" "${rest%%/*}"
}

_gw_state_file() { # SMOKE_STATE 在 e2e 里是标记文件本身，不是目录
  if [[ -d "$SMOKE_STATE" ]]; then printf '%s' "$SMOKE_STATE/gramps-marker"; else printf '%s' "${SMOKE_STATE}.marker"; fi
}

# 走完整 OIDC：核心会话 → 应用 /api/oidc/login/?provider=custom → Hydra authorize → 应用
# /api/oidc/callback/custom（302 到 /oidc/complete#code=…）→ POST /api/oidc/tokens/ 换 JWT。
# 打到标准输出（诊断走标准错误）。provider 名和回调路径来自清单（provider 固定 custom，上游源码）。
gw_token() {
  local jar base_core origin loc url callback done_url code body hops
  jar="${SMOKE_JAR:-}"
  base_core="${SMOKE_BASE:-}"
  origin="$(_gw_origin)" || return 1
  [[ -n "$jar" && -n "$base_core" && -n "${SMOKE_TOKEN:-}" ]] ||
    { echo "缺冒烟变量：需要 SMOKE_JAR/SMOKE_BASE/SMOKE_TOKEN" >&2; return 1; }
  # 核心会话补进 jar（-b 保留别的域的 Cookie，只写 -c 会抹掉）。
  curl -sS --noproxy '*' -o /dev/null -b "$jar" -c "$jar" --data-urlencode "token=${SMOKE_TOKEN}" \
    "$base_core/_wonton/login" ||
    { echo "主人令牌登录不了核心" >&2; return 1; }
  # 应用起跳：302 到身份提供方的 authorize 地址（顺带拿到应用的 OIDC 会话 Cookie）。
  loc="$(curl -sS --noproxy '*' -o /dev/null -w '%{redirect_url}' -b "$jar" -c "$jar" \
    "$origin/api/oidc/login/?provider=custom")" ||
    { echo "起跳失败：/api/oidc/login/" >&2; return 1; }
  [[ "$loc" == http* ]] || { echo "起跳没给 authorize 地址：$loc" >&2; return 1; }
  # authorize 链不止一跳：/oauth2/auth → 核心登录页（/_wonton/oidc/login?login_challenge=…）
  # → 带 login_verifier 回 /oauth2/auth → 可能还有同意页 → 应用回调。逐跳跟着走，
  # 直到 Location 落在应用回调上；核心会话 Cookie 全程有效，不用提交表单。
  local url="$loc" callback="" hops=0
  while (( hops < 8 )); do
    callback="$(curl -sS --noproxy '*' -o /dev/null -w '%{redirect_url}' -b "$jar" -c "$jar" "$url")" ||
      { echo "authorize 链请求失败：$url" >&2; return 1; }
    if [[ "$callback" == *"/api/oidc/callback/custom"* ]]; then break; fi
    if [[ "$callback" != http* ]]; then
      echo "authorize 链断了（第 $((hops + 1)) 跳不是重定向）：$url" >&2
      return 1
    fi
    url="$callback"
    hops=$((hops + 1))
  done
  [[ -n "$callback" && "$callback" == *"/api/oidc/callback/custom"* ]] ||
    { echo "authorize 没走到应用回调，停在：$url" >&2; return 1; }
  # 回调再 302 到前端 /oidc/complete#code=…：别跟（跟过去是 SPA，fragment 不好拿），直接读 Location。
  done_url="$(curl -sS --noproxy '*' -o /dev/null -w '%{redirect_url}' -b "$jar" -c "$jar" "$callback")" ||
    { echo "回调失败：$callback" >&2; return 1; }
  code="${done_url##*#code=}"
  [[ -n "$code" && "$code" != "$done_url" ]] || { echo "回调没带 code：$done_url" >&2; return 1; }
  body="$(curl -sS --noproxy '*' --fail-with-body -b "$jar" -H 'Content-Type: application/json' \
    -d "{\"code\":\"$code\"}" "$origin/api/oidc/tokens/")" ||
    { echo "换令牌失败：$body" >&2; return 1; }
  jq -r '.access_token // empty' <<<"$body"
}

gw_token_cached() {
  if [[ -z "${GW_TOKEN:-}" ]]; then GW_TOKEN="$(gw_token)" || return 1; fi
  printf '%s' "$GW_TOKEN"
}

web_assert() { grep -q '<gramps-js>'; }   # 上游 SPA 的宿主元素，别的页面不会有

identity_check() {
  local token me
  token="$(gw_token_cached)" || return 1
  me="$(curl -sS --noproxy '*' --fail-with-body -b "${SMOKE_JAR:-}" -H "Authorization: Bearer $token" \
    "$(_gw_origin)/api/users/-/")" ||
    { echo "当前用户接口失败：$me"; return 1; }
  jq -e --arg u "$SMOKE_OWNER" '.name == $u' <<<"$me" >/dev/null ||
    { echo "当前用户不是用户 $SMOKE_OWNER：$(head -c 200 <<<"$me")"; return 1; }
  # 主人在 groups 声明里带 owner，映射到 Gramps Admin（角色值 5）；映射没生效会直接 401。
  jq -e '.role == 5' <<<"$me" >/dev/null ||
    { echo "主人不是 Admin（角色映射没生效？）：$(jq -c '{name,role}' <<<"$me")"; return 1; }
}

# 保数据：带标记的 Note 走 POST /api/objects/（对象形状按上游测试夹具：
# text 是 {_class: StyledText, string: …}），标记和 handle 写进 $SMOKE_STATE；
# 升级/重装后 GET /api/notes/<handle> 读回核对。
data_write() {
  local token marker handle out origin
  origin="$(_gw_origin)" || { echo "推不出应用地址" >&2; return 1; }
  [[ -n "${SMOKE_JAR:-}" ]] ||
    { echo "缺冒烟变量：需要 SMOKE_JAR" >&2; return 1; }
  token="$(gw_token_cached)" || { echo "拿不到应用令牌，写不了数据" >&2; return 1; }
  marker="保数据-$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
  handle="$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
  out="$(curl -sS --noproxy '*' --fail-with-body -b "${SMOKE_JAR:-}" -X POST \
    -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d "[{\"_class\":\"Note\",\"handle\":\"$handle\",\"text\":{\"_class\":\"StyledText\",\"string\":\"$marker\"}}]" \
    "$origin/api/objects/")" || { echo "建备注失败：$out"; return 1; }
  jq -nc --arg m "$marker" --arg h "$handle" '{marker:$m,handle:$h}' >"$( _gw_state_file )"
}

data_check() {
  local token marker handle seen origin
  origin="$(_gw_origin)" || { echo "推不出应用地址" >&2; return 1; }
  marker="$(jq -r .marker "$( _gw_state_file )")"
  handle="$(jq -r .handle "$( _gw_state_file )")"
  token="$(gw_token_cached)" || { echo "拿不到应用令牌，查不了数据" >&2; return 1; }
  seen="$(curl -sS --noproxy '*' --fail-with-body -b "${SMOKE_JAR:-}" -H "Authorization: Bearer $token" \
    "$origin/api/notes/$handle")" ||
    { echo "备注 $handle 读不到（数据丢了？）：$seen"; return 1; }
  jq -e --arg m "$marker" '.text.string == $m' <<<"$seen" >/dev/null ||
    { echo "备注对不上：期待 $marker，看到 $(jq -c '{gramps_id,text}' <<<"$seen" 2>/dev/null || head -c 200 <<<"$seen")"; return 1; }
}
