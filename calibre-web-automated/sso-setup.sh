#!/usr/bin/with-contenv bash
# 启动前的一次性配置（s6 oneshot，应用服务依赖本服务）：等上游的 cwa-init/cwa-auto-library 建好 app.db，
# 把 OIDC 提供方、账号口令写进库。应用第一次读配置就是改好的，不用重启、不用 HTTP 登录。
# 每次创建容器都整体重写（插件规范"OIDC"：保留数据重装后客户端密钥是新的，不能"已存在就跳过"）。
set -euo pipefail

if [ "$(id -u)" = "0" ]; then
  # cwa-init 已把 /config 交给 PUID:PGID；以同一用户写库，不留 root 的 sqlite 日志文件。
  exec s6-setuidgid abc "$0" "$@"
fi

: "${WONTON_OWNER_NAME:?核心没有注入用户名（owner.identity）}"
: "${WONTON_SECRET_CLIENT_PASSWORD:?核心没有注入密钥}"
: "${WONTON_OIDC_CLIENT_SECRET:?核心没有注入 OIDC 密钥}"

db=/config/app.db
i=0
until [ -f "$db" ] && [ "$(sqlite3 "$db" "SELECT count(*) FROM sqlite_master WHERE name IN ('settings','user','oauthProvider')")" = "3" ]; do
  i=$((i + 1))
  [ "$i" -lt 120 ] || { echo "[wonton-sso] app.db 两分钟内没有就绪（上游初始化没跑完？）"; exit 1; }
  sleep 1
done

/lsiopy/bin/python3 /etc/s6-overlay/s6-rc.d/wonton-sso/setup.py
