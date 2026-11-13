# shellcheck shell=bash
# scripts/smoke.sh 和 e2e/smoke.sh 的插件探测。
# changedetection.io 没有用户体系：上游默认无访问口令，网页只经云吞网关进来，身份由网关会话把守。
# 数据探针走应用自己的快速添加表单（POST /form/add）写一条 watch——REST API 默认开密钥校验，
# 令牌存在容器数据里，钩子契约里拿不到；data_check 断言这条 watch 的 URL 还在总览页。

# shellcheck disable=SC2034  # 由开发套件 probes.sh 与 e2e/smoke.sh source 后读取
IDENTITY_NA=1
# shellcheck disable=SC2034  # 同上：SMOKE_NOTE 给 probes.sh，E2E_NOTE 给 e2e/smoke.sh
SMOKE_NOTE="应用没有用户体系，也没有应用登录（上游默认无口令）：能打开网页就只有网关认过的主人，身份以网页特征命中为证"
# shellcheck disable=SC2034  # 同上
E2E_NOTE="$SMOKE_NOTE"

web_assert() { grep -qiE 'new-watch-form|add-watch-url-row'; }   # 标准输入：watch 总览页

# 核心会话：数据钩子和邮件检查跑的时候套件的会话可能还没建或已失效（网关对插件路径一律 401），
# 照 audiobookshelf/bookstack 探针的做法自己登一次。
cd_session() {
  local base="${E2E_BASE:-${SMOKE_BASE:-}}" token="${E2E_TOKEN:-${SMOKE_TOKEN:-}}"
  [[ -n "$base" && -n "$token" && -n "${SMOKE_JAR:-}" ]] || { echo "没有核心地址、主人令牌或 jar"; return 1; }
  curl -sS --noproxy '*' -o /dev/null -b "$SMOKE_JAR" -c "$SMOKE_JAR" \
    --data-urlencode "token=$token" "$base/_wonton/login" ||
    { echo "核心会话登录不了"; return 1; }
  # 插件网页的应用会话要经网关的 SSO 跳转链兑换（_wonton/session），curl -L 自动走完
  curl -sS --noproxy '*' -o /dev/null -L -b "$SMOKE_JAR" -c "$SMOKE_JAR" -H 'Accept: text/html' \
    "${SMOKE_WEB_URL%/}/" || { echo "走不通插件网页的登录跳转"; return 1; }
}

# data_write：经表单建 watch，URL 带唯一标记并覆盖写进 SMOKE_STATE。
# 应用全局开 CSRFProtect：先从总览页拿 csrf_token；首查后标题会换成网页标题，
# 但 URL 原样渲染在总览页的 favicon 链接（open_link）里，data_check 靠它断言。
data_write() {
  local ts tok page body
  cd_session || return 1
  ts="$(date +%s)"
  if ! page="$(web_get / 2>&1)"; then
    echo "data_write：拿不到总览页：$(head -c 200 <<<"$page")"
    return 1
  fi
  tok="$(sed -n 's/.*name="csrf_token" value="\([^"]*\)".*/\1/p' <<<"$page" | head -1)"
  [[ -n "$tok" ]] || { echo "data_write：总览页里没有 csrf_token"; return 1; }
  if ! body="$(web_get /form/add/quickwatch --data-urlencode "csrf_token=$tok" --data-urlencode "url=https://example.com/?wonton-smoke-$ts" -L 2>&1)"; then
    echo "data_write：POST /form/add/quickwatch 失败：$(head -c 200 <<<"$body")"
    return 1
  fi
  printf '%s' "$ts" >"$SMOKE_STATE"
  echo "data_write：建了 watch wonton-smoke-$ts，标记写入 SMOKE_STATE"
}

data_check() {
  local ts page
  cd_session || return 1
  ts="$(cat "$SMOKE_STATE" 2>/dev/null)"
  [[ -n "$ts" ]] || { echo "data_check：SMOKE_STATE 里没有 data_write 写的标记"; return 1; }
  if ! page="$(web_get / 2>&1)"; then
    echo "data_check：拿不到总览页：$(head -c 200 <<<"$page")"
    return 1
  fi
  grep -q "wonton-smoke-$ts" <<<"$page" ||
    { echo "data_check：总览页里没有 wonton-smoke-$ts（watch 丢了）"; return 1; }
}

# ---------- 内部邮箱（notify）的端到端检查 ----------

_mp_api() { # <路径>：调 Mailpit 的 HTTP API（basic core:<api-password>）。Mailpit 只挂在插件网络，
  # 宿主够不着；照核心自己 smoke-mailbox 的做法，在核心容器的网络栈里起一次性 curl 访问它。
  local core pw dd out
  core="wonton-${SMOKE_INSTANCE:?}--core"
  docker inspect "$core" >/dev/null 2>&1 || { echo "找不到核心容器 $core"; return 1; }
  if declare -F read_env >/dev/null; then dd="$(read_env WONTON_DATA_DIR)"
  elif declare -F probes_setting >/dev/null; then dd="$(probes_setting WONTON_DATA_DIR)"
  else echo "读不到 WONTON_DATA_DIR（read_env / probes_setting 都没有）"; return 1; fi
  pw="$(docker exec "$core" cat "$dd/core/mailpit/api-password" 2>/dev/null)"
  [[ -n "$pw" ]] || { echo "读不到 Mailpit API 密码（核心数据目录 core/mailpit/api-password）"; return 1; }
  out="$(docker run --rm --network "container:$core" curlimages/curl:8.23.0 \
    -s -u "core:$pw" "http://mailpit.wonton.internal:8025$1" 2>&1)"
  if [[ "$out" == "{"* ]]; then printf '%s' "$out"; return 0; fi
  echo "Mailpit 没给出 JSON：$(head -c 200 <<<"$out")"
  return 1
}

_mp_wait() { # <jq 参数…> [轮数=20]：轮询 Mailpit 信箱直到过滤命中。参数原样交给 jq（--arg 键 值、过滤串）；
  # 最后一个参数是纯数字时取作轮数。
  local -a args=()
  local n=20 a i out
  while (($#)); do
    a="$1"; shift
    if (($# == 0)) && [[ "$a" =~ ^[0-9]+$ ]]; then n="$a"; else args+=("$a"); fi
  done
  ((${#args[@]})) || { echo "mp_wait 没给 jq 过滤"; return 1; }
  for ((i = 0; i < n; i++)); do
    out="$(_mp_api "/api/v1/messages?limit=100")" || { sleep 3; continue; }
    jq -e "${args[@]}" <<<"$out" >/dev/null 2>&1 && return 0
    sleep 3
  done
  return 1
}

# 核心把信投进主人的私聊后马上从内部邮箱删掉（投完才删），等“信出现在 Mailpit”有竞态：
# 直接断言投递的结果——以主人身份读主人的 Wonton 私聊（通知房间），找这个插件发的信。

_mx_curl() { # <路径> [curl 参数…]：在核心容器的网络栈里调 Tuwunel 的 Matrix 客户端接口（宿主够不着）
  local path="$1"; shift
  docker run --rm --network "container:wonton-${SMOKE_INSTANCE:?}--core" curlimages/curl:8.23.0 \
    -sS --noproxy '*' --max-time 30 --fail-with-body "$@" "http://tuwunel.wonton.internal:8008$path"
}

_mx_room_mail() { # <插件 id>：以主人身份读主人的 Wonton 私聊，输出这个插件发进来的信（结构化事件数组）
  local core="wonton-${SMOKE_INSTANCE:?}--core" admin server room owner tok enc
  admin="$(docker exec "$core" sh -c 'cat "$WONTON_DATA_DIR/core/matrix-admin-token"')" ||
    { echo "拿不到核心的 Matrix 管理令牌"; return 1; }
  server="$(docker exec "$core" sh -c 'cat "$WONTON_MATRIX_SERVER_NAME_FILE" 2>/dev/null || printf %s "${WONTON_MATRIX_SERVER_NAME:-}"')"
  [[ -n "$server" ]] || { server="${SMOKE_BASE#*://}"; server="${server%%:*}"; }
  room="$(docker exec "$core" sh -c 'cat "$WONTON_DATA_DIR/core/matrix-notice-room" 2>/dev/null')" ||
    { echo "主人的通知房间还没建（还没投过信？）"; return 1; }
  enc="$(python3 -c 'import sys,urllib.parse;print(urllib.parse.quote(sys.argv[1],safe=""))' "$room")"
  owner="@${SMOKE_OWNER:-owner}:$server"
  tok="$(_mx_curl "/_synapse/admin/v1/users/$owner/login" \
    -H "Authorization: Bearer $admin" -H 'Content-Type: application/json' -d '{}' | jq -er '.access_token // empty')" ||
    { echo "换不出主人的 Matrix 会话（impersonate 失败）"; return 1; }
  _mx_curl "/_matrix/client/v3/rooms/$enc/messages?dir=b&limit=30" -H "Authorization: Bearer $tok" |
    jq -c --arg id "$1" '[.chunk[] | select(.content["app.wonton.plugin_mail"].plugin_id == $id)]'
}

_mx_wait_mail() { # <插件 id> [轮数=30]：轮询主人的私聊，等这封信出现在房间里（每轮 2 秒）
  local id="$1" n="${2:-30}" i chunk
  for ((i = 0; i < n; i++)); do
    chunk="$(_mx_room_mail "$id")" || { sleep 2; continue; }
    [[ "$chunk" != "[]" ]] && { printf '%s' "$chunk"; return 0; }
    sleep 2
  done
  return 1
}

_owner_email() { # 主人资料里的内部邮箱（全局通知 URL 的收件人，不猜域名）
  local base token me
  base="${E2E_BASE:-${SMOKE_BASE:-}}"; token="${E2E_TOKEN:-${SMOKE_TOKEN:-}}"
  [[ -n "$base" && -n "$token" ]] || { echo "没有核心地址或主人令牌"; return 1; }
  me="$(curl -sS --noproxy '*' --fail-with-body -H "Authorization: Bearer $token" "$base/_wonton/api/me")" ||
    { echo "读不到主人资料（/_wonton/api/me）"; return 1; }
  jq -er '.email // empty' <<<"$me" || { echo "主人资料里没有邮箱：$me"; return 1; }
}

extra_check() { # 内部邮箱端到端：一次性初始化设好的全局邮件通知真发信，核心投进主人私聊的私聊里断言
  local appc mailinit marker to page tok rc
  cd_session || return 1
  appc="$(docker ps --format '{{.Names}}' | grep -E -- "-changedetection--changedetection$|changedetection-changedetection-1$" | head -1)"
  [[ -n "$appc" ]] || { echo "找不到 changedetection 应用容器"; return 1; }
  mailinit="$(docker ps -a --format '{{.Names}}' | grep -E -- "-changedetection--mail-init$|changedetection-mail-init-1$" | head -1)"
  [[ -n "$mailinit" ]] || { echo "找不到 mail-init 一次性容器"; return 1; }
  # 一次性初始化的结论在它自己的日志里（标记文件在应用的数据卷里）
  docker logs "$mailinit" 2>&1 | grep -q "全局邮件通知已设为 mailto://" ||
    { echo "mail-init 日志里没有设好的通知 URL：$(docker logs "$mailinit" 2>&1 | tail -2)"; return 1; }
  docker exec "$appc" sh -c 'test -f /datastore/.wonton-mail-configured' ||
    { echo "datastore 里没有 .wonton-mail-configured 标记"; return 1; }
  echo "mail-init 已把全局邮件通知设到核心内部邮箱"

  to="$(_owner_email)" || { echo "$to"; return 1; }
  page="$(web_get /)" || { echo "拿不到总览页：$(head -c 200 <<<"$page")"; return 1; }
  tok="$(sed -n 's/.*name="csrf_token" value="\([^"]*\)".*/\1/p' <<<"$page" | head -1)"
  [[ -n "$tok" ]] || { echo "总览页里没有 csrf_token"; return 1; }
  # send-test 要求至少有一个 watch（没有就先建一个）；global-settings 模式下通知 URL 要放在表单里
  #（端点不读存储的全局列表），mail-init 的标记文件里存的就是设好的 mailto:// URL
  local mailurl reply
  mailurl="$(docker exec "$appc" cat /datastore/.wonton-mail-configured)" ||
    { echo "读不到 mail-init 设的通知 URL（标记文件）"; return 1; }
  marker="wonton-mail-$(date +%s)"
  web_get /form/add/quickwatch --data-urlencode "csrf_token=$tok" \
    --data-urlencode "url=https://example.com/?$marker" -L -o /dev/null ||
    { echo "建 watch 失败"; return 1; }
  reply="$(web_get "/notification/send-test?mode=global-settings" --data-urlencode "csrf_token=$tok" \
    --data-urlencode "notification_urls=$mailurl" 2>&1)" || { echo "POST /notification/send-test 失败：$reply"; return 1; }
  grep -q "OK - Sent test notifications" <<<"$reply" ||
    { echo "测试通知没发出去，端点回了：$(head -c 200 <<<"$reply")"; return 1; }
  if ! mail="$(_mx_wait_mail io.changedetection)"; then
    echo "60 秒内没在主人的 Wonton 私聊看到 changedetection 的告警测试信；内部邮箱现在：$(_mp_api '/api/v1/messages?limit=10' 2>/dev/null | jq -c '[.messages[] | {to: [.To[].Address], subject}]' 2>/dev/null | head -c 300)"
    echo "房间读取原始输出：$(_mx_room_mail io.changedetection 2>&1 | head -c 400)"
    return 1
  fi
  echo "告警测试信已投进 $to 的 Wonton 私聊"
}
