# shellcheck shell=bash
# ConvertX：sso=none，应用自己的账号（第一个注册的是管理员，之后注册关闭）。
# 探针在插件容器里用镜像自带的 curl 走 127.0.0.1:3000；请求同容器的回环地址，不涉代理。
#
# 数据钩子：注册（首次）或登录（数据已在时）→ 上传带标记的 txt → calibre 转 PDF →
# 等历史页出现 completed；data_check 用存的凭据重新登录，在历史页找标记。
# 凭据和标记存 $SMOKE_STATE（JSON），升级/重装后照样能登录。

web_assert() { grep -qi 'convertx'; }   # setup 页（Welcome to ConvertX!）和登录页（ConvertX | Login）都命中

export IDENTITY_NA=1
export SMOKE_NOTE="ConvertX 用应用自己的账号（第一个注册的是管理员），与核心会话无关；主人能打开由网页项证明，应用账号和数据的读写由 data_write/data_check 验证。"

state_file() { # SMOKE_STATE 套件里是标记文件，dev.sh probes 里是状态目录
  if [[ -d "${SMOKE_STATE:-}" ]]; then printf '%s' "$SMOKE_STATE/convertx-state.json"; else printf '%s' "${SMOKE_STATE:-}"; fi
}

_container() { # 本实例里 convertx 服务的容器；e2e 的 data_write 跑在 SMOKE_PLUGIN_ID 导出之前，id 用字面量
  local inst="${SMOKE_INSTANCE:-}" args=(--filter "label=wonton.plugin=app.convertx.convertx" --filter "label=wonton.service=convertx")
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

_cx() { # <cookie jar> <路径> [curl 参数…]：容器里对 127.0.0.1:3000 发请求，输出状态码
  local jar="$1" path="$2"; shift 2
  docker exec "$_cx_container" curl -s --noproxy '*' -o /dev/null -w '%{http_code}' -b "$jar" -c "$jar" "$@" "http://127.0.0.1:3000$path"
}

_cx_body() { # <cookie jar> <路径>：容器里取页面正文
  docker exec "$_cx_container" curl -s --noproxy '*' -b "$1" -c "$1" "http://127.0.0.1:3000$2"
}

data_write() {
  local container marker email pass code jar rand out job_id
  [[ -n "${SMOKE_STATE:-}" ]] || { echo "SMOKE_STATE 没设"; return 1; }
  container="$(_container)"
  [[ -n "$container" ]] || { echo "找不到 convertx 容器（wonton.plugin 标签）"; return 1; }
  _cx_container="$container"
  jar="/tmp/.cx-smoke-cj"

  rand="$(od -An -N6 -tx4 /dev/urandom | tr -d ' \n')"
  marker="wonton-smoke-$rand"

  # 已有凭据就登录（升级起点重跑、数据已在）；否则注册第一个账号（FIRST_RUN 时总是允许）。
  if [[ -s "$(state_file)" ]]; then
    email="$(jq -r '.email' "$(state_file)")"
    pass="$(jq -r '.password' "$(state_file)")"
    code="$(_cx "$jar" /login -d "email=$email&password=$pass")"
    [[ "$code" == 302 ]] || { echo "用已存凭据登录失败（HTTP $code）"; return 1; }
  else
    email="wonton-smoke-$rand@example.invalid"
    pass="cx-$rand"
    code="$(_cx "$jar" /register -d "email=$email&password=$pass")"
    # 成功和“注册已关”都回 302：成功会写 auth Cookie，被拒不会，拿 Cookie 判定。
    if [[ "$code" != 302 ]] || ! docker exec "$container" grep -q $'\tauth\t' "$jar" 2>/dev/null; then
      echo "注册第一个账号失败（HTTP $code；数据已在且没有凭据存档时也会这样）"; return 1
    fi
    jq -n --arg e "$email" --arg p "$pass" '{email:$e,password:$p}' >"$(state_file)"
  fi

  # GET / 建一个 job，jobId 在 cookie jar 里；后面上传和转换都要带它。
  code="$(_cx "$jar" /)"
  [[ "$code" == 200 ]] || { echo "打开首页失败（HTTP $code）"; return 1; }
  job_id="$(docker exec "$container" awk '$6=="jobId" {print $7}' "$jar" | tail -1)"
  [[ -n "$job_id" ]] || { echo "cookie jar 里没有 jobId"; return 1; }

  # 上传带标记的 txt，calibre 转成 PDF（txt→pdf 是上游 calibre 转换器的合法组合）。
  docker exec "$container" sh -c "printf 'wonton smoke marker $marker\n' > '/tmp/$marker.txt'" || { echo "造上传文件失败"; return 1; }
  code="$(docker exec "$container" curl -s --noproxy '*' -o /dev/null -w '%{http_code}' -b "$jar" \
    -F "file=@/tmp/$marker.txt" http://127.0.0.1:3000/upload)"
  [[ "$code" == 200 ]] || { echo "上传失败（HTTP $code）"; return 1; }
  code="$(_cx "$jar" /convert -d "convert_to=pdf,calibre" --data-urlencode "file_names=[\"$marker.txt\"]")"
  [[ "$code" == 302 ]] || { echo "发起转换失败（HTTP $code）"; return 1; }

  # 转换是后台任务：等历史页出现这个文件且 job 是 completed（calibre 冷启动一次要几十秒）。
  for _ in $(seq 1 60); do
    out="$(_cx_body "$jar" /history)"
    if grep -q "$marker" <<<"$out" && grep -q 'completed' <<<"$out"; then
      docker exec "$container" rm -f "/tmp/$marker.txt" "$jar"
      jq -n --arg email "$email" --arg pw "$pass" --arg marker "$marker" \
        '{email:$email,password:$pw,marker:$marker}' >"$(state_file)"
      return 0
    fi
    sleep 5
  done
  echo "300 秒内历史页没出现 completed 的标记任务；页面开头：$(head -c 300 <<<"$out" | tr '\n' ' ')"
  return 1
}

data_check() {
  local container email pass marker out
  container="$(_container)"
  [[ -n "$container" ]] || { echo "找不到 convertx 容器（wonton.plugin 标签）"; return 1; }
  _cx_container="$container"
  [[ -s "$(state_file)" ]] || { echo "没有 data_write 留下的凭据存档"; return 1; }
  email="$(jq -r '.email' "$(state_file)")"
  pass="$(jq -r '.password' "$(state_file)")"
  marker="$(jq -r '.marker' "$(state_file)")"
  [[ -n "$email" && -n "$pass" && -n "$marker" ]] || { echo "凭据存档不完整：$(cat "$(state_file)")"; return 1; }

  jar="/tmp/.cx-smoke-cj"
  code="$(_cx "$jar" /login -d "email=$email&password=$pass")"
  [[ "$code" == 302 ]] || { echo "重装后用存档凭据登录失败（HTTP $code）——账号没保住？"; return 1; }
  out="$(_cx_body "$jar" /history)"
  if ! grep -q "$marker" <<<"$out"; then
    echo "历史页里没有标记 $marker；页面开头：$(head -c 300 <<<"$out" | tr '\n' ' ')"
    return 1
  fi
  docker exec "$container" rm -f "$jar"
}
