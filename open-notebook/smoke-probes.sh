# shellcheck shell=bash
# scripts/smoke.sh / e2e/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# Open Notebook：sso=none，应用只有一把口令、没有用户体系，身份项 N/A；清单没有 mcp。
# 应用自己的口令在容器环境 OPEN_NOTEBOOK_PASSWORD（核心生成的密钥），REST API 用 Bearer 口令
# （上游 api/auth.py）；口令按核心打的标签从容器里读，冒烟和 e2e 两个环境都成立。
# 数据用"建一本笔记本"来写：POST /api/notebooks 建好后 GET /api/notebooks 能查到，
# 存在 SurrealDB（/mydata），升级/重装不动数据目录，所以原样保留。
export IDENTITY_NA=1
export SMOKE_NOTE="应用只有一把口令、没有用户体系，身份项 N/A；访问控制由网关把关"

ONB_CONTAINER() { # 本实例里插件服务的容器
  local inst="${SMOKE_INSTANCE:-}" args=(--filter "label=wonton.plugin=${SMOKE_PLUGIN_ID:-com.github.lfnovo.opennotebook}" --filter "label=wonton.service=notebook")
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

onb_password() { # 应用口令：从容器环境读核心生成的密钥
  local c
  c="$(ONB_CONTAINER)" || return 1
  [[ -n "$c" ]] || { echo "找不到插件的 notebook 容器"; return 1; }
  docker exec "$c" printenv OPEN_NOTEBOOK_PASSWORD 2>/dev/null
}

onb_session() { # 核心会话落在 SMOKE_JAR（网关认 cookie，应用自己只认口令）
  local page
  page="$(dweb --jar "$SMOKE_JAR" --fresh / -L)" || { echo "会话没建立起来：$(head -c 200 <<<"$page")"; return 1; }
}

onb_api() { # <方法> <路径> [<JSON>]：经核心网关调 REST API（网关会话 + 应用口令，和浏览器同一条路）
  local method="$1" path="$2" body="${3:-}" pw
  pw="$(onb_password)" || return 1
  [[ -n "$pw" ]] || { echo "容器里没有 OPEN_NOTEBOOK_PASSWORD"; return 1; }
  local args=(-sS --noproxy '*' --max-time 60 --fail-with-body -b "$SMOKE_JAR" -X "$method" \
    -H "Authorization: Bearer $pw" -H 'Content-Type: application/json')
  [[ -z "$body" ]] || args+=(--data "$body")
  curl "${args[@]}" "$SMOKE_WEB_URL$path" 2>&1
}

onb_state() { # 数据标记的路径：SMOKE_STATE 是文件（e2e harness）就用它，是状态目录（模板 smoke/probes、dev.sh probes）
  # 就在目录里放一个；两套套件给的同名变量含义不同，按一边写另一边 data_check 会报没数据。
  if [[ -d "$SMOKE_STATE" ]]; then printf '%s/open-notebook.env' "$SMOKE_STATE"; else printf '%s' "$SMOKE_STATE"; fi
}

web_assert() { grep -q 'Open Notebook'; }   # 标准输入：应用登录页的标题（各语言都是 "Open Notebook"）

identity_check() { :; }

data_write() { # 建一本带随机标记的笔记本，id 和名字写进数据标记
  local name resp id state
  onb_session || return 1
  name="云吞冒烟 $(date +%Y%m%d%H%M%S)"
  resp="$(onb_api POST /api/notebooks "$(jq -cn --arg n "$name" '{name:$n,description:"smoke marker"}')")" ||
    { echo "建笔记本失败：$resp"; return 1; }
  id="$(jq -r '.id // empty' <<<"$resp" 2>/dev/null)" || true
  [[ -n "$id" ]] || { echo "建笔记本响应没有 id：$(head -c 200 <<<"$resp")"; return 1; }
  state="$(onb_state)"
  printf 'ONB_NOTEBOOK_ID=%q\nONB_NOTEBOOK_NAME=%q\n' "$id" "$name" >"$state" ||
    { echo "写不进数据标记 $state"; return 1; }
}

data_check() { # 升级/重装后读回 data_write 建的笔记本，id 和名字对得上
  local resp id name got_id got_name state
  state="$(onb_state)"
  [[ -f "$state" && -s "$state" ]] || { echo "数据标记里没有数据（$state）：先跑 data_write"; return 1; }
  # shellcheck source=/dev/null
  source "$state"
  onb_session || return 1
  resp="$(onb_api GET "/api/notebooks/$ONB_NOTEBOOK_ID")" || { echo "查笔记本失败：$resp"; return 1; }
  got_id="$(jq -r '.id // empty' <<<"$resp" 2>/dev/null)" || true
  got_name="$(jq -r '.name // empty' <<<"$resp" 2>/dev/null)" || true
  [[ "$got_id" == "$ONB_NOTEBOOK_ID" ]] || { echo "笔记本 id 对不上：期望 $ONB_NOTEBOOK_ID，实际 $got_id"; return 1; }
  [[ "$got_name" == "$ONB_NOTEBOOK_NAME" ]] || { echo "笔记本名字对不上：期望 $ONB_NOTEBOOK_NAME，实际 $got_name"; return 1; }
}
