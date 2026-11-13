# shellcheck shell=bash
# RomM 5.4.1：OIDC 会话由套件建立，探针只通过 RomM 自己的页面和 API 验证。

romm_session() {
	dweb --sso / >/dev/null || { echo "RomM 的 OIDC 登录链走不通" >&2; return 1; }
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

data_write() {
	local marker response
	marker="$(head -c 12 /dev/urandom | base64 | tr -d '=+/' | head -c 16)"
	[[ -n "$marker" ]] || { echo "生成 RomM 数据标记失败"; return 1; }
	romm_session || return 1
	response="$(curl -sS --noproxy '*' --max-time 60 --fail-with-body \
		-b "$SMOKE_JAR" -F "name=wonton-smoke-$marker" -F "description=$marker" \
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
