# shellcheck shell=bash
# Browser：Bot 的可见 Chromium（noVNC 画面 + Playwright MCP）。sso=none、没有账号体系，身份项 N/A。
# 镜像没发布时在本机构建（README 的做法）。
export IDENTITY_NA=1
SMOKE_NOTE="noVNC 无账号体系（网关把关），身份项 N/A"

# 本机没有清单里那个摘要时按 README 构建，再把清单里的引用换成构建出来的（只改冒烟的副本）。
before_install() {
  local ref dig proxy="${https_proxy:-${HTTPS_PROXY:-}}" args=()
  ref="$(awk '/^ *image: /{print $2}' "$SMOKE_PLUGIN/docker-compose.yml" | grep '/browser:' | head -1)"
  [[ -n "$ref" ]] || { echo "清单里没有 browser 镜像"; return 1; }
  if [[ -z "${SMOKE_BUILD:-}" ]] && docker manifest inspect "$ref" >/dev/null 2>&1; then
    # 镜像已发布（ghcr）：交给 install 直接拉。只查本地 image inspect 会在干净 runner 上
    # 误判成「没发布」，本地构建出来的 dev 摘要 install 又拉不到（2026-11-08 CI 实测）。
    echo "清单镜像已在发布仓：$ref，不本地构建"
    return 0
  fi
  [[ -n "$proxy" ]] && args=(--build-arg "https_proxy=$proxy")
  (cd "$SMOKE_PLUGIN" && nice -n 19 timeout 2400 docker buildx build --platform linux/amd64 --load \
    --network host "${args[@]}" -t ghcr.io/wontonapp/wonton-plugins/browser/browser:dev images/browser) || return 1
  dig="$(docker image inspect ghcr.io/wontonapp/wonton-plugins/browser/browser:dev --format '{{.Id}}')" || return 1
  sed -i -E "s#image: [^ ]*/browser:[^ ]*#image: ghcr.io/wontonapp/wonton-plugins/browser/browser:dev@$dig#" "$SMOKE_PLUGIN/docker-compose.yml"
  grep -qF "ghcr.io/wontonapp/wonton-plugins/browser/browser:dev@$dig" "$SMOKE_PLUGIN/docker-compose.yml" || { echo "清单摘要没换成功"; return 1; }
}

web_assert() { grep -qiE 'novnc|vnc'; }   # 标准输入：noVNC 页面

mcp_check() { # 有只读工具（观察类）就真调一个，否则只断言 tools/list 有工具
  local n_mine tool out
  n_mine="$(jq --arg s "$SMOKE_SLUG" '[.[] | select(.name | startswith($s + "__"))] | length' "$SMOKE_TOOLS")"
  [[ "$n_mine" -ge 1 ]] || { echo "tools/list 没有 ${SMOKE_SLUG}__ 前缀工具"; return 1; }
  tool="$(jq -r --arg s "$SMOKE_SLUG" '[.[] | select(.name | startswith($s + "__") and .annotations.readOnlyHint == true)][0].name // ""' "$SMOKE_TOOLS")"
  if [[ -z "$tool" ]]; then
    SMOKE_NOTE="${SMOKE_NOTE}；工具没标 readOnlyHint，仅 tools/list，未调"
    return 0
  fi
  out="$(mcp call "$tool" '{}')" || { echo "只读工具 $tool 调用失败：$out"; return 1; }
  grep -qiE 'snapshot|console|page|tabs' <<<"$out" || { echo "只读工具 $tool 结果不像观察结果：$(head -c 150 <<<"$out")"; return 1; }
}

# 数据：Chromium 的 profile（登录过的网站、Cookie）存在插件数据目录（docker-compose.yml 的
# ${WONTON_APP_DATA}:/config）。data_write 在容器里往 profile 目录落一个标记文件（Chromium 不认识
# 的文件会忽略，不影响 profile），data_check 在升级、重装后读回比对；内容带随机令牌。
_br_container() {
  local inst="${SMOKE_INSTANCE:-${SMOKE_BASE#http://}}"
  inst="${inst%%.*}"
  docker ps --format '{{.Names}}' --filter "label=wonton.instance=${inst//-/}" \
    --filter 'label=wonton.plugin=app.wonton.browser' --filter 'label=wonton.service=browser' | head -1
}

data_write() {
  local container token out
  container="$(_br_container)"
  token="wonton-smoke-$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
  docker exec "$container" sh -c "printf 'wonton-smoke %s\n' '$token' > /config/wonton-smoke-marker" ||
    { echo "往 $container:/config 写标记失败"; return 1; }
  out="$(docker exec "$container" cat /config/wonton-smoke-marker)" || { echo "标记写完读不回"; return 1; }
  grep -qF "$token" <<<"$out" || { echo "标记读回对不上：$out"; return 1; }
  printf '%s' "$token" >"$SMOKE_STATE" || { echo "写不进标记文件"; return 1; }
}

data_check() {
  local container token out
  container="$(_br_container)"
  token="$(cat "$SMOKE_STATE" 2>/dev/null)" || true
  [[ -n "$token" ]] || { echo "没有 data_write 留下的标记（SMOKE_STATE=${SMOKE_STATE:-未设置}）"; return 1; }
  out="$(docker exec "$container" cat /config/wonton-smoke-marker 2>/dev/null)" ||
    { echo "标记文件没了：$container:/config/wonton-smoke-marker 读不回"; return 1; }
  grep -qF "$token" <<<"$out" || { echo "标记内容对不上：$(head -c 120 <<<"$out")"; return 1; }
  echo "profile 目录里的标记还在（wonton-smoke $token）"
}
