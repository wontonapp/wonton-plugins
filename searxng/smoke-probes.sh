# shellcheck shell=bash
# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# SearXNG：sso=none，应用没有账号体系，访问控制由网关完成；mcp 工具都是只读搜索/读网页。
# 只用套件保证的东西：变量 SMOKE_STATE、SMOKE_JAR、SMOKE_WEB_URL、SMOKE_OWNER、SMOKE_BASE、
# SMOKE_TOKEN、SMOKE_INSTANCE，助手 web_get、dweb。
export IDENTITY_NA=1
export SMOKE_NOTE="应用没有账号体系（sso: none，网关校验会话），身份项 N/A"

web_assert() { grep -q '<h1>SearXNG</h1>'; }   # 标准输入：SearXNG 首页标题

identity_check() { :; }

# 工具：搜索工具是只读的，直接查一个常见词，结果里有 URL 即算通过。
export MCP_TOOL=searxng_web_search
export MCP_ARGS='{"query":"云吞"}'

mcp_assert() { # 标准输入：工具结果（markdown，含结果标题和链接）
	local out
	out="$(cat)"
	grep -qE 'https?://' <<<"$out" ||
		{ echo "搜索工具没有返回带链接的结果：$(head -c 200 <<<"$out")"; return 1; }
}

# 数据：SearXNG 不在服务器上存用户数据（偏好放在浏览器 cookie 里，聚合结果是现查的），
# 升级/重装没有数据可保。设置文件是插件包的 configs，每次建容器都重发。
export DATA_NA=1
