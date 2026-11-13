#!/usr/bin/env bash
# shellcheck shell=bash
# BookStack 使用自己的账号登录；探针通过登录页、个人资料页和书籍页面验证应用。

bookstack_init_container() {
  local args=(--filter "label=wonton.plugin=$SMOKE_PLUGIN_ID" --filter "label=wonton.service=init")
  [[ -z "${SMOKE_INSTANCE:-}" ]] || args+=(--filter "label=wonton.instance=$SMOKE_INSTANCE")
  docker ps -a "${args[@]}" --format '{{.Names}}' | head -1
}

bookstack_password() {
  local container
  container="$(bookstack_init_container)"
  [[ -n "$container" ]] || { echo "找不到 BookStack 初始化容器" >&2; return 1; }
  docker inspect "$container" --format '{{range .Config.Env}}{{println .}}{{end}}' |
    sed -n 's/^WONTON_SECRET_ADMIN_PASSWORD=//p' | tail -1
}

bookstack_csrf() {
  sed -n 's/.*name="_token" value="\([^"]*\)".*/\1/p' | head -1
}

bookstack_login() {
  local page token password response
  page="$(dweb /login -L)" || { echo "读取 BookStack 登录页失败" >&2; return 1; }
  token="$(bookstack_csrf <<<"$page")"
  [[ -n "$token" ]] || { echo "BookStack 登录页没有 CSRF 令牌" >&2; return 1; }
  password="$(bookstack_password)"
  [[ -n "$password" ]] || { echo "初始化容器里没有 BookStack 管理员密码" >&2; return 1; }
  response="$(dweb /login -L --data-urlencode "_token=$token" \
    --data-urlencode "email=$SMOKE_OWNER_EMAIL" \
    --data-urlencode "password=$password")" || {
    echo "BookStack 登录请求失败" >&2
    return 1
  }
  grep -qi 'bookstack' <<<"$response" || {
    echo "BookStack 登录后没有返回应用页面" >&2
    return 1
  }
}

web_assert() {
  grep -qiE '<title[^>]*>[^<]*BookStack|BookStack[^<]*</title>'
}

identity_check() {
  local profile
  bookstack_login || return 1
  profile="$(dweb /my-account/profile)" || { echo "读取 BookStack 个人资料失败：$profile"; return 1; }
  grep -qF "$SMOKE_OWNER_EMAIL" <<<"$profile" || {
    echo "BookStack 当前账号不是主人邮箱 $SMOKE_OWNER_EMAIL：$(head -c 300 <<<"$profile")"
    return 1
  }
}

data_write() {
  local marker page token response
  marker="$(head -c 12 /dev/urandom | base64 | tr -d '=+/' | head -c 16)"
  [[ -n "$marker" ]] || { echo "生成 BookStack 数据标记失败"; return 1; }
  bookstack_login || return 1
  page="$(dweb /create-book)" || { echo "读取 BookStack 新建书籍页面失败"; return 1; }
  token="$(bookstack_csrf <<<"$page")"
  [[ -n "$token" ]] || { echo "BookStack 新建书籍页面没有 CSRF 令牌"; return 1; }
  response="$(dweb /books -L --data-urlencode "_token=$token" \
    --data-urlencode "name=wonton-smoke-$marker" \
    --data-urlencode "description_html=$marker")" || {
    echo "BookStack 创建持久化书籍失败" >&2
    return 1
  }
  grep -qF "wonton-smoke-$marker" <<<"$response" || {
    echo "BookStack 创建后找不到标记书籍：$(head -c 300 <<<"$response")"
    return 1
  }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() {
  local marker books
  [[ -f "${SMOKE_STATE:-}" && -s "$SMOKE_STATE" ]] || {
    echo "BookStack 标记文件里没有数据：先跑 data_write"
    return 1
  }
  marker="$(cat "$SMOKE_STATE")"
  books="$(dweb /books)" || { echo "读取 BookStack 书籍列表失败：$books"; return 1; }
  grep -qF "wonton-smoke-$marker" <<<"$books" || {
    echo "BookStack 书籍列表里找不到 $marker：$(head -c 400 <<<"$books")"
    return 1
  }
}
