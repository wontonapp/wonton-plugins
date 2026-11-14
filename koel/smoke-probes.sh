# shellcheck shell=bash
# Koel：sso=none——登录用应用自己的邮箱账号。init 服务把上游写死的默认管理员（admin@koel.dev /
# KoelIsCool）的登录邮箱改成主人的内部邮箱，身份探测就用内部邮箱登录；密码默认 KoelIsCool。
# 网页和应用接口都经核心网关（请求要带用户会话）；应用自己的接口再加 Koel 的 Bearer 令牌。

web_assert() { grep -qi 'koel'; }   # 标准输入：SPA 首页 HTML

koel_login() { # 登录主人账号，输出 Bearer 令牌
  local resp
  resp="$(curl -sS --noproxy '*' -b "$SMOKE_JAR" -H 'Content-Type: application/json' \
    -d "{\"email\":\"${SMOKE_OWNER}@wonton.internal\",\"password\":\"KoelIsCool\"}" "$SMOKE_WEB_URL/api/me")" ||
    { echo "登录请求失败"; return 1; }
  local token
  token="$(jq -r '.token // empty' <<<"$resp")"
  [[ -n "$token" ]] || { echo "登录没有返回 token：$resp"; return 1; }
  printf '%s' "$token"
}

identity_check() { # 应用自己的当前用户接口：认出的是管理员
  local token me
  token="$(koel_login)" || return 1
  me="$(curl -sS --noproxy '*' -b "$SMOKE_JAR" -H "Authorization: Bearer $token" -H 'Accept: application/json' \
    "$SMOKE_WEB_URL/api/me")" || { echo "GET /api/me 失败：$me"; return 1; }
  jq -e '.is_admin == true and .role == "admin"' <<<"$me" >/dev/null ||
    { echo "当前用户不是管理员：$me"; return 1; }
}

data_write() { # 建一个带随机标记的歌单，把标记原样写进 $SMOKE_STATE（覆盖写）
  local token resp marker
  marker="wonton-smoke-$(date +%s)-$RANDOM"
  token="$(koel_login)" || return 1
  resp="$(curl -sS --noproxy '*' -b "$SMOKE_JAR" -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d "{\"name\":\"$marker\"}" "$SMOKE_WEB_URL/api/playlists")" || { echo "建歌单失败：$resp"; return 1; }
  jq -e --arg m "$marker" '(if type == "object" and has("data") then .data else . end).name == $m' <<<"$resp" >/dev/null ||
    { echo "建歌单回显对不上：$resp"; return 1; }
  printf '%s' "$marker" > "$SMOKE_STATE"
}

data_check() { # 从 $SMOKE_STATE 读回标记，去应用里找回带标记的歌单
  local token playlists marker
  marker="$(cat "$SMOKE_STATE" 2>/dev/null)"
  [[ -n "$marker" ]] || { echo "没有标记或标记为空"; return 1; }
  token="$(koel_login)" || return 1
  playlists="$(curl -sS --noproxy '*' -b "$SMOKE_JAR" -H "Authorization: Bearer $token" -H 'Accept: application/json' \
    "$SMOKE_WEB_URL/api/playlists")" || { echo "列歌单失败：$playlists"; return 1; }
  jq -e --arg m "$marker" '(if type == "array" then . else (.data // [.]) end) | map(.name) | index($m)' <<<"$playlists" >/dev/null ||
    { echo "歌单里找不到标记 $marker：$(head -c 300 <<<"$playlists")"; return 1; }
}

# ---------- 邮件（notify）：找回密码的信经核心内部邮箱投进主人私聊 ----------
# Koel 免费版里能触发的发信只有找回密码（邀请是 Koel Plus 的功能）；Laravel 的密码broker 同步发信，
# SMTP 连不上时请求直接 500，所以 204 就是「信已交给核心的 SMTP」。成功投递在核心日志里静默，
# 以主人身份读 Matrix 私聊里的 app.wonton.plugin_mail 消息为证。发往不存在的外部用户时 Koel
# 出于防枚举根本不创建邮件（无 SMTP 往返），外部 550 拒收在这条触发路径上不可观测，不写假断言。
extra_check() {
  local koel_container koel_env mail_host mail_port mail_from owner_email token code room_out wp
  koel_container="$(docker ps --format '{{.Names}}' | grep -E "wonton-${SMOKE_INSTANCE}--dev-koel-koel--koel$" | head -1)"
  [[ -n "$koel_container" ]] || { echo "找不到 koel 容器"; return 1; }

  # 0) 三个 SMTP 变量进了 koel 容器：MAIL_* 不是进程环境变量，是启动脚本写进 /var/www/html/.env 的
  koel_env="$(docker exec "$koel_container" cat /var/www/html/.env)" || { echo "读不到 koel 容器的 .env"; return 1; }
  mail_host="$(sed -n 's/^MAIL_HOST=//p' <<<"$koel_env")"
  mail_port="$(sed -n 's/^MAIL_PORT=//p' <<<"$koel_env")"
  mail_from="$(sed -n 's/^MAIL_FROM_ADDRESS=//p' <<<"$koel_env")"
  [[ "$mail_host" == "smtp.wonton.internal" && "$mail_port" == "25" &&
    "$mail_from" == "dev.koel.koel@plugins.wonton.internal" ]] ||
    { echo "SMTP 变量不对：host=$mail_host port=$mail_port from=$mail_from"; return 1; }
  echo "SMTP 变量已注入：$mail_host:$mail_port from=$mail_from"

  # 1) 触发找回密码（发给主人账号的内部地址；Laravel 同步发信，失败会 500）。
  # init 把主人账号的登录邮箱改成 <主人>@wonton.internal（见文件头），找回密码信就发往这个核心收得到的地址。
  owner_email="${SMOKE_OWNER}@wonton.internal"
  token="$(koel_login)" || { echo "koel 登录失败"; return 1; }

  # 2) 以主人身份读主人的 Wonton 私聊，等这封信（投递成功在核心日志里静默）。
  # 等待先挂后台再触发：信可能在等待开始前就到，套件的房间等待只认开始之后的消息。
  room_out="$(mktemp)"
  probes_owner_room_wait 150 "Koel" >"$room_out" &
  wp=$!
  code="$(curl -sS --noproxy '*' -o /dev/null -w '%{http_code}' -b "${SMOKE_JAR:-}" \
    -H 'Content-Type: application/json' -H 'Accept: application/json' -H "Authorization: Bearer $token" \
    -d "{\"email\":\"$owner_email\"}" \
    "$SMOKE_WEB_URL/api/forgot-password")" || code=""
  if [[ "$code" != 204 ]]; then
    kill "$wp" 2>/dev/null
    wait "$wp" 2>/dev/null
    rm -f "$room_out"
    { echo "找回密码请求失败或没成功（HTTP ${code:-curl 失败}；Laravel 同步发信失败会 500）"; return 1; }
  fi
  echo "找回密码信已交给核心 SMTP（HTTP 204）"
  if wait "$wp"; then
    cat "$room_out"
    rm -f "$room_out"
  else
    { echo "Koel 的找回密码信没进主人的 Wonton 私聊：$(cat "$room_out")"; rm -f "$room_out"; return 1; }
  fi
}
