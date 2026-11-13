# shellcheck shell=bash
# Navidrome：sso=proxy（Remote-User 代登录）；Subsonic 接口经网关；清单没有 mcp。

web_assert() { grep -qi 'navidrome'; }   # 标准输入：SPA 首页 HTML 含应用名

identity_check() { # Subsonic getUser：认出用户且是管理员
  local ping user
  ping="$(dweb "/rest/ping.view?v=1.16.1&c=wonton-smoke&f=json")" || { echo "ping 失败：$ping"; return 1; }
  grep -q '"status":"ok"' <<<"$ping" || { echo "ping 不是 ok：$ping"; return 1; }
  user="$(dweb "/rest/getUser.view?v=1.16.1&c=wonton-smoke&f=json&username=$SMOKE_OWNER")" ||
    { echo "getUser 失败：$user"; return 1; }
  jq -e --arg u "$SMOKE_OWNER" \
    '.["subsonic-response"].user.username == $u and .["subsonic-response"].user.adminRole == true' <<<"$user" >/dev/null ||
    { echo "getUser 不是用户的管理员账号：$user"; return 1; }
}

# 合并前验证升级、重装不丢用户数据：建一条带随机标记的私有歌单，标记存进 $SMOKE_STATE。
data_write() {
  local marker created
  marker="wonton-smoke-$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
  # 代登录要先走一次页面，应用才建出账号和自己会话 Cookie，之后带同一 Cookie 调 Subsonic 接口。
  dweb / -L >/dev/null || { echo "打开首页失败，建不了应用会话"; return 1; }
  created="$(dweb "/rest/createPlaylist.view?v=1.16.1&c=wonton-smoke&f=json&name=$marker")" ||
    { echo "建歌单失败：$created"; return 1; }
  jq -e '.["subsonic-response"].status == "ok"' <<<"$created" >/dev/null ||
    { echo "建歌单不是 ok：$created"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() { # getPlaylists 里还有那条歌单
  local marker got
  marker="$(cat "$SMOKE_STATE")"
  [[ -n "$marker" ]] || { echo "SMOKE_STATE 里没有标记"; return 1; }
  dweb / -L >/dev/null || { echo "打开首页失败，建不了应用会话"; return 1; }
  got="$(dweb "/rest/getPlaylists.view?v=1.16.1&c=wonton-smoke&f=json")" ||
    { echo "getPlaylists 失败：$got"; return 1; }
  jq -e --arg m "$marker" \
    '.["subsonic-response"].playlists.playlist // [] | if type == "array" then . else [.] end | any(.[]; .name == $m)' \
    <<<"$got" >/dev/null || { echo "歌单里找不到 $marker：$(head -c 300 <<<"$got")"; return 1; }
}
