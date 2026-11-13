#!/bin/sh
# 一次性服务：生成 broker 的口令文件。核心每次重建容器都会重跑，必须幂等：
# mosquitto_passwd 不带 -c 时对已有用户改口令、没有的用户新增，文件不存在时才用 -c 新建。
set -e

PWFILE=/mosquitto/data/passwd
if [ ! -s "$PWFILE" ]; then
	mosquitto_passwd -b -c "$PWFILE" health "$WONTON_SECRET_HEALTH_PASSWORD"
else
	mosquitto_passwd -b "$PWFILE" health "$WONTON_SECRET_HEALTH_PASSWORD"
fi
# 主人账号：用户名是注入的用户名，口令来自核心生成的密钥，和 lan.client 显示的一致。
mosquitto_passwd -b "$PWFILE" "$WONTON_OWNER_NAME" "$WONTON_SECRET_CLIENT_PASSWORD"
