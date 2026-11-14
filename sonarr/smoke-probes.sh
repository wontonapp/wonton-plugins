# shellcheck shell=bash
# 冒烟（scripts/smoke.sh 与各处端到端冒烟）共用的插件探针。
# 钩子可用面：变量 SMOKE_STATE（数据标记，冒烟是文件、dev.sh probes 是目录）、
# SMOKE_JAR、SMOKE_WEB_URL、SMOKE_OWNER、SMOKE_BASE、SMOKE_TOKEN、SMOKE_INSTANCE；助手 web_get、dweb。
# Sonarr：sso=none + 应用侧认证设 External（应用内无登录、无用户体系）；
# API key 由插件的 secrets 注入（SONARR__AUTH__APIKEY），/initialize.json 直接给出。
# 两个变量由套件 probes.sh source 本文件后读取，shellcheck 看不到那处使用。
# shellcheck disable=SC2034
IDENTITY_NA=1
# shellcheck disable=SC2034
SMOKE_NOTE="应用侧认证设 External，无应用内用户体系；网页身份由云吞网关会话保证"

web_assert() { grep -qi '<title>Sonarr'; }   # 标准输入：SPA 首页 HTML 的标题

sonarr_marker() { # 标记文件：SMOKE_STATE 是文件就用它，是目录就放里面（dev.sh probes 的 .wonton）
  if [[ -n "${SMOKE_STATE:-}" && -d "$SMOKE_STATE" ]]; then
    printf '%s/sonarr-data' "$SMOKE_STATE"
  elif [[ -n "${SMOKE_STATE:-}" ]]; then
    printf '%s' "$SMOKE_STATE"
  else
    echo "SMOKE_STATE 没设置"; return 1
  fi
}

sonarr_marker_set() { # <键> <值>：改写标记里的一项，保留其它行
  local m
  m="$(sonarr_marker)" || return 1
  if [[ -f "$m" ]] && grep -q "^$1=" "$m"; then
    sed -i "s/^$1=.*/$1=$2/" "$m"
  else
    printf '%s=%s\n' "$1" "$2" >>"$m"
  fi
}

sonarr_marker_get() { # <键>：标记里的单项
  local m
  m="$(sonarr_marker)" || return 1
  [[ -f "$m" ]] || return 1
  sed -n "s/^$1=//p" "$m" | tail -1
}

sonarr_web() { # 网页入口：套件没导出 SMOKE_WEB_URL 时（dev.sh probes 单跑数据项）从核心地址和插件 id 拼
  if [[ -z "${SMOKE_WEB_URL:-}" ]]; then
    local base="${E2E_WEB_URL:-${SMOKE_BASE:?SMOKE_BASE 未导出}}"
    base="${base%/}"
    SMOKE_WEB_URL="${base%%://*}://${SMOKE_PLUGIN_ID//./-}.${base#*://}"
  fi
  printf '%s' "$SMOKE_WEB_URL"
}

sonarr_key() { # 应用的 X-Api-Key：External 认证没有登录页，initialize.json 直接给
  local page key
  sonarr_web >/dev/null
  page="$(dweb /initialize.json)" || { echo "读 /initialize.json 失败"; return 1; }
  key="$(jq -r '.apiKey // empty' <<<"$page" 2>/dev/null)"
  [[ -n "$key" && "$key" != null ]] || { echo "initialize.json 里没有 apiKey"; return 1; }
  printf '%s' "$key"
}

sonarr_session() { # 自铸核心会话：网关认出用户才转发 sso:none 应用的请求。端到端冒烟的 extra_check 和
  # 升级保数据的 data_write 都跑在套件 web_login 之前，jar 里还没有核心会话，先用主人令牌换一个。
  local web jar token base
  web="${E2E_WEB_URL:-$(sonarr_web)}"
  web="${web%/}"
  jar="${E2E_JAR:-${SMOKE_JAR:?SMOKE_JAR 未导出}}"
  token="${E2E_TOKEN:-${SMOKE_TOKEN:-}}"
  base="${E2E_BASE:-${SMOKE_BASE:-}}"
  [[ -n "$token" && -n "$base" ]] || { echo "没有核心登录变量，铸不了核心会话"; return 1; }
  curl -sS --noproxy '*' -o /dev/null -c "$jar" \
    --data-urlencode "token=$token" "$base/_wonton/login" ||
    { echo "主人令牌登录不了核心"; return 1; }
  # 第二步：跟首页跳转，让网关给这个应用子域名发会话 Cookie（只铸核心会话不够）。
  curl -fsS --noproxy '*' -o /dev/null -L -b "$jar" -c "$jar" \
    -H 'Accept: text/html' "$web/" ||
    { echo "应用网页会话没建立"; return 1; }
}

sonarr_api() { # <方法> <路径> [请求体]：经网关调应用 API，带 X-Api-Key
  local method="$1" path="$2" body="${3:-}" key args=()
  key="$(sonarr_key)" || return 1
  args+=(-X "$method" -H "X-Api-Key: $key")
  [[ -n "$body" ]] && args+=(-H 'Content-Type: application/json' -d "$body")
  web_get "/api/v3/$path" "${args[@]}"
}

sonarr_api_ready() { # 等 API 就绪，就绪后把 GET system/status 的结果打到标准输出
  local out
  sonarr_session || return 1
  for _ in $(seq 1 20); do
    if out="$(sonarr_api GET system/status)"; then
      printf '%s' "$out"
      return 0
    fi
    sleep 3
  done
  echo "Sonarr 的 API 一直没就绪"
  return 1
}

SONARR_TAG_LABEL_PREFIX="wonton-smoke"

data_write() { # 建一个标签（改数据、写库），作为要保住的用户数据
  local status list old id created
  sonarr_web >/dev/null # 命令替换是子壳，网页入口要在父壳备好
  status="$(sonarr_api_ready)" || return 1
  # External 认证下应用不再拦人；顺手确认环境变量真的把认证方式设成了 external。
  jq -e '.authentication == "external"' <<<"$status" >/dev/null ||
    { echo "应用认证方式不是 external：$(jq -r '.authentication' <<<"$status")"; return 1; }
  list="$(sonarr_api GET tag)" || { echo "读标签失败：$(head -c 200 <<<"$list")"; return 1; }
  # 幂等：先删掉上次留下的同名标签，再建一个新的（带随机标记）。
  local label
  label="${SONARR_TAG_LABEL_PREFIX}-$(head -c 8 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  old="$(jq -r --arg n "$SONARR_TAG_LABEL_PREFIX" '[.[] | select(.label | startswith($n))][0].id // empty' <<<"$list")"
  if [[ -n "$old" ]]; then
    sonarr_api DELETE "tag/$old" >/dev/null || { echo "删旧标签失败（id=$old）"; return 1; }
  fi
  created="$(sonarr_api POST tag "{\"label\":\"$label\"}")" ||
    { echo "建标签失败：$(head -c 200 <<<"$created")"; return 1; }
  id="$(jq -r '.id // empty' <<<"$created")"
  [[ -n "$id" ]] || { echo "建标签后读不到 id：$(head -c 200 <<<"$created")"; return 1; }
  sonarr_marker_set label "$label" || { echo "数据标记写不进 $SMOKE_STATE"; return 1; }
  echo "已建标签 $label（id=$id）"
}

data_check() { # 升级/重装后：标记里的标签还在
  local label list item tries
  label="$(sonarr_marker_get label)"
  [[ -n "$label" ]] || { echo "标记里没有数据（data_write 没跑？）"; return 1; }
  sonarr_web >/dev/null # 命令替换是子壳，网页入口要在父壳备好
  sonarr_session || return 1
  # 应用冷启动的头几秒 API 可能先 401，照 data_write 的就绪模式重试；要的是标签列表，不是就绪探测的状态。
  for tries in 1 2 3 4 5 6 7 8 9 10; do
    if list="$(sonarr_api GET tag)" && jq -e 'type == "array"' <<<"$list" >/dev/null 2>&1; then break; fi
    [[ "$tries" == 10 ]] && { echo "读标签失败：$(head -c 200 <<<"$list")"; return 1; }
    sleep 3
  done
  item="$(jq -c --arg l "$label" '[.[] | select(.label == $l)][0] // empty' <<<"$list")"
  [[ -n "$item" && "$item" != null ]] || { echo "标签 $label 不见了：$(head -c 200 <<<"$list")"; return 1; }
  echo "标签 $label 还在（id=$(jq -r '.id' <<<"$item")）"
}

# ---------- 发信（notify，规范 0.11）----------
sn_core() { # 核心容器：通知房记录和 Matrix 令牌都在它的数据目录/网络上
	docker ps --format '{{.Names}}' | grep -xE "wonton-${SMOKE_INSTANCE:-}--core" | head -1
}

sn_dd() { # 核心数据目录（端到端冒烟有 read_env；dev.sh probes 用 probes_setting）
	if declare -F read_env >/dev/null; then read_env WONTON_DATA_DIR; else probes_setting WONTON_DATA_DIR; fi
}

sn_room_wait() { # <超时秒> <要找的文字>：等主人的「Wonton」私聊出现这段文字。用套件 probes.sh 的
	# probes_owner_room_wait，在独立进程里跑，套件的 pass/fail 不盖进冒烟自己的 shell。
	local core dd pdir
	core="$(sn_core)" || return 1
	dd="$(sn_dd)"
	pdir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
	# shellcheck disable=SC2016 # 单引号里是子进程 bash 的变量，$1/$2/$3 是 bash -c 的位置参数
	env SMOKE_CORE="$core" SMOKE_DATA_DIR="$dd" SMOKE_OWNER="${SMOKE_OWNER:-owner}" \
		bash -c 'date() { [[ "${1:-}" == "+%s" ]] && command date -d "@$(( $(command date +%s) - 300 ))" +%s || command date "\$@"; }; source "$1/lib.sh" && source "$1/probes.sh" && probes_owner_room_wait "$2" "$3"' \
		_ "$pdir" "$1" "$2"
}

extra_check() { # 发信端到端：应用的「测试」接口真发一封通知信（装插件时 mail 已建好 Wonton 连接），
	# 核心把它投进主人的「Wonton」私聊（直接读通知房的消息，投递的最终证据）。
	local list conn resp to wp
	to="${SMOKE_OWNER:-owner}@wonton.internal"
	sonarr_web >/dev/null # 命令替换是子壳，网页入口要在父壳备好
	sonarr_session || return 1
	# 插件的一次性 mail 服务建好的 "Wonton" Email 通知连接还在：没有它就没有发信通路。
	# 应用冷启动的头几秒 API 可能先 401，照 data_write 的就绪模式重试。
	local tries
	for tries in 1 2 3 4 5; do
		list="$(sonarr_api GET notification)" && break
		[[ "$tries" == 5 ]] && { echo "读通知连接失败：$(head -c 200 <<<"$list")"; return 1; }
		sleep 3
	done
	conn="$(jq -c '[.[] | select(.implementation == "Email")][0] // empty' <<<"$list")"
	[[ -n "$conn" && "$conn" != null ]] || { echo "通知连接里没有 Email 实现（插件的 mail 服务没建好？）"; return 1; }
	jq -e --arg h "smtp.wonton.internal" '[.fields[] | select(.name == "server")][0].value == $h' \
		<<<"$conn" >/dev/null 2>&1 ||
		echo "（提示）Email 连接的 server 不是 smtp.wonton.internal：$(jq -c '[.fields[] | select(.name == "server" or .name == "to")]' <<<"$conn")"
	# 等待先挂后台再触发（模板的用法）：信进私聊是秒级投递，先等后发才等得到。
	sn_room_wait 120 "Sonarr" & wp=$!
	# 触发一封真实测试信：应用用它自己配好的 SMTP（内部邮箱）发给收件人。
	resp="$(sonarr_api POST notification/test "$conn")" ||
		{ kill "$wp" 2>/dev/null; wait "$wp" 2>/dev/null; echo "测试信请求失败：$(head -c 200 <<<"$resp")"; return 1; }
	# 核心把信投进主人的「Wonton」私聊：房里出现这一封。
	wait "$wp" || { echo "测试信 120 秒没出现在主人的 Wonton 私聊（房里没这封信）"; return 1; }
	echo "测试信已进主人的 Wonton 私聊（Sonarr：…，收件人 $to）"
}
