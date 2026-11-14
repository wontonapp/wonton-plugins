# shellcheck shell=bash
# scripts/smoke.sh 的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# 变量别叫 state：shellcheck 把这份文件连着 probes.sh 一起分析，probes.sh 自己也读一个 dev.sh 提供的 state；
# 子壳体函数（fn() ( … )）里给 state 赋值，SC2031 就报在 probes.sh 上，CI 红。
#
# 变量：SMOKE_WEB_URL 是应用的网页入口（套件已去尾斜杠，但可能带路径，比如 SSO 登录页）；
#       拼 API 路径用 SMOKE_WEB_ORIGIN（只有 scheme://host:port，入口的路径不掺进来），别直接接在 SMOKE_WEB_URL 后面。
#
# 页面：web_assert 从标准输入读页面，命中的确是应用的真实页面时返回 0。
# 身份：glance 没有用户体系，网页由云吞网关的会话保护，应用自己不再登录；身份项不适用。
# 工具：读配置走总线的 read_file；镜像没标 readOnlyHint，总线先弹确认卡，mcp_check 里代批。
export IDENTITY_NA=1
export SMOKE_NOTE="glance 没有用户体系，网页由云吞网关登录保护"
# 数据：glance 的配置即数据。data_write 往数据目录里的 glance.yml 追加一个带随机标记的页面，
#       data_check 在重装后按 slug 打开那个页，页面在就说明用户改过的配置原样保留。
_instance() { # 本实例名（钩子契约可能没导出 SMOKE_INSTANCE，按 SMOKE_BASE 的主机名第一段推）
  local inst="${SMOKE_INSTANCE:-}"
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  printf '%s' "$inst"
}

_container() { # <插件 id> <服务名>：本实例里这个服务的容器（按核心打的标签找）
  local inst args=(--filter "label=wonton.plugin=$1" --filter "label=wonton.service=$2")
  inst="$(_instance)"
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

web_assert() { grep -qi 'glance'; }   # 应用自己的页面（默认页的品牌名和资源路径都带 glance）

data_write() {
  local c marker
  c="$(_container app.github.glanceapp.glance glance)"
  [[ -n "$c" ]] || { echo "找不到 glance 容器"; return 1; }
  marker="wonton-$(_instance)-$RANDOM$RANDOM"
  docker exec -i "$c" sh -c 'cat >> /app/config/glance.yml' <<YAML || { echo "追加标记页失败"; return 1; }

  - name: $marker
    slug: $marker
    columns:
      - size: full
        widgets:
          - type: clock
            hour-format: 24h
            timezones:
              - timezone: UTC
                label: $marker
YAML
  echo "$marker" >"$SMOKE_STATE"
}

data_check() {
  local marker page
  marker="$(cat "$SMOKE_STATE")"
  [[ -n "$marker" ]] || { echo "没有数据标记"; return 1; }
  page="$(dweb "/$marker")" || { echo "打开标记页失败：$page"; return 1; }
  grep -qF "$marker" <<<"$page" || { echo "重装后找不到标记页 $marker：$(head -c 200 <<<"$page")"; return 1; }
}

mcp_check() { # 总线上有本插件的文件工具，读配置经确认后拿得到内容。
  # 工具镜像没标 readOnlyHint（上游原样）：总线按敏感处理，先弹确认卡。后台用主人令牌代批，
  # 验证“确认之后拿得到结果”这整条链；循环 60 秒后自停。
  local out
  ( for _ in $(seq 1 30); do probes_dev approve >/dev/null 2>&1 || true; sleep 2; done ) &
  local approver=$!
  out="$(probes_dev mcp call "${SMOKE_SLUG}__read_file" '{"path":"glance.yml"}' 2>&1)"
  kill "$approver" 2>/dev/null || true
  [[ -n "$out" ]] || { echo "调用没有返回内容"; return 1; }
  grep -q 'pages:' <<<"$out" || { echo "读到的不是 glance.yml 的内容：$(head -c 300 <<<"$out")"; return 1; }
}
