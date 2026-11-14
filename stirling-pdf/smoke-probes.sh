# shellcheck shell=bash disable=SC2034
# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# 页面：web_assert 从标准输入读页面，命中的确是应用的真实页面时返回 0。
# 身份：identity_check 用应用自己的“当前用户”接口断言认出的是用户管理员。没有用户体系时写 IDENTITY_NA=1，
#       并在 SMOKE_NOTE 里写原因。dweb / web_get 是带用户会话的请求助手，可继续走应用自己的登录链。
# 工具：MCP_TOOL 写一个只读工具名（可带插件前缀），MCP_ARGS 是 JSON 参数；mcp_assert 从标准输入读调用结果。
#       要断言多个工具时改成写 mcp_check；调用前要备数据时写 before_mcp。
# 装之前要准备（如先配出口代理）：写 before_install。

# 保数据：应用登录被 custom_settings.yml 关掉（见 compose 的 init 服务说明），游客身份没有可写的
# 用户数据——保存到服务器等入口只对登录用户开放，处理的文件也是临时的（about 已如实写明）。
# 没有可写的用户数据，钩子改为核对数据卷里的标记文件（见文末）。

web_assert() { grep -qi 'stirling'; }   # stdin：应用真实页面（SPA 首页，标题带 Stirling）

# 身份：应用登录被 custom_settings.yml 关掉（compose 的 init 服务说明），security profile 恒开、
# 应用 API 对匿名请求一律 401（实测 3.3.0）——应用侧没有可断言的用户身份；主人能打开由 web_assert
# 和网关对匿名请求的 401 证明。
export IDENTITY_NA=1
export SMOKE_NOTE="应用登录被 custom_settings.yml 关掉、应用 API 匿名一律 401，应用侧没有可断言的用户身份；主人会话能打开应用、网关拒绝匿名请求由网页项证明。"

# 保数据：应用登录被 custom_settings.yml 关掉（见 compose 的 init 服务说明），登录接口虽能发令牌，
# 但登录关闭时请求仍是匿名的（实测 3.3.0：Bearer 和会话 Cookie 访问都回 401），应用 API 写不了用户数据。
# 数据卷 `/configs` 里是用户库（H2）、设置和 custom_settings.yml，升级把它弄丢等于把插件设置改回去；
# 这里以应用用户（1000）在 /configs 写一个带随机标记的文件，升级/重装后读回核对。
# 容器名按核心规则拼、标签兜底（见 _stirling_container）。
_stirling_container() { # 应用容器：名字按核心规则拼（wonton-<实例>--<slug 的连字符形>--app）；
  # 实例名未导出时（极早调用）按 SMOKE_BASE 的主机名第一段推，名字查不到再按标签兜底。
  local inst="${SMOKE_INSTANCE:-}"
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  if [[ -n "$inst" ]]; then
    local name="wonton-${inst}--com-stirlingtools-stirling-pdf--app"
    docker inspect "$name" >/dev/null 2>&1 && { printf '%s\n' "$name"; return 0; }
  fi
  local args=(--filter label=wonton.plugin=com.stirlingtools.stirling-pdf --filter label=wonton.service=app)
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

data_write() {
  local marker container
  [[ -n "${SMOKE_STATE:-}" ]] || { echo "SMOKE_STATE 没设"; return 1; }
  marker="wonton-smoke-$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
  container="$(_stirling_container)"
  [[ -n "$container" ]] || { echo "找不到应用容器（wonton.instance 标签）"; return 1; }
  docker exec -u 1000:1000 "$container" sh -c 'printf %s "$1" > "/configs/$2"' sh "$marker" "$marker" ||
    { echo "写不进 /configs（容器 $container）"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() {
  local marker container out
  marker="$(cat "$SMOKE_STATE")" || { echo "读不到标记文件"; return 1; }
  container="$(_stirling_container)"
  [[ -n "$container" ]] || { echo "找不到应用容器（wonton.instance 标签）"; return 1; }
  out="$(docker exec -u 1000:1000 "$container" sh -c 'cat "/configs/$1" 2>&1' sh "$marker")" ||
    { echo "标记文件读不回来（数据卷丢了？）：$out"; return 1; }
  [[ "$out" == "$marker" ]] || { echo "标记对不上：期待 $marker，看到 $out"; return 1; }
}

# ---------- 邮件（notify）：附加检查，3.3.0 上跳过（上游 send-email 带附件必现 NPE） ----------
#
# 原探针经网关 POST /api/v1/general/send-email（表单 fileInput、to、subject、body，与上游 v3.3.0 的
# EmailController/Email 一致），等核心把信投进主人私聊。3.3.0 上它确定性失败：接口回 200「Email sent
# successfully」，但 stirling 容器日志里 @Async 发信线程在 EmailService.sendEmailWithAttachment 的
# mailSender.send 处抛 NullPointerException（Cannot invoke "java.nio.file.Path.getFileSystem()" because
# "<parameter1>" is null），信从未进入 Mailpit，核心无信可投。实测两次（11-12、11-13）结果相同。
# 推断（读 v3.3.0 源码，未在上游逐行验证）：控制器把 multipart 交给 @Async 方法后立即返回，请求结束时
# multipart 的临时文件已被清理，异步线程发信时读附件拿到空路径。上传形态再对也改不了这一点。
# 同理，外部地址那一项（期待 550 拒收）也看不到：接口异步，受理即 200。
# 上游修好后恢复：把下面的 extra_item 换回本文件 git 历史里的旧探针（发信、等私聊、查 Mailpit 滞留、外部地址 550）。
extra_check() {
  extra_item N/A 邮件投递 "上游 stirling-pdf 3.3.0 的 send-email 带附件必现 NPE，信投不进私聊，已跳过（原因见探针注释）"
}
