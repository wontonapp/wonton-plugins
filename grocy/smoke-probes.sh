# shellcheck shell=bash
# grocy：sso=proxy（网关注入 Remote-User，grocy 按它认人、没有的用户自动建号）。清单没有 mcp。
# 代登录下应用没有自己的登录页可走：核心会话 Cookie 就是全部——dweb --sso 从插件入口出发把核心
# 登录页跟完，之后同一 jar 的请求经网关都会带上 Remote-User。数据钩子可能跑在第一次会话建立之前，
# 每个钩子都先备一次核心会话。
# 数据：往购物清单写一条带随机标记的备注项（grocy 的 /api/objects/<实体> 通用接口），data_check 读回。

web_assert() { grep -qi '<title>[^<]*grocy'; }   # 每页 <title> 都带上游名

grocy_session() { # 核心会话就绪（已建立时原地返回）
  if [[ -s "${SMOKE_JAR:-}" ]] && grep -q 'wonton' "$SMOKE_JAR" 2>/dev/null; then return 0; fi
  dweb --sso / >/dev/null 2>&1 || { echo "核心会话建立失败"; return 1; }
}

identity_check() { # 应用自己的“当前用户”接口认出的是主人
  local resp
  grocy_session || return 1
  resp="$(web_get /api/user -H 'Accept: application/json' --fail-with-body)" ||
    { echo "GET /api/user 失败：$(head -c 200 <<<"$resp")"; return 1; }
  jq -e --arg u "$SMOKE_OWNER" 'type == "array" and length == 1 and .[0].username == $u' <<<"$resp" >/dev/null ||
    { echo "当前用户不是 $SMOKE_OWNER：$(head -c 300 <<<"$resp")"; return 1; }
}

data_write() { # 在购物清单写一条带随机标记的备注项，标记覆盖写进 $SMOKE_STATE
  local marker resp
  grocy_session || return 1
  marker="wonton-smoke-$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
  resp="$(web_get /api/objects/shopping_list -X POST -H 'Content-Type: application/json' \
    --data-raw "{\"note\":\"$marker\"}" --fail-with-body)" ||
    { echo "建购物清单项失败：$(head -c 200 <<<"$resp")"; return 1; }
  jq -e '.created_object_id' <<<"$resp" >/dev/null 2>&1 ||
    { echo "购物清单项没建成：$(head -c 200 <<<"$resp")"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() { # 按标记读回购物清单项
  local marker resp
  marker="$(cat "$SMOKE_STATE" 2>/dev/null)" || true
  [[ -n "$marker" ]] || { echo "SMOKE_STATE 是空的（data_write 没跑过？）"; return 1; }
  grocy_session || return 1
  resp="$(web_get /api/objects/shopping_list -H 'Accept: application/json' --fail-with-body)" ||
    { echo "读购物清单失败：$(head -c 200 <<<"$resp")"; return 1; }
  jq -e --arg n "$marker" 'type == "array" and any(.[]; .note == $n)' <<<"$resp" >/dev/null ||
    { echo "标记项不见了，清单内容：$(head -c 300 <<<"$resp")"; return 1; }
}
