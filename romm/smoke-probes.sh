# shellcheck shell=bash
# RomM 5.4.1：OIDC 会话由套件建立，探针只通过 RomM 自己的页面和 API 验证。

# 网页里的登录是前端 JS 做的：GET / 只回 SPA 空壳，套件的 dweb --sso（从 webui.path 出发、提交自动表单）
# 停在空壳上，jar 里没有应用会话。应用的服务端登录入口是 /api/login/openid，它 302 到核心的 OIDC
# 登录页；dweb 每次都先用用户令牌登录核心，所以带着核心会话 GET 它就能跟完授权码回调，落下 romm_session。
romm_session() {
	local out
	# 失败时把响应体带出来：应用拒登时是 {"detail": ...} 的 JSON（如旧版清单缺 groups 时的 403）。
	out="$(dweb /api/login/openid -L)" ||
		{ echo "RomM 的 OIDC 登录链走不通：$(head -c 200 <<<"$out" | tr '\n' ' ')"; return 1; }
}

web_assert() {
	grep -Eqi '<title[^>]*>[^<]*RomM|RomM[^<]*</title>'
}

identity_check() {
	local me
	romm_session || return 1
	me="$(curl -sS --noproxy '*' --max-time 60 --fail-with-body \
		-b "$SMOKE_JAR" "$SMOKE_WEB_ORIGIN/api/users/me")" || {
		echo "RomM 当前用户接口失败：$me"
		return 1
	}
	jq -e --arg owner "$SMOKE_OWNER" \
		'.username == $owner and .role == "admin" and .enabled == true' <<<"$me" >/dev/null || {
		echo "RomM 当前用户不是启用的管理员 $SMOKE_OWNER：$(head -c 300 <<<"$me")"
		return 1
	}
}

# 写操作要过 RomM 的双提交 CSRF：cookie romm_csrftoken 与请求头 x-csrftoken 必须是同一个用户绑定的令牌
# （代码里叫 CSRF token verification failed 的 403 就是它）。令牌随登录后的请求落进 jar，读出来原样带上。
romm_csrf() {
	local token
	curl -sS --noproxy '*' --max-time 60 -b "$SMOKE_JAR" -c "$SMOKE_JAR" -o /dev/null \
		"$SMOKE_WEB_ORIGIN/api/users/me" || { echo "刷新 CSRF 令牌失败"; return 1; }
	token="$(awk '$6 == "romm_csrftoken" { print $7 }' "$SMOKE_JAR" | tail -1)"
	[[ -n "$token" ]] || { echo "jar 里没有 romm_csrftoken"; return 1; }
	printf '%s' "$token"
}

data_write() {
	local marker response token
	marker="$(head -c 12 /dev/urandom | base64 | tr -d '=+/' | head -c 16)"
	[[ -n "$marker" ]] || { echo "生成 RomM 数据标记失败"; return 1; }
	romm_session || return 1
	token="$(romm_csrf)" || return 1
	response="$(curl -sS --noproxy '*' --max-time 60 --fail-with-body \
		-b "$SMOKE_JAR" -H "x-csrftoken: $token" \
		-F "name=wonton-smoke-$marker" -F "description=$marker" \
		"$SMOKE_WEB_ORIGIN/api/collections")" || {
		echo "RomM 创建持久化收藏夹失败：$response"
		return 1
	}
	jq -e --arg marker "$marker" \
		'.name == ("wonton-smoke-" + $marker) and .description == $marker and (.id | type) == "number"' \
		<<<"$response" >/dev/null || {
		echo "RomM 收藏夹创建响应不含预期数据：$(head -c 300 <<<"$response")"
		return 1
	}
	printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() {
	local marker collections
	marker="$(cat "$SMOKE_STATE")"
	[[ -n "$marker" ]] || { echo "RomM 数据标记为空"; return 1; }
	romm_session || return 1
	collections="$(curl -sS --noproxy '*' --max-time 60 --fail-with-body \
		-b "$SMOKE_JAR" "$SMOKE_WEB_ORIGIN/api/collections")" || {
		echo "RomM 收藏夹查询失败：$collections"
		return 1
	}
	jq -e --arg marker "$marker" --arg owner "$SMOKE_OWNER" \
		'any(.[]; .name == ("wonton-smoke-" + $marker) and .description == $marker and .owner_username == $owner)' \
		<<<"$collections" >/dev/null || {
		echo "RomM 没找回标记为 $marker 的收藏夹：$(head -c 500 <<<"$collections")"
		return 1
	}
}
