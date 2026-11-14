# scripts/smoke.sh source 本文件，不单独执行。
# shellcheck shell=bash
# Flood：sso=none。flood 没有 OIDC 和代登录请求头支持：
# - 核心注入 WONTON_TORRENT_CLIENT_URL 时走上游自带的 auth=none，无应用登录页，主人身份由云吞网关认证；
# - 没注入时（能力依赖核心尚未合入），保留 flood 自带账号，第一次打开在应用里注册。
export IDENTITY_NA=1
export SMOKE_NOTE="flood 不支持 OIDC 和代登录请求头；有下载器连接时用上游 auth=none（无应用登录），没有时用自带账号，两种都由云吞网关把守。"

web_assert() { grep -qi '<title>Flood</title>'; }

# 只用套件保证的钩子输入：SMOKE_JAR（主人会话）、SMOKE_WEB_URL、SMOKE_STATE（data_write 覆盖写、data_check 读）。

# flood 会话：auth=none 下 GET /api/auth/verify 无条件发 jwt Cookie；
# 自带账号模式（能力核心未合入的过渡态）下，若还没有用户就注册一个（凭据随机生成，记进 SMOKE_STATE），
# 已有用户就用 SMOKE_STATE 里的凭据登录。返回积累了网关会话 + flood jwt 的临时 cookie 文件。
flood_session() (
  set -eu
  local web="${SMOKE_WEB_URL:?SMOKE_WEB_URL 未导出}"
  local jar out verify code creds user pass
  jar="$(mktemp "${TMPDIR:-/tmp}/flood-smoke-jar.XXXXXX")"
  cp -f "${SMOKE_JAR:?SMOKE_JAR 未导出}" "$jar"
  creds="${SMOKE_STATE:?SMOKE_STATE 未导出}"
  out="$(curl -sS --noproxy '*' -b "$jar" -c "$jar" --max-time 20 -w '\n%{http_code}' "$web/api/auth/verify")"
  code="${out##*$'\n'}"
  verify="${out%$'\n'*}"
  if [ "$code" != 200 ] && [ "$code" != 401 ]; then
    echo "flood 会话建立失败：HTTP $code $(head -c 120 <<<"$verify")" >&2; return 1
  fi
  echo "flood 会话基线：HTTP $code $(grep -o '"initialUser":[a-z]*\|"authMethod":"[a-z]*"' <<<"$verify" | tr '\n' ' ')" >&2
  # verify 的三种形态：200+initialUser:true＝还没有用户（自带账号模式，先注册）；
  # 200＝auth=none 或已有会话；401＝自带账号且已有用户，用 SMOKE_STATE 里的凭据登录。
  if [ "$code" = 401 ] || grep -q '"initialUser":true' <<<"$verify"; then
    # 套件会把 SMOKE_STATE 预创建成空文件：读到空字段时按没有凭据处理，现场生成。
    user=""
    pass=""
    if [ -f "$creds" ]; then
      user="$(awk 'NR==1{print $1}' "$creds")"
      pass="$(awk 'NR==1{print $2}' "$creds")"
    fi
    if [ -z "$user" ] || [ -z "$pass" ]; then
      user="wonton-smoke-$RANDOM"
      pass="$(head -c 24 /dev/urandom | base64 | tr -d '/+=')"
    fi
    if grep -q '"initialUser":true' <<<"$verify"; then
      # 注册失败就重试，并把响应体和诊断带回日志（见下方失败分支）。
      local ok=1 body code
      for _ in 1 2 3; do
        body="$(curl -sS --noproxy '*' -b "$jar" -c "$jar" --max-time 20 \
          -H 'Content-Type: application/json' \
          -d "{\"username\":\"$user\",\"password\":\"$pass\",\"client\":{\"client\":\"qBittorrent\",\"type\":\"web\",\"version\":1,\"url\":\"http://127.0.0.1:1\",\"username\":\"\",\"password\":\"\"},\"level\":10}" \
          -w '\n%{http_code}' "$web/api/auth/register")"
        code="${body##*$'\n'}"
        if [ "$code" = 200 ]; then ok=0; break; fi
        echo "flood 注册第 ${_} 次失败：HTTP $code $(head -c 300 <<<"${body%$'\n'*}")" >&2
        sleep 5
      done
      [ "$ok" = 0 ] || { echo "flood 注册测试用户失败（重试 3 次）" >&2; return 1; }
      mkdir -p "$(dirname "$creds")"
      printf '%s %s\n' "$user" "$pass" >"$creds"
    else
      curl -fsS --noproxy '*' -b "$jar" -c "$jar" --max-time 20 \
        -H 'Content-Type: application/json' \
        -d "{\"username\":\"$user\",\"password\":\"$pass\"}" \
        "$web/api/auth/authenticate" >/dev/null || {
        echo "flood 测试用户登录失败：$user" >&2; return 1; }
    fi
  fi
  printf '%s' "$jar"
)

# data_write：注册/登录后写一条 RSS 订阅（flood 的按用户持久数据，升级重装都在数据库里），
# 把登录凭据和订阅地址覆盖写进 SMOKE_STATE。URL 用 192.0.2.x 文档地址、间隔 24h：冒烟期间不会被真正抓取。
data_write() (
  set -eu
  local web="${SMOKE_WEB_URL:?SMOKE_WEB_URL 未导出}"
  local creds_file="${SMOKE_STATE:?SMOKE_STATE 未导出}"
  local jar out code feeds url user pass
  jar="$(flood_session)" || return 1
  jar="$(printf %s "$jar")"
  url="http://192.0.2.10/wonton-smoke-$(date +%s).xml"
  user="$(awk 'NR==1{print $1}' "$creds_file")"
  pass="$(awk 'NR==1{print $2}' "$creds_file")"
  out="$(curl -sS --noproxy '*' -b "$jar" --max-time 20 \
    -X PUT -H 'Content-Type: application/json' \
    -d "{\"label\":\"wonton-smoke\",\"url\":\"$url\",\"interval\":86400}" \
    -w '\n%{http_code}' "$web/api/feed-monitor/feeds")"
  code="${out##*$'\n'}"
  feeds="${out%$'\n'*}"
  [ "$code" = 200 ] || { echo "写入 RSS 订阅失败：HTTP $code $(head -c 200 <<<"$feeds")"; return 1; }
  printf '%s %s\n%s\n' "$user" "$pass" "$url" >"$creds_file"
  echo "已写 RSS 订阅 wonton-smoke：$url"
)

# data_check：用 SMOKE_STATE 里的凭据重建会话，订阅还在且 URL 对得上才算数据保住了。
data_check() (
  set -eu
  local web="${SMOKE_WEB_URL:?SMOKE_WEB_URL 未导出}"
  local creds_file="${SMOKE_STATE:?SMOKE_STATE 未导出}"
  local jar feeds url
  url="$(awk 'NR==2{print $0}' "$creds_file")"
  [ -n "$url" ] || { echo "SMOKE_STATE 里没有订阅地址（data_write 没跑过）"; return 1; }
  jar="$(flood_session)" || return 1
  feeds="$(curl -sS --noproxy '*' -b "$jar" --max-time 20 -w '\n%{http_code}' "$web/api/feed-monitor/feeds")"
  code="${feeds##*$'\n'}"
  [ "$code" = 200 ] || { echo "读 RSS 订阅失败：HTTP $code $(head -c 200 <<<"${feeds%$'\n'*}")"; return 1; }
  jq -e --arg url "$url" 'any(.[]; .label == "wonton-smoke" and .url == $url)' \
    <<<"${feeds%$'\n'*}" >/dev/null || { echo "升级/重装后找不到 wonton-smoke 订阅（$url）：$(head -c 200 <<<"${feeds%$'\n'*}")"; return 1; }
  echo "wonton-smoke 订阅还在：升级/重装保住了数据"
)
