# zcode 是 Agent：网页是登录页（底座的静态页，sso: none，网关挡在前面），没有应用账号体系或插件自己的 MCP 服务。
# shellcheck shell=bash disable=SC2034
# 通用插件冒烟验证装得上、登录页打得开、工作区文件升级和重装不丢；加密 Matrix 私聊、模型回答、Wonton MCP 工具、
# 打断、重启、登录和名单外不应答在附加检查里实测（套件的 Agent 助手，见 scripts/agent-probes.sh）。
IDENTITY_NA=1
SMOKE_NOTE="zcode 的网页是登录页，没有应用账号体系，用户身份由核心网关保证；没有自己的 MCP 服务。"

# guarded 模式下容器里哪些动作会出卡片没实测过，确认卡片这一项不测。
EXTRA_TIMEOUT=3000
extra_check() {
  agent_check_chat '请用一句话回答：zcode 冒烟。记住暗号：青杏。'
  agent_check_model
  agent_check_mcp_chat
  agent_check_mcp_bus
  extra_item N/A 确认卡片 "guarded 模式下容器里哪些动作出卡片没实测，不测"
  agent_check_interrupt
  agent_check_restart agent bridge
  agent_check_resume 青杏
  agent_check_outsider
  agent_check_login_web account-zai account-bigmodel model-gateway
  agent_check_login account-zai 'https?://'
}

web_assert() { grep -q '<title>登录</title>'; }

# 数据：Agent 的工作区（${WONTON_BOT_DATA}/workspace）会话之间一直在用。data_write 在 agent 容器的工作区落一个
# 带随机令牌的标记文件，data_check 在升级、重装后读回比对。
_zc_container() {
  local inst="${SMOKE_INSTANCE:-${SMOKE_BASE#http://}}"
  inst="${inst%%.*}"
  docker ps --format '{{.Names}}' --filter "label=wonton.instance=${inst//-/}" \
    --filter 'label=wonton.plugin=ai.zcode' --filter 'label=wonton.service=agent' | head -1
}

data_write() {
  local container token out
  container="$(_zc_container)" || { echo "找不到 agent 容器"; return 1; }
  token="wonton-smoke-$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
  docker exec "$container" sh -c "printf 'wonton-smoke %s\n' '$token' > /workspace/wonton-smoke-marker" ||
    { echo "往 $container:/workspace 写标记失败"; return 1; }
  out="$(docker exec "$container" cat /workspace/wonton-smoke-marker)" || { echo "标记写完读不回"; return 1; }
  grep -qF "$token" <<<"$out" || { echo "标记读回对不上：$out"; return 1; }
  printf '%s' "$token" >"$SMOKE_STATE" || { echo "写不进标记文件"; return 1; }
}

data_check() {
  local container token out
  container="$(_zc_container)" || { echo "找不到 agent 容器"; return 1; }
  token="$(cat "$SMOKE_STATE" 2>/dev/null)" || true
  [[ -n "$token" ]] || { echo "没有 data_write 留下的标记"; return 1; }
  out="$(docker exec "$container" cat /workspace/wonton-smoke-marker 2>/dev/null)" ||
    { echo "标记文件没了：$container:/workspace/wonton-smoke-marker 读不回"; return 1; }
  grep -qF "$token" <<<"$out" || { echo "标记内容对不上：$(head -c 120 <<<"$out")"; return 1; }
  echo "工作区里的标记还在（wonton-smoke $token）"
}
