# shellcheck shell=bash
# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# 页面：web_assert 从标准输入读页面，命中的确是应用的真实页面时返回 0。
# 身份：identity_check 用应用自己的“当前用户”接口断言认出的是用户管理员。没有用户体系时写 IDENTITY_NA=1，
#       并在 SMOKE_NOTE 里写原因。dweb / web_get 是带用户会话的请求助手，可继续走应用自己的登录链。
# 工具：MCP_TOOL 写一个只读工具名（可带插件前缀），MCP_ARGS 是 JSON 参数；mcp_assert 从标准输入读调用结果。
# 数据：data_write 用随机标记写一条数据并把凭据和标记覆盖写进 $SMOKE_STATE，data_check 读回来在
#       应用里核对。两套 harness（scripts/smoke.sh、e2e/smoke.sh）都跑这两项；这里的 data_* 只用
#       套件保证的变量和助手（$SMOKE_STATE、$SMOKE_JAR、$SMOKE_WEB_URL、$SMOKE_OWNER）。
#
# Cloudreve：sso=none；清单没有 mcp。首次打开是登录/注册页，首个注册的账号自动升为管理员
# （inventory/user.go 的 Create：first user registered → admin group），所以身份探测断言的是
# 应用自己报告的“注册开放”——注册入口只在网关后面，谁先注册谁是管理员。

web_assert() { grep -qi 'cloudreve'; }   # SPA 首页 HTML（标题和资源路径都带应用名）

identity_check() { # 应用自己的登录配置：注册开放，首个注册账号即管理员
  local cfg
  # dweb（dev.sh web）两套 harness 都保证：会话和 URL 拼接它自己处理，不用 SMOKE_JAR/SMOKE_WEB_URL。
  cfg="$(dweb /api/v4/site/config/login -H 'Accept: application/json')" ||
    { echo "GET /api/v4/site/config/login 失败：$cfg"; return 1; }
  jq -e '.data.register_enabled == true' <<<"$cfg" >/dev/null ||
    { echo "注册没开放，第一次打开没法建管理员：$cfg"; return 1; }
}

# 数据钩子的账号：邮箱固定（主人名 @wonton-smoke.local，本实例收不到信，够登录绑定用），密码每轮
# 随机，凭据覆盖写 $SMOKE_STATE。应用返回 HTTP 200 + JSON code，错误要看 .code。
# SMOKE_* 变量由 harness 在钩子运行前保证，这里在钩子里取值，不在 source 时求值。
cr_data_email() { printf '%s@wonton-smoke.local' "${SMOKE_OWNER:-owner}"; }
cr_data_stash() { printf '%s' "${SMOKE_STATE:?SMOKE_STATE 没设}"; }

cr_app_api() { # <方法> <路径> [curl 参数…]：带主人会话调应用自己的 API
  local m="$1" p="$2" base="${SMOKE_WEB_URL%/}"
  shift 2
  curl -sS --noproxy '*' --fail-with-body -b "$SMOKE_JAR" -X "$m" "$@" "$base$p"
}

cr_app_token() { # 登录应用（BuiltinLoginResponse），输出 access_token；$1 密码
  local r
  r="$(cr_app_api POST /api/v4/session/token -H 'Content-Type: application/json' \
    -d "{\"email\":\"$(cr_data_email)\",\"password\":\"$1\"}")" || return 1
  jq -er '.data.token.access_token // empty' <<<"$r"
}

cr_upload() { # <access_token> <标记>：建上传会话、传分片（内容含标记），分片到齐即完成
  local at="$1" marker="$2" body sid tmp size raw
  tmp="$(mktemp)"; printf 'wonton smoke marker: %s\n' "$marker" >"$tmp"
  size="$(wc -c <"$tmp")"
  body="$(jq -nc --arg uri "cloudreve://my/wonton-smoke-data/wonton-smoke-$marker.txt" --argjson size "$size" '{uri:$uri,size:$size}')"
  body="$(cr_app_api PUT /api/v4/file/upload -H 'Content-Type: application/json' -H "Authorization: Bearer $at" -d "$body")" ||
    { rm -f "$tmp"; echo "建上传会话失败：$body"; return 1; }
  sid="$(jq -er '.data.session_id // empty' <<<"$body")" ||
    { rm -f "$tmp"; echo "上传会话没有 session_id：$body"; return 1; }
  raw="$(cr_app_api POST "/api/v4/file/upload/$sid/0" -H 'Content-Type: application/octet-stream' \
    -H "Authorization: Bearer $at" --data-binary @"$tmp")" ||
    { rm -f "$tmp"; echo "上传分片失败：$raw"; return 1; }
  jq -e '.code == 0' <<<"$raw" >/dev/null || { rm -f "$tmp"; echo "上传分片被拒：$raw"; return 1; }
  rm -f "$tmp"
}

data_write() { # 注册首账号（即管理员），上传带随机标记的文件，凭据和标记覆盖写 $SMOKE_STATE
  local marker pass at r
  marker="$(head -c8 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  pass="$(head -c24 /dev/urandom | base64 | tr -d '/+=')"
  r="$(cr_app_api POST /api/v4/user -H 'Content-Type: application/json' \
    -d "{\"email\":\"$(cr_data_email)\",\"password\":\"$pass\"}")" ||
    { echo "注册首账号请求失败：$r"; return 1; }
  jq -e '.code == 0' <<<"$r" >/dev/null || { echo "注册首账号被拒：$r"; return 1; }
  at="$(cr_app_token "$pass")" || { echo "注册后登录失败"; return 1; }
  cr_upload "$at" "$marker" || return 1
  printf 'cr_pass=%s\ncr_marker=%s\n' "$pass" "$marker" >"$(cr_data_stash)"
}

data_check() { # 登录、列目录、取下载地址，核对文件和内容里的标记
  local pass marker at listing dl content
  pass="$(sed -n 's/^cr_pass=//p' "$(cr_data_stash)" | tail -1)"
  marker="$(sed -n 's/^cr_marker=//p' "$(cr_data_stash)" | tail -1)"
  [[ -n "$pass" && -n "$marker" ]] || { echo "SMOKE_STATE 里没有数据钩子的凭据，data_write 跑过吗"; return 1; }
  at="$(cr_app_token "$pass")" || { echo "升级/重装后登录不上（账号没了？）"; return 1; }
  listing="$(cr_app_api GET "/api/v4/file?uri=cloudreve://my/wonton-smoke-data&page_size=100" -H "Authorization: Bearer $at")" ||
    { echo "列目录失败：$listing"; return 1; }
  jq -e --arg m "$marker" 'any(.data.files[]?; .name == "wonton-smoke-" + $m + ".txt")' \
    <<<"$listing" >/dev/null ||
    { echo "目录里找不到 wonton-smoke-$marker.txt：$(head -c 300 <<<"$listing")"; return 1; }
  dl="$(cr_app_api POST /api/v4/file/url -H 'Content-Type: application/json' -H "Authorization: Bearer $at" \
    -d "{\"uris\":[\"cloudreve://my/wonton-smoke-data/wonton-smoke-$marker.txt\"],\"download\":true}")" ||
    { echo "取下载地址失败：$dl"; return 1; }
  dl="$(jq -er '.data.urls[0].url // empty' <<<"$dl")" || { echo "下载地址为空：$dl"; return 1; }
  [[ "$dl" == /* ]] && dl="${SMOKE_WEB_URL%/}$dl"
  content="$(curl -sS --noproxy '*' --fail-with-body -b "$SMOKE_JAR" "$dl")" ||
    { echo "下载文件失败：$content"; return 1; }
  grep -qF "wonton smoke marker: $marker" <<<"$content" ||
    { echo "文件内容对不上（marker 丢了）：$(head -c 200 <<<"$content")"; return 1; }
}
