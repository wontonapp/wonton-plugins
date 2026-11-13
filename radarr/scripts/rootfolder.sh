#!/bin/sh
# 建电影根目录 /media/movies 并登记成 Radarr 的根目录，加片时下拉框里直接有。
# 每次创建容器都会重跑，必须幂等：目录已存在不动；同路径的根目录已登记就不再 POST。
# 以 PUID:PGID 运行（compose 里写 user），建的目录归用户的用户，Radarr 和 Jellyfin 都写读得动。
set -eu

path=/media/movies
api="http://radarr:7878/api/v3"
key="${WONTON_SECRET_API_KEY:?核心没有注入 API key}"

[ -d "$path" ] || mkdir "$path" || { echo "建根目录 $path 失败（共享文件夹不可写？）"; exit 1; }

get() {
  curl --noproxy '*' -fsS -H "X-Api-Key: $key" "$api/$1"
}

# API 就绪重试：/ping 过了但路由偶尔还会慢半拍。
list=""
i=0
while :; do
  if list=$(get rootfolder); then break; fi
  i=$((i + 1))
  [ "$i" -lt 15 ] || { echo "Radarr 的 API 一直没就绪"; exit 1; }
  sleep 2
done

existing=$(printf '%s' "$list" | jq '[.[] | select(.path == "'"$path"'")] | length')
if [ "$existing" -ge 1 ]; then
  echo "根目录 $path 已登记，不动"
  exit 0
fi

code=$(curl --noproxy '*' -sS -H "X-Api-Key: $key" -H 'Content-Type: application/json' \
  -d "{\"path\":\"$path\"}" -o /tmp/rootfolder-created.json -w '%{http_code}' "$api/rootfolder" || true)
case "$code" in
  200 | 201) echo "已建并登记电影根目录 $path" ;;
  *) echo "登记根目录失败（HTTP $code），应用返回："
     cat /tmp/rootfolder-created.json 2>/dev/null || true
     exit 1 ;;
esac
