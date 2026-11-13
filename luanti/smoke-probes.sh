# shellcheck shell=bash
# Luanti 没有网页和 MCP：web、身份、工具三项由套件按清单记 N/A。
# 用户数据就是数据卷里的世界（${WONTON_APP_DATA}/worlds/<游戏名>/）。
# 引擎没有能从外面写数据的接口（控制台要终端，游戏协议要完整客户端），
# 所以数据探针把随机标记写进服务器容器里挂载的世界目录：升级、重装后标记还在，
# 就证明世界目录随数据卷保住了（容器里 exec 写的文件和引擎写世界用的是同一个挂载）。

_container() { # <插件 id> <服务名>：本实例里这个服务的容器
  local inst="${SMOKE_INSTANCE:-}" args=(--filter "label=wonton.plugin=$1" --filter "label=wonton.service=$2")
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

_world_dir() { # 服务器容器里现存的第一个世界目录
  local c
  c="$(_container "org.luanti.luanti" server)" || return 1
  [[ -n "$c" ]] || { echo "找不到 server 容器"; return 1; }
  docker exec "$c" sh -c 'find /var/lib/minetest/worlds -maxdepth 1 -type d ! -path /var/lib/minetest/worlds | head -1'
}

data_write() {
  local marker world
  marker="wonton-smoke-$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
  world="$(_world_dir)" || { echo "找不到世界目录：$world"; return 1; }
  local c
  c="$(_container "org.luanti.luanti" server)"
  docker exec "$c" sh -c "printf %s '$marker' > '$world/wonton-smoke-marker'" ||
    { echo "往 $world 写标记失败"; return 1; }
  printf '%s' "$marker" >"$(marker_file)"
}

data_check() { # 重装后标记还在，内容一致
  local marker world got
  marker="$(cat "$(marker_file)")"
  [[ -n "$marker" ]] || { echo "SMOKE_STATE 里没有标记"; return 1; }
  world="$(_world_dir)" || { echo "找不到世界目录：$world"; return 1; }
  local c
  c="$(_container "org.luanti.luanti" server)"
  got="$(docker exec "$c" cat "$world/wonton-smoke-marker" 2>&1)" ||
    { echo "读不回标记：$got"; return 1; }
  [[ "$got" == "$marker" ]] || { echo "标记对不上：期望 $marker，读到 $got"; return 1; }
}

marker_file() { # SMOKE_STATE 在套件里是状态目录（e2e 传的是标记文件本身），标记放对应路径
  if [[ -d "$SMOKE_STATE" ]]; then printf '%s' "$SMOKE_STATE/luanti-marker"; else printf '%s' "${SMOKE_STATE}.marker"; fi
}
