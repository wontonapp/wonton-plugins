# shellcheck shell=bash
# Obsidian LiveSync：CouchDB 通过局域网入口服务同步请求；clients.http 的 HTTPS 请求在开发栈手动验证。
# 探针直接对 CouchDB 发请求：不带账号要被拒；客户端账号能访问库；CORS 放行 Obsidian 桌面端；
# 数据探针写一个文档验证升级、重装不丢。
#
# 变量：SMOKE_STATE 是存数据标记的文件；SMOKE_JAR/SMOKE_WEB_URL 用不上（没有网关入口）。
# 请求从插件网络里的一次性 curl 容器发出（插件网络不出网，宿主连不进去）：
#   _container <插件 id> <服务名>  按核心标签找本实例里这个服务的容器
#   _curl <参数…>                 在插件网络里跑一次 curlimages/curl（镜像与 init 服务同一个）
_container() { # <插件 id> <服务名>：本实例里这个服务的容器
  local inst="${SMOKE_INSTANCE:-}" args=(--filter "label=wonton.plugin=$1" --filter "label=wonton.service=$2")
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  # init 是一次性服务，装完就退出：要能看到已退出的容器（docker ps -a）。
  docker ps -a --format '{{.Names}}' "${args[@]}" | head -1
}

_init_image() { # init 一次性服务用的镜像（和 compose 里的 curlimages/curl 同一个摘要）
  local ct; ct="$(_container "$PROBES_PLUGIN_ID" init)"
  [[ -n "$ct" ]] || { echo "找不到 init 容器"; return 1; }
  docker inspect --format '{{.Config.Image}}' "$ct"
}

_curl() { # <参数…>：对 couchdb 服务名发请求
  local img net
  img="$(_init_image)" || { echo "$img"; return 1; }
  net="$(docker inspect --format '{{range $k, $v := .NetworkSettings.Networks}}{{$k}} {{end}}' \
    "$(_container "$PROBES_PLUGIN_ID" couchdb)")"
  docker run --rm --network "${net%% *}" "$img" "$@" 2>&1
}

_client_creds() { # 客户端账号：init 容器 env 里的用户名和核心生成的密码（密码不回显）
  local ct
  ct="$(_container "$PROBES_PLUGIN_ID" init)"
  [[ -n "$ct" ]] || { echo "找不到 init 容器"; return 1; }
  docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "$ct" \
    | awk -F= '/^WONTON_OWNER_NAME=/{u=$2} /^WONTON_SECRET_CLIENT_/{p=substr($0, index($0,"=")+1)} END{print u "\t" p}'
}

data_write() {
  local unauth code origin creds user pw marker body
  echo "-- CouchDB /_up 与身份"

  unauth="$(_curl -s -o /dev/null -w '%{http_code}' http://couchdb:5984/_up)"
  [ "$unauth" = "401" ] || { echo "不带账号访问 /_up 得到 $unauth，要 401"; return 1; }
  echo "   不带账号访问 /_up：401"

  origin="$(_curl -s -D - -o /dev/null -X OPTIONS http://couchdb:5984/ \
    -H 'Origin: app://obsidian.md' -H 'Access-Control-Request-Method: PUT' \
    -H 'Access-Control-Request-Headers: content-type' | tr -d '\r' \
    | sed -n 's/^[Aa]ccess-[Cc]ontrol-[Aa]llow-[Oo]rigin: //p')"
  [ "$origin" = "app://obsidian.md" ] || { echo "CORS 预检没有放行 app://obsidian.md：${origin:-无}"; return 1; }
  echo "   CORS 预检放行 app://obsidian.md"

  creds="$(_client_creds)" || { echo "$creds"; return 1; }
  user="${creds%%$'\t'*}"
  pw="${creds#*$'\t'}"
  [[ -n "$user" && -n "$pw" ]] || { echo "init 容器里没有客户端账号信息"; return 1; }

  code="$(_curl -s -o /dev/null -w '%{http_code}' -u "$user:$pw" http://couchdb:5984/obsidiandb)"
  [ "$code" = "200" ] || { echo "客户端账号 $user 访问 obsidiandb 得到 $code，要 200"; return 1; }
  echo "   客户端账号 $user 访问 obsidiandb：200"

  marker="wonton-smoke-$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
  code="$(_curl -s -o /dev/null -w '%{http_code}' -u "$user:$pw" -X PUT "http://couchdb:5984/obsidiandb/smoke-probe" \
    -H 'Content-Type: application/json' -d "{\"marker\":\"$marker\"}")"
  [ "$code" = "201" ] || [ "$code" = "202" ] || { echo "写探针文档得到 $code"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
  echo "   写入文档 smoke-probe（标记 $marker）"
}

data_check() { # 重装、升级后文档还在，标记一致
  local marker body creds user pw got
  marker="$(cat "$SMOKE_STATE")"
  [[ -n "$marker" ]] || { echo "SMOKE_STATE 里没有标记"; return 1; }
  creds="$(_client_creds)" || { echo "$creds"; return 1; }
  user="${creds%%$'\t'*}"
  pw="${creds#*$'\t'}"
  body="$(_curl -s -u "$user:$pw" http://couchdb:5984/obsidiandb/smoke-probe)"
  got="$(jq -r '.marker // empty' <<<"$body" 2>/dev/null)"
  [ "$got" = "$marker" ] || { echo "文档里的标记是 ${got:-无}，要 $marker：$(head -c 200 <<<"$body")"; return 1; }
  echo "   重装后 smoke-probe 的标记还在：$got"
}
