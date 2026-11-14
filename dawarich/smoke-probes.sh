# shellcheck shell=bash
# scripts/smoke.sh 的一项一项探测。页面特征、身份、数据都走真实的应用接口。

# 源地址（scheme://host:port）：真实冒烟（e2e/smoke.sh）给的是 SMOKE_WEB_URL（网页入口
# 完整地址，含路径），模板 dev 探针给的是 SMOKE_WEB_ORIGIN；后者没有就从前者切。
# 探针文件被 source 时这两个变量可能都还没设（框架是逐项探测时才注入的），所以现取现用。
_smoke_origin() {
  if [[ -n "${SMOKE_WEB_ORIGIN:-}" ]]; then
    printf '%s' "$SMOKE_WEB_ORIGIN"
    return
  fi
  local tail="${SMOKE_WEB_URL#*://}"
  printf '%s' "${SMOKE_WEB_URL%%"$tail"}${tail%%/*}"
}

# 页面：SSO 登录后落到应用的时间线/地图页，标题带 Dawarich。
web_assert() { grep -qi '<title>[^<]*dawarich'; }

# GET 页面：带会话、把 Set-Cookie 写回会话文件。不用框架的 dweb——各冒烟实现里它不一定
# 写回 cookie，而登录表单的 CSRF 令牌跟着会话走，取表单的请求必须把新会话存下来。
_fetch_page() {
  curl -sS --noproxy '*' -m 60 -L -b "$SMOKE_JAR" -c "$SMOKE_JAR" \
    -H 'Accept: text/html' "$(_smoke_origin)$1"
}

# 工具：上游自带的只读 MCP。get_latest_location 空库时也返回 {"point":null}，不依赖数据。
# 工具名带全前缀（总线用清单 id 换成连字符的 slug），不依赖框架给的插件前缀变量。
export MCP_TOOL=app-dawarich-dawarich__get_latest_location
export MCP_ARGS={}

mcp_assert() {
  grep -q '"point"' && ! grep -q '"isError":true'
}

# 框架的默认 mcp_check 在 e2e 里跑不通，SMOKE_MCP_URL/SMOKE_SLUG 也不在它的变量契约里；
# 这里按标签找 Hermes 的 gateway 容器（e2e 一定装了 Hermes），用它的总线地址真实调一次。
# ../scripts/mcp_client.py 是开发套件随合仓根目录带的 MCP 客户端，gateway 镜像里有它要的 SDK。
mcp_check() {
  local gw url out
  gw="$(docker ps --filter "label=wonton.instance=${SMOKE_INSTANCE:-}" --format '{{.Names}}' \
    | grep -F "com-nousresearch-hermes--gateway" | head -1)"
  [[ -n "$gw" ]] || { echo "找不到 Hermes 的 gateway 容器（wonton.instance=${SMOKE_INSTANCE:-}）"; return 1; }
  url="$(docker exec "$gw" printenv WONTON_MCP_URL 2>/dev/null)" ||
    { echo "gateway 容器（$gw）里没有 WONTON_MCP_URL"; return 1; }
  out="$(docker exec -i "$gw" python3 - call "$url" "$MCP_TOOL" "$MCP_ARGS" \
    <"$(dirname "${BASH_SOURCE[0]}")/../scripts/mcp_client.py" 2>&1)" ||
    { echo "调用只读工具 $MCP_TOOL 失败：$(head -c 300 <<<"$out" | tr '\n' ' ')"; return 1; }
  mcp_assert <<<"$out" ||
    { echo "$MCP_TOOL 的结果不符合预期：$(head -c 200 <<<"$out" | tr '\n' ' ')"; return 1; }
}

identity_check() { # 设置的用户列表只有管理员看得见，页面上有主人邮箱和 Admin 徽标
  local page
  page="$(_authorized_page /settings/users)" || { echo "${page:-页面失败}"; return 1; }
  grep -qF "${SMOKE_OWNER}@" <<<"$page" ||
    { echo "用户列表没显示主人邮箱（${SMOKE_OWNER}@…）：$(head -c 150 <<<"$page")"; return 1; }
  grep -q '>Admin<' <<<"$page" ||
    { echo "用户列表里没有 Admin 徽标：$(head -c 150 <<<"$page")"; return 1; }
}

# 应用会话经 OIDC 建立后才有效。冒烟框架默认的 web_login 只走到应用登录页；
# omniauth 2.x 的 OIDC 请求段只收 POST，这里补上提交表单、跟完跳转链这一段。
web_login() {
  local page
  page="$(_fetch_page /settings/users)" || { echo "页面请求失败：/settings/users"; return 1; }
  grep -q 'action="/users/auth/openid_connect"' <<<"$page" || return 0 # 已有应用会话
  _app_sso_login
}

# 应用会话要经 OIDC 建立过才有。omniauth 2.x 的 OIDC 请求段只收 POST，核心的 --sso 自动表单
# 会撞上排在前面、字段为空的邮箱登录表单，所以这里自己解析按钮表单提交，再跟完跳转链。
_authorized_page() {
  local page
  page="$(_fetch_page "$1")" || { echo "页面失败：$1"; return 1; }
  if grep -q 'action="/users/auth/openid_connect"' <<<"$page"; then
    _app_sso_login || return 1
    page="$(_fetch_page "$1")" || { echo "页面失败：$1"; return 1; }
  fi
  printf '%s' "$page"
}

# 标准输入是页面：输出第一个（可选 $1 匹配 action 子串）自动提交型 POST 表单的 urlencoded 字段。
# 只认字段全是 hidden 的表单：跳转链上要跟的是无 JS 就不动的自动提交表单；
# 应用页面上的普通表单（有文本框、按钮的）不能抓来重发。
_form_post_data() {
  python3 -c '
import html.parser, sys
from urllib.parse import urlencode

want = sys.argv[1] if len(sys.argv) > 1 else ""

class Forms(html.parser.HTMLParser):
    def __init__(self):
        super().__init__()
        self.cur = None
        self.forms = []
    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "form" and self.cur is None:
            self.cur = {"action": a.get("action", ""), "method": a.get("method", "get").lower(), "fields": [], "extra": False}
        elif tag == "input" and self.cur is not None and a.get("name"):
            if a.get("type", "text").lower() == "hidden":
                self.cur["fields"].append((a["name"], a.get("value", "")))
            else:
                self.cur["extra"] = True
        elif tag in ("select", "textarea") and self.cur is not None:
            self.cur["extra"] = True
        elif tag == "button" and self.cur is not None and not want:
            # 裸调用（跟跳转链）只要纯 hidden 的自动提交表单；显式给了 action 子串才认带按钮的表单
            # （登录页的 OIDC 入口表单有按钮，登出之类的表单不能抓来重发）。
            self.cur["extra"] = True
    def handle_endtag(self, tag):
        if tag == "form" and self.cur is not None:
            self.forms.append(self.cur)
            self.cur = None

p = Forms()
p.feed(sys.stdin.read())
for f in p.forms:
    if f["method"] == "post" and f["fields"] and not f["extra"] and want in f["action"]:
        print(urlencode(f["fields"]))
        break
' "${1:-}"
}

_app_sso_login() { # 提交登录页的 OIDC 按钮表单（CSRF），跟完 hydra 的跳转链；链上遇到隐藏字段自动表单就照提交。
  local page form out url
  for _attempt in 1 2; do
    page="$(_fetch_page /users/sign_in)" || { echo "登录页请求失败"; return 1; }
    form="$(_form_post_data '/users/auth/openid_connect' <<<"$page")"
    [[ -n "$form" ]] || { echo "登录页上没有 OIDC 表单"; return 1; }
    url="$(_smoke_origin)/users/auth/openid_connect"
    for _ in 1 2 3 4 5 6 7 8; do
      out="$(curl -sS --noproxy '*' -m 60 -L -b "$SMOKE_JAR" -c "$SMOKE_JAR" \
        -H 'Accept: text/html' --data "$form" -w '\n%{url_effective}' "$url")" ||
        { echo "OIDC 跳转请求失败"; return 1; }
      url="${out##*$'\n'}"
      page="${out%$'\n'*}"
      form="$(_form_post_data <<<"$page")"
      [[ -n "$form" ]] || break
    done
    grep -qi '<title>[^<]*dawarich' <<<"$page" && return 0
    # 跳转链偶尔会撞上一次性的状态（再走一遍是全新会话），最后一遍的报错才作数。
    ((_attempt < 2)) || { echo "OIDC 登录没落到应用页面：$(head -c 150 <<<"$page" | tr '\n' ' ')"; return 1; }
  done
}

# 数据：按设置里主人那行的接口密钥调应用自己的 API（过网关带核心会话 cookie）。
# data_write 写一个带随机标记（device_id）的点，data_check 从 points 列表里找回这个标记。
_owner_page() { # 主人在用户列表里的详情页（接口密钥在这里）
  local index url
  index="$(_authorized_page /settings/users)" || { echo "$index"; return 1; }
  url="$(SMOKE_OWNER="${SMOKE_OWNER:-}" python3 -c '
import os, re, sys
t = sys.stdin.read()
owner = os.environ.get("SMOKE_OWNER", "")
m = re.search(r"href=\"(/settings/users/\d+)\"[^>]*>[^<]*" + re.escape(owner) + "@", t)
print(m.group(1) if m else "")
' <<<"$index")"
  [[ -n "$url" ]] || { echo "用户列表里找不到主人（${SMOKE_OWNER}@…）"; return 1; }
  _fetch_page "$url"
}

_api_key() {
  local page key
  page="$(_owner_page)" || { echo "$page"; return 1; }
  key="$(grep -o 'data-clipboard-text-value="[^"]*"' <<<"$page" | head -1 | sed 's/.*="//; s/"$//')"
  [[ "$key" =~ ^[0-9a-f]{32,128}$ ]] || { echo "主人页上取不到接口密钥：$(head -c 150 <<<"$page")"; return 1; }
  printf '%s' "$key"
}

data_write() {
  local key marker lat lon body code
  key="$(_api_key)" || { echo "$key"; return 1; }
  marker="wontonsmoke${RANDOM}${RANDOM}"
  lat="55.0${RANDOM}1"
  lon="12.6${RANDOM}2"
  body="$(printf '{"locations":[{"type":"Feature","geometry":{"type":"Point","coordinates":[%s,%s]},"properties":{"timestamp":1700000123,"device_id":"%s"}}]}' "$lon" "$lat" "$marker")"
  code="$(curl -sS --noproxy '*' -m 30 -b "$SMOKE_JAR" -H 'Content-Type: application/json' -d "$body" \
    -o /dev/null -w '%{http_code}' "$(_smoke_origin)/api/v1/points?api_key=$key")" ||
    { echo "请求失败：$code"; return 1; }
  [[ "$code" == 2* ]] || { echo "写点失败：POST /api/v1/points 返回 $code"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() {
  local key marker
  key="$(_api_key)" || { echo "$key"; return 1; }
  marker="$(cat "$SMOKE_STATE")"
  [[ -n "$marker" ]] || { echo "标记为空"; return 1; }
  curl -sS --noproxy '*' -m 30 -b "$SMOKE_JAR" \
    "$(_smoke_origin)/api/v1/points?api_key=$key&per_page=10000" | grep -qF "$marker" ||
    { echo "points 列表里没有标记 $marker"; return 1; }
}
