# shellcheck shell=bash disable=SC2034
# IDENTITY_NA/DATA_NA/SMOKE_NOTE 由探针套件（scripts/probes.sh）读取。

web_assert() { grep -qi '<title>ComfyUI</title>'; }

# ComfyUI 没有可断言的用户体系：单用户、无登录，能打开网页的人就是网关认出的用户。
IDENTITY_NA=1
SMOKE_NOTE="ComfyUI 单用户无登录，没有当前用户接口；网关会话挡在应用前面"

# 用户数据：保存的工作流存在数据目录 user/default/workflows/（经 userdata API 写读），
# 升级和重装都不该丢。
data_write() {
  local marker body
  marker="wonton-$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
  body="$(jq -nc --arg m "$marker" '{"marker":$m,"nodes":[]}')"
  web_get "/userdata/workflows%2Fsmoke-$marker.json?overwrite=true" \
    -X POST --data-binary "$body" -H 'Content-Type: application/json' >/dev/null ||
    { echo "工作流 smoke-$marker.json 写不进去"; return 1; }
  printf '%s' "$marker" > "$SMOKE_STATE" || { echo "存不了标记：$SMOKE_STATE"; return 1; }
}

data_check() {
  local marker got
  marker="$(cat "$SMOKE_STATE" 2>/dev/null)" || { echo "读不到标记"; return 1; }
  got="$(web_get "/userdata/workflows%2Fsmoke-$marker.json")" ||
    { echo "工作流 smoke-$marker.json 读不回来"; return 1; }
  grep -qF "\"marker\":\"$marker\"" <<<"$got" ||
    { echo "读回的内容对不上：$(head -c 200 <<<"$got")"; return 1; }
}
