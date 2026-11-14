# shellcheck shell=bash
# scripts/smoke.sh / e2e/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# AstrBot：sso=none，登录走应用自己的账号（用户名 astrbot + 插件密钥里的初始密码，
# init 一次性服务把它按上游格式写进 data/cmd_config.json）。探针经核心网关请求，
# 应用自己的登录用 AstrBot 的登录接口，凭据从插件容器环境读（WONTON_SECRET_DASHBOARD_PASSWORD）。
export SMOKE_NOTE="应用自己的登录：astrbot + 插件密钥里的初始密码，经网关转发后登录"

# 页面：AstrBot 面板的首页是单页应用，标题和资源路径都带 astrbot。
web_assert() { grep -qiE 'astrbot'; }

_app_container() { # <服务名>：本实例里这个服务的容器（smoke 与 e2e 两个环境都成立）
	local inst="${SMOKE_INSTANCE:-}" args=(--filter "label=wonton.plugin=${SMOKE_PLUGIN_ID:-app.astrbot.chatbot}" --filter "label=wonton.service=$1")
	if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
		inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
	fi
	[[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
	docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

astrbot_login() { # 用插件密钥里的初始密码走应用自己的登录，stdout 只有 curl 参数，报错走 stderr
	local body token
	body="$(curl -sS --noproxy '*' --max-time 60 -b "$SMOKE_JAR" -H 'Content-Type: application/json' \
		--data "$(jq -cn --arg p "$(docker exec "$(_app_container astrbot)" printenv WONTON_SECRET_DASHBOARD_PASSWORD)" \
			'{username:"astrbot",password:$p}')" \
		"${SMOKE_WEB_ORIGIN:-${SMOKE_WEB_URL}}/api/auth/login")" || { echo "登录请求失败" >&2; return 1; }
	token="$(jq -r '.data.token // empty' <<<"$body" 2>/dev/null)" || true
	[[ -n "$token" ]] || { echo "登录没拿到 token：$(head -c 200 <<<"$body")" >&2; return 1; }
	printf -- '-H\nAuthorization: Bearer %s\n' "$token"
}

identity_check() { # setup_required=false 说明 init 写的密码已生效且没有强制改密流程；再用它实际登录一次
	local status
	astrbot_session || return 1
	status="$(dweb /api/auth/setup-status)" || { echo "setup-status 请求失败"; return 1; }
	grep -q '"setup_required":false' <<<"$status" ||
		{ echo "面板仍要求初始化设置（setup_required 不是 false）：$(head -c 200 <<<"$status")"; return 1; }
	astrbot_login >/dev/null || { echo "应用自己的登录失败"; return 1; }
}

# 数据：人格是面板里建的用户数据，走面板同款接口（POST /api/v1/personas）写一条带随机标记的人格，
# 升级/重装后按 persona_id 读回，system_prompt 一字不差。init 只动 dashboard 密码、网关供应商
# 和 mcp_server.json 里的 wonton 条目，不动人格。
astrbot_env() {
	local name
	for name in SMOKE_JAR SMOKE_WEB_URL SMOKE_STATE; do
		[[ -n "${!name:-}" ]] || { echo "套件没有给 \$$name"; return 1; }
	done
}

astrbot_state() { # SMOKE_STATE 是文件（e2e）就用它，是目录（dev.sh probes）就在里面放一个
	if [[ -d "$SMOKE_STATE" ]]; then printf '%s/astrbot-data.env' "$SMOKE_STATE"; else printf '%s' "$SMOKE_STATE"; fi
}

astrbot_session() { # 核心会话落在 SMOKE_JAR，并确认网关放行（sso=none：网关认 cookie，应用自己认账号）
	local page
	page="$(dweb --jar "$SMOKE_JAR" --fresh /)" || { echo "会话没建立起来：$(head -c 200 <<<"$page")"; return 1; }
}

astrbot_api() { # <curl 参数…> <路径>：带核心会话和应用登录态请求面板接口
	local -a args auth
	local login_args
	args=("$@")
	local path="${args[$# - 1]}"
	args=("${args[@]:0:$# - 1}")
	login_args="$(astrbot_login)" || return 1
	mapfile -t auth <<<"$login_args"
	curl -sS --noproxy '*' --max-time 120 -b "$SMOKE_JAR" "${auth[@]}" "${args[@]}" \
		"${SMOKE_WEB_ORIGIN:-${SMOKE_WEB_URL}}${path}"
}

data_write() {
	local name resp id state
	astrbot_env || return 1
	astrbot_session || return 1
	name="云吞冒烟 $(date +%Y%m%d%H%M%S)"
	resp="$(astrbot_api -H 'Content-Type: application/json' \
		--data "$(jq -cn --arg id "$name" '{persona_id:$id,system_prompt:("冒烟标记:"+$id)}')" \
		-X POST /api/v1/personas)" || { echo "建人格失败：$resp"; return 1; }
	id="$(jq -r '.data.persona.persona_id // empty' <<<"$resp" 2>/dev/null)" || true
	[[ -n "$id" ]] || { echo "建人格响应不是预期的 JSON（没有 data.persona.persona_id）：$(head -c 200 <<<"$resp")"; return 1; }
	state="$(astrbot_state)"
	printf 'ASTRBOT_PERSONA_ID=%q\n' "$id" >"$state" ||
		{ echo "写不进数据标记 $state"; return 1; }
}

data_check() {
	local resp got state
	astrbot_env || return 1
	state="$(astrbot_state)"
	[[ -f "$state" && -s "$state" ]] || { echo "数据标记里没有数据（$state）：先跑 data_write"; return 1; }
	# shellcheck source=/dev/null
	. "$state"
	[[ -n "${ASTRBOT_PERSONA_ID:-}" ]] || { echo "数据标记不完整：$state"; return 1; }
	astrbot_session || return 1
	local quoted
	quoted="$(python3 -c "import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1],safe=''))" "$ASTRBOT_PERSONA_ID")" ||
		{ echo "编码人格 id 失败"; return 1; }
	resp="$(astrbot_api "/api/v1/personas/$quoted")" ||
		{ echo "读人格失败：$resp"; return 1; }
	got="$(jq -r --arg id "$ASTRBOT_PERSONA_ID" --arg prompt "冒烟标记:$ASTRBOT_PERSONA_ID" '
		if .status=="ok" and .data.persona_id==$id and .data.system_prompt==$prompt then "ok" else "no" end' <<<"$resp" 2>/dev/null)" || true
	[[ "$got" == "ok" ]] || {
		echo "人格 $ASTRBOT_PERSONA_ID 读不回来或内容对不上，接口拿到：$(head -c 200 <<<"$resp")"
		return 1
	}
}
