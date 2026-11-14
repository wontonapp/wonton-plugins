# shellcheck shell=bash
# Komga 按邮箱匹配 OIDC 用户；数据用客户端设置 API 写入，不触碰只读的漫画文件夹。

# 匿名 401 由套件检查；登录后的首页必须是 Komga 自己的页面。
web_assert() { grep -qi '<title>Komga</title>'; }

komga_login() {
  dweb --sso / --fail-with-body >/dev/null || { echo "Komga OIDC 登录失败"; return 1; }
}

identity_check() {
  local me
  komga_login || return 1
  me="$(dweb /api/v2/users/me -H 'Accept: application/json' --fail-with-body)" ||
    { echo "读取 Komga 当前用户失败：$me"; return 1; }
  [[ -n "${SMOKE_OWNER_EMAIL:-}" ]] || { echo "没有主人邮箱"; return 1; }
  jq -e --arg email "$SMOKE_OWNER_EMAIL" '.email == $email and (.roles | index("ADMIN") != null)' <<<"$me" >/dev/null ||
    { echo "当前用户不是主人的管理员账号：$me"; return 1; }
}

data_write() {
  local marker reply body
  marker="wonton-smoke-$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
  komga_login || return 1
  body="$(jq -nc --arg value "$marker" '{"wonton.smoke": {value: $value}}')"
  reply="$(dweb /api/v1/client-settings/user -X PATCH -H 'Content-Type: application/json' --data "$body" --fail-with-body)" ||
    { echo "写入 Komga 用户设置失败：$reply"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() {
  local marker reply
  marker="$(cat "$SMOKE_STATE")" || { echo "读取数据标记失败"; return 1; }
  [[ -n "$marker" ]] || { echo "数据标记为空"; return 1; }
  komga_login || return 1
  reply="$(dweb /api/v1/client-settings/user/list -H 'Accept: application/json' --fail-with-body)" ||
    { echo "读取 Komga 用户设置失败：$reply"; return 1; }
  jq -e --arg marker "$marker" '.["wonton.smoke"].value == $marker' <<<"$reply" >/dev/null ||
    { echo "Komga 用户设置没有写入的标记：$reply"; return 1; }
}
