# scripts/smoke.sh source 本文件，不单独执行。
# shellcheck shell=bash
# Penpot 的登录链不能只靠重定向走完：授权起点是 POST 接口（返回 JSON），首次登录停在官方
# 姓名确认表单。探测按浏览器会发的请求自己走一遍（发起 → 跟跳转 → 调官方 register-profile），
# 不写库、不预建账号、不改应用；断言当前用户是网关注入邮箱对应的 OIDC 主人。
web_assert() { grep -qi 'Full-stack design'; }

penpot_oidc_callback() {
  python3 -c 'import json,sys,urllib.parse
url=urllib.parse.urlsplit(sys.stdin.read())
route,sep,query=url.fragment.partition("?")
params=urllib.parse.parse_qs(url.query if url.query else query)
screen=params.get("screen", [route.lstrip("/").replace("/", "-")])[0]
token=params.get("token", [""])[0]
if screen not in ("auth-register-validate", "auth-verify-token") or not token:
    sys.exit("OIDC callback has no supported screen or token")
json.dump({"screen":screen, "token":token}, sys.stdout)'
}

penpot_identity_check() { # <jar> <url> <邮箱>：注册和已有账号登录均核对应用当前用户，不输出授权令牌。
  local jar="$1" url="$2" owner_email="$3" start code body callback profile stop
  body="$(mktemp)"
  code="$(curl -sS --noproxy '*' --max-time 30 -o "$body" -w '%{http_code}' -b "$jar" -c "$jar" \
    -H 'Accept: application/json' -X POST "$url/api/auth/oidc?provider=oidc")" ||
    { echo "发起 OIDC 请求失败" >&2; rm -f "$body"; return 1; }
  [[ "$code" == 200 ]] ||
    { echo "POST /api/auth/oidc 返回 HTTP $code" >&2; rm -f "$body"; return 1; }
  start="$(jq -er '.redirectUri // .["redirect-uri"]' <"$body")" ||
    { echo "发起 OIDC 的响应里没有 redirectUri" >&2; rm -f "$body"; return 1; }
  rm -f "$body"; body="$(mktemp)"
  stop="$(curl -sS --noproxy '*' --max-time 60 -o "$body" -L -b "$jar" -c "$jar" \
    -H 'Accept: text/html' -w '%{http_code} %{url_effective}' "$start")" ||
    { echo "authorize 链请求失败" >&2; rm -f "$body"; return 1; }
  code="${stop%% *}"; stop="${stop#* }"
  [[ "$code" == 2* ]] ||
    { echo "authorize 链返回 HTTP $code" >&2; rm -f "$body"; return 1; }
  callback="$(printf '%s' "$stop" | penpot_oidc_callback)" ||
    { rm -f "$body"; return 1; }
  if [[ "$(jq -r .screen <<<"$callback")" == auth-register-validate ]]; then
    # register-profile 只收令牌和可选订阅设置；姓名来自令牌内的 OIDC claims。
    code="$(jq -c '{token}' <<<"$callback" | curl -sS --noproxy '*' --max-time 30 \
      -o "$body" -w '%{http_code}' -b "$jar" -c "$jar" \
      -H 'Accept: application/json' -H 'Content-Type: application/json' \
      --data-binary @- "$url/api/rpc/command/register-profile")" ||
      { echo "register-profile 请求失败" >&2; rm -f "$body"; return 1; }
    [[ "$code" == 2* ]] ||
      { echo "register-profile 返回 HTTP $code" >&2; rm -f "$body"; return 1; }
  fi
  rm -f "$body"
  profile="$(curl -sS --noproxy '*' --max-time 30 -b "$jar" \
    -H 'Accept: application/json' "$url/api/rpc/command/get-profile")" ||
    { echo "get-profile 失败" >&2; return 1; }
  jq -e --arg email "$owner_email" '
    .id != null and .id != "00000000-0000-0000-0000-000000000000" and
    .authBackend == "oidc" and .isActive == true and .email == $email
  ' <<<"$profile" >/dev/null || {
    echo "应用当前用户不是 OIDC 主人（$owner_email）：$(head -c 200 <<<"$profile")"
    return 1
  }
}

identity_check() {
  local email
  # OIDC 给插件的是核心稳定的内部邮箱；冒烟套件旧版 SMOKE_OWNER_EMAIL 可能仍是实例域名邮箱。
  email="${SMOKE_OWNER}@wonton.internal"
  penpot_identity_check "$SMOKE_JAR" "${SMOKE_WEB_URL%/}" "$email"
}

penpot_rpc() { # <命令> <JSON 参数>：带应用会话（identity_check 建立的）调 Penpot RPC
  curl -sS --noproxy '*' --fail-with-body -b "$SMOKE_JAR" -H 'Content-Type: application/json' \
    -d "$2" "${SMOKE_WEB_URL%/}/api/rpc/command/$1"
}

_penpot_uuid() { # 从响应里抽第一个 UUID（penpot 部分命令回 Transit-EDN，id 是 ~u 前缀或裸字符串）
  grep -oE '~u[0-9a-f-]{36}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1 | tr -d '~u'
}

# 数据：设计文件就是用户数据（Postgres 在插件数据目录，重装保留）。data_write 调应用自己的 RPC：
# 建一个名字带随机标记的项目（create-project，挂在自己的默认 team 下），data_check 重装/升级后项目还在。
data_write() {
  local teams team_id marker resp
  identity_check || return 1
  teams="$(penpot_rpc get-teams '{}')" || { echo "get-teams 失败：$teams"; return 1; }
  team_id="$(_penpot_uuid <<<"$teams")"
  [[ -n "$team_id" ]] || { echo "拿不到 team：$(head -c 300 <<<"$teams")"; return 1; }
  marker="wonton-smoke-$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')"
  resp="$(penpot_rpc create-project "{\"teamId\":\"$team_id\",\"name\":\"$marker\"}")" ||
    { echo "create-project 失败：$resp"; return 1; }
  grep -Fq "$marker" <<<"$resp" || { echo "项目没建成：$(head -c 300 <<<"$resp")"; return 1; }
  jq -nc --arg t "$team_id" --arg m "$marker" '{team_id:$t,name:$m}' >"$SMOKE_STATE"
}

data_check() {
  local team_id name marker_projects
  read -r team_id name < <(jq -r '[.team_id, .name] | @tsv' "$SMOKE_STATE" 2>/dev/null) || true
  [[ -n "$team_id" && -n "$name" ]] || { echo "没有 data_write 留下的标记"; return 1; }
  identity_check || return 1
  marker_projects="$(penpot_rpc get-projects "{\"teamId\":\"$team_id\"}")" ||
    { echo "get-projects 失败：$marker_projects"; return 1; }
  grep -Fq "$name" <<<"$marker_projects" ||
    { echo "升级/重装后找不到项目 $name：$(head -c 300 <<<"$marker_projects")"; return 1; }
}
