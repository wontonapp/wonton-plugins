# shellcheck shell=bash
# grist 的冒烟探针。变量和助手由套件提供：SMOKE_OWNER、SMOKE_OWNER_EMAIL、SMOKE_STATE、SMOKE_JAR、
# dweb / web_get 等，见 scripts/probes.sh。插件不声明 MCP（上游社区版没有内置 MCP 服务），
# “工具”一项由套件按清单没有 mcp 记 N/A。
#
# 页面：Grist 是单页应用，引导页里有 window.gristConfig。页面从标准输入进来只能读一次：
# 先整个读进变量，再对变量做两个断言（对 stdin 连 grep 两次，第二次是空的）。
web_assert() {
  local page
  page="$(cat)"
  grep -q 'window.gristConfig' <<<"$page" && grep -q 'Grist' <<<"$page"
}

# 身份：Grist 的“当前用户”接口。登录邮箱是核心注入的内部邮箱，域名随环境变（不是通知地址
# SMOKE_OWNER_EMAIL），认人按用户名（头里邮箱的前缀）加安装管理员标志；两个值都来自网关注入的头，
# 和 GRIST_DEFAULT_EMAIL 同源，所以主人登录即管理员。
identity_check() {
  local body
  body="$(dweb /api/session/access/active)" || { echo "请求失败：$body"; return 1; }
  grep -qF "\"name\":\"$SMOKE_OWNER\"" <<<"$body" ||
    { echo "当前用户不是主人（$SMOKE_OWNER）：$(head -c 200 <<<"$body")"; return 1; }
  grep -qF '"isInstallAdmin":true' <<<"$body" ||
    { echo "主人不是安装管理员：$(head -c 200 <<<"$body")"; return 1; }
}

# 数据：建一张文档、写一行带随机标记的记录；
# 重装/升级后按文档 id 读回这一行，标记还在才算数。
_data_post() { # <路径> <JSON>：带会话调 Grist 的 REST 接口，输出响应体
  dweb "$1" -X POST -H 'Content-Type: application/json' --data "$2"
}

data_write() {
  local marker
  marker="probe-$(head -c 6 /dev/urandom | base64 | tr -dc 'a-z0-9' | head -c 8)"
  local orgs org_id workspaces ws_id doc doc_id out
  orgs="$(dweb /api/orgs)" || { echo "列站点失败：$orgs"; return 1; }
  org_id="$(jq -r '.[0].id // empty' <<<"$orgs")"
  [[ -n "$org_id" ]] || { echo "没有可用的站点：$(head -c 200 <<<"$orgs")"; return 1; }
  workspaces="$(dweb "/api/orgs/$org_id/workspaces")" || { echo "列工作区失败：$workspaces"; return 1; }
  ws_id="$(jq -r '.[0].id' <<<"$workspaces")"
  [[ -n "$ws_id" && "$ws_id" != null ]] || { echo "home 站点没有工作区：$(head -c 200 <<<"$workspaces")"; return 1; }
  doc="$(_data_post "/api/workspaces/$ws_id/docs" "{\"name\":\"冒烟-$marker\"}")" || { echo "建文档失败：$doc"; return 1; }
  doc_id="$(jq -r '.' <<<"$doc")"
  [[ -n "$doc_id" && "$doc_id" != null ]] || { echo "建文档返回不是 id：$(head -c 200 <<<"$doc")"; return 1; }
  out="$(_data_post "/api/docs/$doc_id/tables/Table1/records" \
    "{\"records\":[{\"fields\":{\"A\":\"$marker\"}}]}")" || { echo "写记录失败：$out"; return 1; }
  grep -q '"id":' <<<"$out" || { echo "写记录返回不对：$(head -c 200 <<<"$out")"; return 1; }
  printf '%s %s\n' "$doc_id" "$marker" >"$SMOKE_STATE"
}

data_check() {
  local doc_id marker out
  read -r doc_id marker <"$SMOKE_STATE"
  out="$(dweb "/api/docs/$doc_id/tables/Table1/records")" || { echo "读记录失败：$out"; return 1; }
  grep -qF "\"A\":\"$marker\"" <<<"$out" ||
    { echo "文档 $doc_id 里找不到标记 $marker：$(head -c 300 <<<"$out")"; return 1; }
}
