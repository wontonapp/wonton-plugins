# shellcheck shell=bash
# babybuddy：sso=proxy（网关注入 Remote-User，应用按它认人、没有的用户自动建号）。清单没有 mcp。
# 应用的 /api/ 不认代理头（上游 CustomRemoteUser 对 /api/ 直接放行），靠应用自己的会话：
# 先带核心会话开一次应用页面，Django 才建会话（每页都带 csrftoken）。
# 数据：写一条带随机标记的育儿记录（没有宝宝就先建一个），标记和写入时间存进 $SMOKE_STATE；
# data_check 按时间过滤后逐条比对标记。

web_assert() { grep -q 'Baby Buddy'; }   # 每页 <title> 都带上游名

_bb_origin() { # 网页入口可能带路径，拼应用接口前取 origin
  local u="${SMOKE_WEB_URL%%\?*}" rest="${SMOKE_WEB_URL#*://}"
  printf '%s://%s' "${u%%://*}" "${rest%%/*}"
}

_bb_session() { # jar 里备好核心会话（-b 保留 jar 里已有的 cookie，只写 -c 会把别的域的会话抹掉）
  grep -q 'sessionid' "${SMOKE_JAR:-/dev/null}" 2>/dev/null && return 0
  dweb / -L >/dev/null || { echo "核心会话建立失败"; return 1; }
  grep -q 'sessionid' "${SMOKE_JAR:-/dev/null}" 2>/dev/null ||
    { echo "应用没建会话（网关没带 Remote-User？）"; return 1; }
}

_bb_api() { # <方法> <路径> [curl 参数…]：带用户会话调应用接口，正文到标准输出
  local method="$1" path="$2"
  shift 2
  curl -sS --noproxy '*' --max-time "$SMOKE_WEB_MAX_TIME" --fail-with-body -b "$SMOKE_JAR" -c "$SMOKE_JAR" \
    -X "$method" "$(_bb_origin)$path" "$@"
}

# 会话认证的写请求要带 CSRF 头；jar 里同名 cookie 有多份时，Django 读请求里最后出现的那份，
# 所以挑域最长的（应用子域），不用核心域的那份。
_csrf() {
  awk -F'\t' '$6=="csrftoken" {
    d = $1; sub(/^#HttpOnly_/, "", d)
    if (length(d) > length(best)) best = $7
  } END {print best}' "${SMOKE_JAR:?}"
}

identity_check() { # 应用自己的“当前用户”接口认出的是主人，且是后台用户
  local profile
  _bb_session || return 1
  profile="$(_bb_api GET /api/profile)" || { echo "profile 接口失败：$(head -c 200 <<<"$profile")"; return 1; }
  jq -e --arg u "$SMOKE_OWNER" '.user.username == $u and .user.is_staff == true' <<<"$profile" >/dev/null ||
    { echo "profile 不是 $SMOKE_OWNER 的后台用户：$(head -c 200 <<<"$profile")"; return 1; }
}

data_write() { # 记一条备注（带随机标记）；没有宝宝时先建一个，标记和写入时间存进 $SMOKE_STATE
  local marker time child csrf out
  _bb_session || return 1
  marker="probe-$(date +%s)-$RANDOM"
  # Z 结尾：DRF 的 ISO 解析认它，也不用担心 + 在查询串里被解成空格。
  time="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  out="$(_bb_api GET /api/children/)" || { echo "列不出宝宝：$(head -c 200 <<<"$out")"; return 1; }
  child="$(jq -r '.results[0].id // empty' <<<"$out")"
  csrf="$(_csrf)"
  [[ -n "$csrf" ]] || { echo "会话里没有 csrftoken"; return 1; }
  if [[ -z "$child" ]]; then
    out="$(_bb_api POST /api/children/ -H 'Content-Type: application/json' -H "X-CSRFToken: $csrf" \
      --data-raw '{"first_name":"宝宝","birth_date":"2026-01-01"}')" ||
      { echo "建宝宝失败：$(head -c 200 <<<"$out")"; return 1; }
    child="$(jq -r '.id // empty' <<<"$out")"
    [[ -n "$child" ]] || { echo "建宝宝没成：$(head -c 200 <<<"$out")"; return 1; }
  fi
  out="$(_bb_api POST /api/notes/ -H 'Content-Type: application/json' -H "X-CSRFToken: $csrf" \
    --data-raw "{\"child\":$child,\"note\":\"$marker\",\"time\":\"$time\"}")" ||
    { echo "写记录失败：$(head -c 200 <<<"$out")"; return 1; }
  jq -e --arg m "$marker" '.note == $m' <<<"$out" >/dev/null ||
    { echo "写记录没成：$(head -c 200 <<<"$out")"; return 1; }
  # 接口不支持按内容搜记录，把写入时间一起存下，data_check 按时间过滤。
  jq -nc --arg m "$marker" --arg t "$time" '{marker:$m,time:$t}' >"$SMOKE_STATE"
}

data_check() { # 按写入时间列记录，找标记
  local marker time found
  marker="$(jq -r .marker "$SMOKE_STATE")"
  time="$(jq -r .time "$SMOKE_STATE")"
  [[ -n "$marker" && "$marker" != null && -n "$time" && "$time" != null ]] ||
    { echo "SMOKE_STATE 里没有标记（data_write 没跑过？）"; return 1; }
  _bb_session || return 1
  found="$(_bb_api GET "/api/notes/?date_min=$time")" || { echo "列记录失败：$(head -c 200 <<<"$found")"; return 1; }
  jq -e --arg m "$marker" '[.results[].note] | index($m) != null' <<<"$found" >/dev/null ||
    { echo "找不到标记 $marker：$(head -c 300 <<<"$found")"; return 1; }
}
