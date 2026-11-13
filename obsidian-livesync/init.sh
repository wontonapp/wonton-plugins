#!/bin/sh
# 一次性服务：等 CouchDB 健康后建库、建客户端账号、设库权限，并核验配置真的生效。
# 幂等：已存在的库（412）和账号（409）照常继续；库权限每次覆盖写。
set -eu

DB=obsidiandb
MARKER=/opt/couchdb/data/.wonton-initialized
rm -f "$MARKER"

ADMIN_NAME="${COUCHDB_USER:?}"
ADMIN="curl -s -u ${ADMIN_NAME}:${WONTON_SECRET_ADMIN_PASSWORD:?}"
CLIENT_USER="${WONTON_OWNER_NAME:?核心没有注入用户名}"
CLIENT="curl -s -u ${CLIENT_USER}:${WONTON_SECRET_CLIENT_PASSWORD:?}"

code="$($ADMIN -o /dev/null -w '%{http_code}' -X PUT "http://couchdb:5984/$DB")"
[ "$code" = 201 ] || [ "$code" = 412 ] || { echo "建库失败：HTTP $code"; exit 1; }

# 客户端账号：已存在时不动（用户可能改过密码，核心显示的密码以第一次写入为准）。
code="$($ADMIN -o /dev/null -w '%{http_code}' -X PUT "http://couchdb:5984/_users/org.couchdb.user:$CLIENT_USER" \
  -H 'Content-Type: application/json' \
  -d "{\"_id\":\"org.couchdb.user:$CLIENT_USER\",\"name\":\"$CLIENT_USER\",\"password\":\"$WONTON_SECRET_CLIENT_PASSWORD\",\"roles\":[],\"type\":\"user\"}")"
[ "$code" = 201 ] || [ "$code" = 409 ] || { echo "建客户端账号失败：HTTP $code"; exit 1; }

# 数据库只给客户端账号读写（覆盖写，幂等）。
code="$($ADMIN -o /dev/null -w '%{http_code}' -X PUT "http://couchdb:5984/$DB/_security" \
  -H 'Content-Type: application/json' \
  -d '{"admins":{"names":[],"roles":[]},"members":{"names":["'"$CLIENT_USER"'"],"roles":[]}}')"
[ "$code" = 200 ] || [ "$code" = 201 ] || { echo "设置库权限失败：HTTP $code"; exit 1; }

# 核验 Obsidian 桌面端的 CORS 预检放行（couchdb.ini 没加载成时这里失败，安装如实报错）。
origin="$($CLIENT -D - -o /dev/null -X OPTIONS "http://couchdb:5984/" \
  -H 'Origin: app://obsidian.md' -H 'Access-Control-Request-Method: GET' | tr -d '\r' \
  | sed -n 's/^[Aa]ccess-[Cc]ontrol-[Oo]rigin: //p')"
[ "$origin" = "app://obsidian.md" ] || { echo "CORS 没生效，Access-Control-Allow-Origin：${origin:-无}"; exit 1; }

# 核验客户端账号能访问库；失败即停。
body="$($CLIENT "http://couchdb:5984/$DB")"
echo "$body" | grep -q "\"db_name\":\"$DB\"" || { echo "客户端账号访问库失败：$(echo "$body" | head -c 200)"; exit 1; }

# clients.http.auth.app 的 setup_done 只在全部初始化和核验成功后返回 0。
tmp="$MARKER.tmp"
umask 022
printf 'initialized\n' >"$tmp"
chmod 644 "$tmp"
mv "$tmp" "$MARKER"
echo "CouchDB 就绪：数据库 $DB，客户端账号 $CLIENT_USER"
