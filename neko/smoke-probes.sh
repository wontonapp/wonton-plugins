# shellcheck shell=bash
# neko 的冒烟探针。neko 自带多用户登录：账号密码是清单 secrets 生成、经环境变量注入容器的值；
# multiuser 模式下用户名随意、密码定身份（上游 member/multiuser/provider.go 的 Authenticate 只比密码）。
# 不存用户数据：成员密码在环境变量、会话在内存、浏览器配置不持久，升级/重装没有要找回的数据。

web_assert() { grep -qiF '<title>n.eko</title>'; }

export SMOKE_NOTE="neko 不存用户数据：成员密码在环境变量、会话在内存、浏览器配置不持久"
export DATA_NA=1

_container() { # <服务名>：本实例里这个服务的容器（按核心打的标签找，自己拼名字会缺段）
  local inst="${SMOKE_INSTANCE:-}" args=(--filter "label=wonton.plugin=$SMOKE_PLUGIN_ID" --filter "label=wonton.service=$1")
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

_neko_login() { # <密码>：走 neko 自己的登录接口，成功打印 SessionData（token 在 .token），失败打印原因并返回非 0
  local pass="$1" body resp
  body="$(jq -cn --arg p "$pass" '{username: "wonton-smoke", password: $p}')"
  resp="$(web_get /api/login -X POST -H 'Content-Type: application/json' -d "$body")" ||
    { echo "登录请求失败：$resp"; return 1; }
  jq -e '.token != null' <<<"$resp" >/dev/null ||
    { echo "登录响应里没有令牌：$(head -c 200 <<<"$resp")"; return 1; }
  printf '%s' "$resp"
}

_neko_whoami() { # <密码>：登录拿令牌后调 neko 的当前用户接口，成功打印 SessionData
  local pass="$1" login token who
  login="$(_neko_login "$pass")" || return 1
  token="$(jq -r '.token' <<<"$login")"
  who="$(web_get /api/whoami -H "Authorization: Bearer $token")" ||
    { echo "whoami 请求失败：$who"; return 1; }
  jq -e '.profile.is_admin != null' <<<"$who" >/dev/null ||
    { echo "whoami 响应里没有 profile：$(head -c 200 <<<"$who")"; return 1; }
  printf '%s' "$who"
}

identity_check() { # 管理员密码认出管理员，普通用户密码认出非管理员（两把密码都来自清单 secrets）
  local admin_pass user_pass admin user container
  container="$(_container neko)"
  [[ -n "$container" ]] || { echo "找不到 neko 容器"; return 1; }
  admin_pass="$(docker exec "$container" printenv NEKO_MEMBER_MULTIUSER_ADMIN_PASSWORD)" ||
    { echo "容器里读不到管理员密码"; return 1; }
  user_pass="$(docker exec "$container" printenv NEKO_MEMBER_MULTIUSER_USER_PASSWORD)" ||
    { echo "容器里读不到普通用户密码"; return 1; }
  admin="$(_neko_whoami "$admin_pass")" || return 1
  user="$(_neko_whoami "$user_pass")" || return 1
  jq -e '.profile.is_admin == true and .profile.can_login == true' <<<"$admin" >/dev/null ||
    { echo "管理员登录没被认成管理员：$(head -c 200 <<<"$admin")"; return 1; }
  jq -e '.profile.is_admin == false' <<<"$user" >/dev/null ||
    { echo "普通用户密码登录没被认成普通身份：$(head -c 200 <<<"$user")"; return 1; }
}
