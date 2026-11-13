# Syncthing：sso=proxy，网关加 X-API-Key（核心生成的密钥，等于用户的凭据）；清单没有 mcp。
# shellcheck shell=bash disable=SC2034
# 应用没有“用户”概念，认的是这把只属于用户的密钥。

web_assert() { grep -qi 'syncthing'; }   # 标准输入：应用真实页面

identity_check() { # 自己的 REST 接口：网关带上用户的 API key 才 200，且返回本机身份
  local st mine anon
  st="$(st_rest GET system/status)" || { echo "/rest/system/status 失败：$st"; return 1; }
  mine="$(jq -r '.myID // empty' <<<"$st")"
  [[ "$mine" =~ ^[A-Z0-9-]{10,}$ ]] || { echo "status 没有 myID：$st"; return 1; }
  # 同一接口不带会话要被拒
  anon="$(curl -sS --noproxy '*' -o /dev/null -w '%{http_code}' "$SMOKE_WEB_URL/rest/system/status")"
  [[ "$anon" != 200 ]] || { echo "匿名 /rest/system/status 也是 200"; return 1; }
}

st_rest() { # <方法> <rest 路径> [curl 参数…]：带会话走网关调 Syncthing REST（网关注入 API key）
  local method="$1" path="$2"
  shift 2
  curl -sS --noproxy '*' --fail-with-body -b "${SMOKE_JAR:-}" -X "$method" -H 'Accept: application/json' \
    "$@" "$SMOKE_WEB_URL/rest/$path"
}

# 数据：配置和同步目录都是用户数据（配置在插件数据目录，重装保留）。走应用自己的配置接口：
# GET /rest/config 取全量，把 sync 文件夹的 label 改成随机标记再 PUT 回去，重装后读回核对。
data_write() {
  local cfg marker
  cfg="$(st_rest GET config)" || { echo "读不到 /rest/config：$cfg"; return 1; }
  jq -e '.folders | type == "array" and length >= 1 and any(.[]; .id == "sync")' <<<"$cfg" >/dev/null ||
    { echo "配置里没有 sync 文件夹：$(head -c 200 <<<"$cfg")"; return 1; }
  marker="wonton-smoke-$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
  cfg="$(jq --arg m "$marker" '(.folders[] | select(.id == "sync") | .label) = $m' <<<"$cfg")" || return 1
  st_rest PUT config -H 'Content-Type: application/json' -d "$cfg" >/dev/null ||
    { echo "PUT /rest/config 失败"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() {
  local marker cfg label
  marker="$(cat "$SMOKE_STATE" 2>/dev/null)" || { echo "没有 data_write 留下的标记"; return 1; }
  [[ -n "$marker" ]] || { echo "标记是空的"; return 1; }
  cfg="$(st_rest GET config)" || { echo "读不到 /rest/config：$cfg"; return 1; }
  label="$(jq -r '.folders[] | select(.id == "sync") | .label' <<<"$cfg")" || { echo "配置里没有 sync 文件夹"; return 1; }
  [[ "$label" == "$marker" ]] || { echo "sync 文件夹 label 对不上：期待 $marker，看到 $label"; return 1; }
}
