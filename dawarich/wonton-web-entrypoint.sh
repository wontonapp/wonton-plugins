#!/bin/sh
# 从 WONTON_APP_URL 取公开主机名补进 APPLICATION_HOSTS：production 按 Rails config.hosts 拒绝
# 没列出的 Host，而网关转发保留原始 Host、compose 插值取不出主机名。协议同样从 URL 推，
# 决定 config.force_ssl。之后交给上游入口脚本（建库、迁移、种子照上游跑），参数原样传。
set -e

host=$(printf '%s' "$WONTON_APP_URL" | sed -e 's|^[a-zA-Z][a-zA-Z0-9+.-]*://||' -e 's|[/:?#].*||')
proto=$(printf '%s' "$WONTON_APP_URL" | sed -n 's|^\([a-zA-Z][a-zA-Z0-9+.-]*\)://.*|\1|p')

if [ -n "$host" ]; then
  export APPLICATION_HOSTS="${APPLICATION_HOSTS:+$APPLICATION_HOSTS,}$host"
fi
if [ -n "$proto" ]; then
  export APPLICATION_PROTOCOL="$proto"
fi

# 绝对路径：上游脚本按 $(dirname "$0") 找同目录的 entrypoint-env-guard.sh。
exec /usr/local/bin/web-entrypoint.sh "$@"
