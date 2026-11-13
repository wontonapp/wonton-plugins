# shellcheck shell=bash
# scripts/smoke.sh 的探针：Calibre-Web Automated。
# sso=oidc（云吞登录，主人带 admin 组、家人自动建号）；OPDS 走应用自带 Basic Auth；清单没有 mcp。
# 变量别叫 state：shellcheck 把这份文件连着 probes.sh 一起分析，probes.sh 自己也读一个 dev.sh 提供的 state；
# 子壳体函数（fn() ( … )）里给 state 赋值，SC2031 就报在 probes.sh 上，CI 红。

app_session() { # 走完云吞→OIDC 跳转，让 jar 里带上应用的登录态（--sso 会重建 jar，重试一次抗抖动）
  dweb --sso / >/dev/null 2>&1 || dweb --sso / >/dev/null 2>&1
}

web_assert() { # 未登录会被网关挡下；带会话到达的是应用登录页（OIDC 入口由 webui.path 直达，不依赖按钮）
  grep -q 'Calibre-Web Automated'
}

identity_check() { # 应用自己的用户表接口（仅管理员）：认出的是用户的、有管理员位的账号
  local users
  app_session || { echo "OIDC 登录跳转走不通"; return 1; }
  users="$(dweb "/ajax/listusers?limit=50")" || { echo "listusers 失败：$users"; return 1; }
  jq -e --arg u "$SMOKE_OWNER" \
    '.rows | map(select(.name == $u)) | length == 1 and (.[0].role % 2) == 1' <<<"$users" >/dev/null ||
    { echo "用户表里没有 $SMOKE_OWNER 的管理员账号：$(head -c 200 <<<"$users")"; return 1; }
}

marker_file() { # 标记文件：套件传状态目录时放目录下，传文件本身时放旁边
  if [[ -d "$SMOKE_STATE" ]]; then printf '%s' "$SMOKE_STATE/cwa-marker"; else printf '%s' "${SMOKE_STATE}.marker"; fi
}

_container() { # 本实例里这个服务的容器（插件 id 是本仓库常量；PROBES_PLUGIN_ID 不随 e2e 的子壳体传进来）
  local inst="${SMOKE_INSTANCE:-}" args=(
    --filter "label=wonton.plugin=app.calibre-web-automated.books" --filter "label=wonton.service=calibre-web-automated")
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

data_write() (
  local marker c
  marker="wonton-smoke-$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
  c="$(_container)"
  [[ -n "$c" ]] || { echo "找不到插件容器"; return 1; }
  # 用应用自带的 calibredb 往书库加一本以标记命名的书（显式 --title，入库处理不联网改标题）。
  if ! { docker exec "$c" sh -c "printf 'wonton data probe\n' > /tmp/$marker.txt" &&
    docker exec "$c" calibredb add --library-path /calibre-library --title "$marker" "/tmp/$marker.txt" &&
    docker exec "$c" rm -f "/tmp/$marker.txt"; }; then
    echo "calibredb add 失败"
    return 1
  fi
  for _ in $(seq 1 120); do
    if docker exec "$c" sqlite3 /calibre-library/metadata.db \
      "SELECT count(*) FROM books WHERE title = '$marker'" 2>/dev/null | grep -q '^1$'; then
      printf '%s' "$marker" >"$(marker_file)"
      echo "书已入库：$marker"
      return 0
    fi
    sleep 2
  done
  echo "4 分钟内没在 metadata.db 里看到 $marker"
  return 1
)

data_check() (
  local marker c found
  marker="$(cat "$(marker_file)")"
  [[ -n "$marker" ]] || { echo "标记文件里没有标记"; return 1; }
  c="$(_container)"
  [[ -n "$c" ]] || { echo "找不到插件容器"; return 1; }
  found="$(docker exec "$c" sqlite3 /calibre-library/metadata.db \
    "SELECT count(*) FROM books WHERE title = '$marker'" 2>/dev/null)" ||
    { echo "读 metadata.db 失败"; return 1; }
  [[ "$found" == "1" ]] || { echo "书库里没有 $marker（count=$found）"; return 1; }
  # 应用（不只数据库）还把它列出来：首页默认按"最新"排，登录后的会话能看到。
  app_session || { echo "OIDC 登录跳转走不通"; return 1; }
  if grep -qF "$marker" <<<"$(dweb / 2>/dev/null)"; then
    echo "书还在，应用也列得出来：$marker"
    return 0
  fi
  echo "metadata.db 里有 $marker，但应用没列出来"
  return 1
)
