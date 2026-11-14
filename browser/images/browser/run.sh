#!/bin/sh
# 以容器用户（PUID:PGID）运行（compose 的 user）。
# 核心负责数据目录属主；没有写权限就停，不以 root 修补历史目录。
if [ ! -w /config ]; then
  echo '浏览器数据目录 /config 不可写：请检查 PUID:PGID 和数据目录属主。' >&2
  exit 1
fi
mkdir -p /config/profile || exit 1
if [ ! -w /config/profile ]; then
  echo 'Chromium profile 不可写：请检查 profile 目录属主。' >&2
  exit 1
fi
mkdir -p /tmp/.X11-unix || exit 1
exec supervisord -c /etc/wonton/supervisord.conf
