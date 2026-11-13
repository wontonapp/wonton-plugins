# shellcheck shell=bash
# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# 钩子可用面：变量 SMOKE_STATE（数据标记：文件，或 dev.sh probes 的目录）、
# SMOKE_JAR、SMOKE_WEB_URL、SMOKE_OWNER、SMOKE_BASE、SMOKE_TOKEN、SMOKE_INSTANCE；助手 web_get、dweb。
# AutoBangumi：sso=none + 应用自带登录。上游在首次启动固定建出厂账号 admin/adminadmin
# （源码 module/database/user.py，应用没有改初始密码的配置项），插件 about 已写明首登改密。
# identity_check 用出厂账号走应用自己的登录，断言应用认出的当前用户；数据断言交给 data_write / data_check。
web_assert() { grep -q '<title>AutoBangumi'; }   # 标准输入：SPA 首页 HTML 的标题

ab_login() { # 出厂账号登录应用，会话 Cookie 落进 dweb 的会话 jar；成功回显应用响应
	dweb /api/v1/auth/login -X POST \
		-H 'Content-Type: application/x-www-form-urlencoded' \
		--data 'username=admin&password=adminadmin'
}

identity_check() { # 应用自己的“当前用户”接口认出登录的账号
	local me
	ab_login >/dev/null || { echo "应用登录失败（出厂账号 admin）"; return 1; }
	me="$(web_get /api/v1/auth/me)" || { echo "读不到当前用户：$me"; return 1; }
	grep -q '"username":"admin"' <<<"$me" ||
		{ echo "应用认出的不是出厂管理员：$(head -c 200 <<<"$me")"; return 1; }
}

# ---------- 工具（MCP，规范 0.11 起清单声明）----------
# shellcheck disable=SC2034  # MCP_TOOL、MCP_ARGS 由 scripts/probes.sh 读取
MCP_TOOL="get_program_status"   # 只读：返回 {version, running, first_run}
# shellcheck disable=SC2034
MCP_ARGS="{}"

# ---------- 依赖提供方（before_install，冒烟专用）----------
# 本插件 requires torrent_client，提供方是 qBittorrent 插件；它上游（geraldpeng6/wonton-plugin-qbittorrent）
# 还没声明 provides，商店也没有带声明的版本，冒烟实例里 requires 没有提供方就装不上。
# 这里取上游真实包，补上 provides 一行、去掉与本验证无关的 lan 端口段（51413 在共用主机上会撞别人的栈），
# 装进冒烟实例。提供方声明落地后整个函数删掉。
before_install() {
	local tmp
	if probes_dev api GET /_wonton/api/plugins/org.qbittorrent.qbittorrent >/dev/null 2>&1; then
		return 0
	fi
	tmp="$(mktemp -d)"
	if ! curl -fsSL --max-time 120 "https://codeload.github.com/geraldpeng6/wonton-plugin-qbittorrent/tar.gz/refs/heads/main" |
		tar -xz -C "$tmp"; then
		echo "取不到 qbittorrent 上游包，requires 没有提供方可装"; return 1
	fi
	mv "$tmp/wonton-plugin-qbittorrent-main" "$tmp/src"
	python3 - "$tmp/src/docker-compose.yml" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
if "provides:" not in s:
    old = 'x-wonton:\n  spec: "'
    assert s.count(old) == 1
    nl = s.index("\n", s.index(old) + len(old))
    s = s[: nl + 1] + "  provides:\n    torrent_client: { service: qbittorrent, port: 8080 }\n" + s[nl + 1 :]
# 去掉整段 lan:（子项缩进 ≥ 4；下一个恰好 2 空格的键结束这一段）。
out = []
skip = False
for line in s.split("\n"):
    if line == "  lan:":
        skip = True
        continue
    if skip:
        if not line.strip() or line.startswith("    "):
            continue
        skip = False
    out.append(line)
# lan.ports 去掉后，成对的 network.lan 权限也去掉，check 要求两者要么都有要么都没有。
s = "\n".join(out)
s = "\n".join(line for line in s.split("\n") if line.strip() != "- network.lan")
open(p, "w").write(s)
PY
	local image
	image="$(sed -n 's/^WONTON_CORE_IMAGE=//p' "$SMOKE_DEV_STATE/.env" | tail -1)"
	[[ -n "$image" ]] || { echo "冒烟实例没写 WONTON_CORE_IMAGE，打不了提供方包"; return 1; }
	if ! docker run --rm --network none -v "$tmp/src:/plugin:ro" "$image" plugin pack /plugin >"$tmp/pkg.tar" 2>"$tmp/err"; then
		echo "提供方打包失败：$(tail -c 200 "$tmp/err")"; return 1
	fi
	local grants
	grants="$(probes_dev api POST /_wonton/api/plugins/preview -H 'Content-Type: application/x-tar' --data-binary @"$tmp/pkg.tar" 2>/dev/null |
		python3 -c "import json,sys; print(','.join(l['permission'] for l in json.load(sys.stdin)['permissions']))")" ||
		{ echo "提供方预览失败"; return 1; }
	probes_dev api POST "/_wonton/api/plugins?grant=$grants" -H 'Content-Type: application/x-tar' --data-binary @"$tmp/pkg.tar" >/dev/null ||
		{ echo "提供方安装失败"; return 1; }
	probes_status_wait org.qbittorrent.qbittorrent 300 >/dev/null ||
		{ echo "提供方装上后 300 秒内没有 running healthy"; return 1; }
	rm -rf "$tmp"
}

# AB 的 MCP 服务器是上游自带的，tools/list 不带 annotations（readOnlyHint 全缺），套件默认的
# 「按 annotations 挑只读工具」走不通；总线按规范把没有 readOnlyHint 的调用放进确认卡片等主人批准。
# 这里走真实链路：发起调用、用主人令牌批准本插件的卡片、断言结果；不绕确认，也不代上游补 annotations。
mcp_cards_to_approve() { # 确认卡片 JSON：挑出想用本插件 MCP_TOOL 的卡，每行「id<TAB>回答体」
	MCP_CARDS="$1" MCP_TOOL_SUFFIX="的 ${MCP_TOOL}" python3 - <<'PY'
import json, os
allow = {"approve", "approve_wider", "allow_window"}
for card in json.loads(os.environ["MCP_CARDS"]).get("confirmations", []):
    title = card.get("title", "")
    if "想用" not in title or not title.endswith(os.environ["MCP_TOOL_SUFFIX"]):
        continue
    chosen = next((c for c in card.get("choices", []) if c.get("choice_id") and c.get("answer") in allow), None)
    if chosen is not None:
        print(card["id"], json.dumps({"answer": chosen["answer"], "choice_id": chosen["choice_id"]}), sep="\t")
    elif not any(c.get("choice_id") for c in card.get("choices", [])):
        print(card["id"], '{"answer":"approve"}', sep="\t")
PY
}

mcp_check() {
	local caller out cards rc=1
	out="$(mktemp)"
	# 客户端结果落盘和进程退出有先后：等文件内容，不等进程。
	( mcp call "${SMOKE_SLUG}__${MCP_TOOL}" "$MCP_ARGS" >"$out" 2>&1 ) &
	caller=$!
	local deadline=$((SECONDS + 80))
	# 总线把工具结果包成转义 JSON 字符串（\"version\"），引号匹配不到，按裸关键字断言。
	while ((SECONDS < deadline)); do
		grep -q 'first_run' "$out" 2>/dev/null && { rc=0; break; }
		cards="$(mcp_cards_to_approve "$(smoke_api GET /_wonton/api/confirmations 2>/dev/null)")"
		while IFS=$'\t' read -r id body; do
			[[ -n "$id" ]] || continue
			smoke_api POST "/_wonton/confirmations/$id" -H 'Content-Type: application/json' -d "$body" >/dev/null ||
				echo "批准卡片 $id 失败"
		done <<<"$cards"
		sleep 2
	done
	kill "$caller" 2>/dev/null || true
	wait "$caller" 2>/dev/null || true
	((rc != 0)) && cat "$out" >&2
	rm -f "$out"
	return "$rc"
}

# ---------- 数据（重装/升级保数据）----------
# 数据标记：用出厂管理员建一个以标记为名的用户（应用 SQLite 在数据目录里，重装保留）。
ab_marker() { # 标记文件：SMOKE_STATE 是文件就用它，是目录就放里面
	if [[ -f "$SMOKE_STATE" ]]; then
		printf '%s' "$SMOKE_STATE"
	else
		printf '%s' "$SMOKE_STATE/marker"
	fi
}

ab_marker_read() { cat "$(ab_marker)" 2>/dev/null || true; }

ab_marker_write() { # <值>：原样覆盖写标记
	local f
	f="$(ab_marker)"
	mkdir -p "$(dirname "$f")"
	printf '%s' "$1" >"$f"
}

ab_users() { # 已登录会话：列出应用用户
	web_get /api/v1/users
}

data_write() { # 建一个用户名带随机标记的账号，作为重装后要找到的用户数据
	local marker resp
	marker="wonton$(date +%s)"
	ab_login >/dev/null || { echo "应用登录失败（出厂账号 admin）"; return 1; }
	resp="$(dweb /api/v1/users -X POST -H 'Content-Type: application/json' \
		--data "{\"username\":\"$marker\",\"password\":\"${marker}Pass1\"}")" ||
		{ echo "建用户失败：$resp"; return 1; }
	grep -q '"username"' <<<"$resp" || { echo "建用户的响应不对：$(head -c 200 <<<"$resp")"; return 1; }
	ab_marker_write "$marker"
}

data_check() { # 重装/升级后：标记用户还在应用的用户列表里
	local marker users
	marker="$(ab_marker_read)"
	[[ -n "$marker" ]] || { echo "没有数据标记（data_write 没跑？）"; return 1; }
	ab_login >/dev/null || { echo "应用登录失败（出厂账号 admin）"; return 1; }
	users="$(ab_users)" || { echo "读不到用户列表：$users"; return 1; }
	grep -q "\"$marker\"" <<<"$users" ||
		{ echo "标记用户 $marker 不在了，用户列表：$(head -c 300 <<<"$users")"; return 1; }
}
