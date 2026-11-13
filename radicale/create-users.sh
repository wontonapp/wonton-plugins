#!/bin/sh
# 一次性服务：把 lan.client 的密码按 bcrypt 写进 htpasswd，账号是主人（WONTON_OWNER_NAME，
# 和网页里网关注入的用户名同一个）。密钥没注入或用户名没注入时直接失败，不放假文件。
# 每次容器重建都重跑，结果等价：同一把密钥算出的 bcrypt 哈希都能验证通过。
set -eu

dir=/var/lib/radicale/auth
users=$dir/users
tmp=$dir/.users.new

mkdir -p "$dir"
[ -n "${WONTON_OWNER_NAME:-}" ] || { echo "WONTON_OWNER_NAME 没注入" >&2; exit 1; }
[ -n "${WONTON_SECRET_CLIENT_PASSWORD:-}" ] || { echo "WONTON_SECRET_CLIENT_PASSWORD 没注入" >&2; exit 1; }

# 镜像自带 bcrypt 模块（passlib 验证，radicale/auth/htpasswd.py）。
hash="$(/app/bin/python -c 'import bcrypt, os; print(bcrypt.hashpw(os.environ["WONTON_SECRET_CLIENT_PASSWORD"].encode(), bcrypt.gensalt()).decode())')"
printf '%s:%s\n' "$WONTON_OWNER_NAME" "$hash" >"$tmp"
mv "$tmp" "$users"
echo "htpasswd 就绪：$WONTON_OWNER_NAME"
