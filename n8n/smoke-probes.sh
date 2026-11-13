# shellcheck shell=bash
# n8n：sso=none，应用自己管账号（第一次打开自己注册管理员，冒烟会话里没有 n8n 登录态）；
# 清单没有 mcp 入口（实例级 MCP 的令牌要登录后才能生成，见 README）。
#
# 数据：数据探针走应用自己的 CLI（import/export:workflow），不需要应用登录态；
# 导入的是普通工作流（未激活，不执行任何节点），重装后还在即为通过。

web_assert() { grep -qi '<title>n8n'; }   # 标准输入：编辑器 SPA 的 HTML，标题带 n8n

# 身份：GET /rest/login 带云吞会话（SMOKE_JAR）但不带 n8n 会话。断言回应的是应用自己的 401
#（body 是 {"status":"error",...}，不是网关的 {"error":{"category":...}}）：请求穿过了网关到达
# 应用，应用又在坚持自己的登录，没有因为云吞会话放行。
identity_check() {
  local body rc=0
  body="$(web_get /rest/login)" || rc=$?
  # curl --fail-with-body 在 401 时返回 22，body 照常输出
  if [[ $rc -ne 0 && $rc -ne 22 ]]; then echo "请求失败（rc=$rc）：$body"; return 1; fi
  grep -q '"status":"error"' <<<"$body" || { echo "期待应用自己的 401，得到：$(head -c 200 <<<"$body")"; return 1; }
  grep -q '"message":"Unauthorized"' <<<"$body" || { echo "401 内容不像应用回的：$(head -c 200 <<<"$body")"; return 1; }
}

_n8n_container() { # 本实例里 n8n 服务的容器（按核心打的标签找，smoke 和 dev probes 两个运行环境都成立）
  local inst="${SMOKE_INSTANCE:-}" args=(--filter "label=wonton.plugin=${SMOKE_PLUGIN_ID:-io.n8n.n8n}" --filter "label=wonton.service=n8n")
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

# 探针在容器里跑 CLI：清掉代理变量，数据库路径不读这些，避免 busybox/网络库误走出口代理。
_n8n_cli() { # <容器> <参数…>；-i 让 data_write 的管道进得了容器（docker exec 默认不接 stdin）
  local container="$1"; shift
  docker exec -i -e HOME=/home/node -e HTTP_PROXY= -e HTTPS_PROXY= -e http_proxy= -e https_proxy= \
    -e ALL_PROXY= -e all_proxy= "$container" "$@"
}

marker_file() { # SMOKE_STATE 在套件里是状态目录（e2e 传的是标记文件本身），标记放对应路径
  if [[ -d "$SMOKE_STATE" ]]; then printf '%s' "$SMOKE_STATE/n8n-marker"; else printf '%s' "${SMOKE_STATE}.marker"; fi
}

data_write() {
  local container marker
  [[ -n "${SMOKE_STATE:-}" ]] || { echo "SMOKE_STATE 没设"; return 1; }
  container="$(_n8n_container)"
  [[ -n "$container" ]] || { echo "找不到 n8n 容器（wonton.instance 标签）"; return 1; }
  marker="wonton-smoke-$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
  printf '[{"name":"%s","nodes":[{"parameters":{},"id":"1","name":"Start","type":"n8n-nodes-base.noOp","typeVersion":1,"position":[0,0]}],"connections":{},"active":false,"settings":{}}]' "$marker" |
    _n8n_cli "$container" n8n import:workflow --input=/dev/stdin >/dev/null ||
    { echo "导入工作流失败"; return 1; }
  printf '%s' "$marker" >"$(marker_file)"
}

data_check() {
  local container marker out
  container="$(_n8n_container)"
  [[ -n "$container" ]] || { echo "找不到 n8n 容器（wonton.instance 标签）"; return 1; }
  [[ -s "$(marker_file)" ]] || { echo "标记文件为空"; return 1; }
  marker="$(cat "$(marker_file)")"
  # 空库时 export:workflow 退出 1、不写文件：列表为空，标记必然找不到，如实报出来。
  out="$(_n8n_cli "$container" sh -c 'n8n export:workflow --all --output=/tmp/smoke-workflows.json >/dev/null 2>&1; cat /tmp/smoke-workflows.json 2>/dev/null')" ||
    { echo "导出工作流失败：$out"; return 1; }
  grep -qF "\"name\":\"$marker\"" <<<"$out" || { echo "工作流列表里没有标记 $marker（$(wc -c <<<"$out") 字节）：$(head -c 200 <<<"$out")"; return 1; }
}
