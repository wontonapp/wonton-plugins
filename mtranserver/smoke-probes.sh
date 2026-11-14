# shellcheck shell=bash
# MTranServer：无账号体系，应用只认 API 令牌（secrets 生成）；网页经网关代注入 Bearer，
# 带用户会话即可调翻译接口，探针不需要令牌。
export IDENTITY_NA=1
export SMOKE_NOTE="无账号体系，API 令牌由网关代注入；局域网令牌在详情页（show_owner）"

web_assert() { grep -qi 'MTranServer'; }   # 标准输入：SPA 首页 HTML 的 <title>

# MCP 探针：mcp_check 一次核对两件事——
# 1) tools/list 里 POST 的翻译工具也标了 readOnlyHint=true（spec 的 summary 以查询动词开头，
#    适配器据此标注；没标的话总线上每次翻译都要弹确认卡）；
# 2) 真实调用只读工具 get_languages，返回支持的语言（不依赖翻译模型，首次安装即有）。
mcp_check() {
  local tool out
  for tool in post_detect post_translate post_translate_batch; do
    if [[ "$(jq -r --arg s "$SMOKE_SLUG" --arg t "$tool" \
        '.[] | select(.name == $s + "__" + $t) | .annotations.readOnlyHint // "missing"' "$SMOKE_TOOLS")" != true ]]; then
      echo "$tool 没标只读"
      return 1
    fi
  done
  out="$(mcp call "${SMOKE_SLUG}__get_languages" '{}')" || return 1
  jq -e '.content[0].text | fromjson | (.languages | index("en")) and (.languages | index("zh-Hans"))' \
    <<<"$out" >/dev/null || { echo "语言列表里没有 en / zh-Hans：$(head -c 200 <<<"$out")"; return 1; }
}

# 插件的用户数据是下载好的翻译模型和配置：翻一句带随机标记的英文（首次触发 en→zh-Hans 的模型下载，
# 可能要几分钟），标记存进 $SMOKE_STATE，重装后核对。

# 按实例和插件标签定位，避免匹配其他开发实例的同名服务。
plugin_container() {
  local inst="${SMOKE_INSTANCE:-${SMOKE_BASE#http://}}"
  inst="${inst%%.*}"
  docker ps --format '{{.Names}}' --filter "label=wonton.instance=${inst//-/}" \
    --filter 'label=wonton.plugin=app.github.xxnuo.mtranserver' --filter 'label=wonton.service=mtranserver' | head -1
}

data_write() {
  local marker out
  marker="wonton smoke $(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
  out="$(dweb /translate -X POST -H 'Content-Type: application/json' \
    --data "{\"from\":\"en\",\"to\":\"zh-Hans\",\"text\":\"$marker\"}")" ||
    { echo "翻译请求失败：$out"; return 1; }
  jq -er '.result // empty' <<<"$out" | grep -q . || { echo "译文为空或形状不对：$out"; return 1; }
  printf '%s' "$marker" >"$SMOKE_STATE"
}

data_check() { # 模型目录还在（en_zh-Hans），同一句翻得出来
  local marker out box files
  marker="$(cat "$SMOKE_STATE")"
  [[ -n "$marker" ]] || { echo "SMOKE_STATE 里没有标记"; return 1; }
  box="$(plugin_container)"
  [[ -n "$box" ]] || { echo "找不到本实例的 mtranserver 容器"; return 1; }
  files="$(docker exec "$box" sh -c 'ls /app/models 2>/dev/null')" ||
    { echo "列不出模型目录"; return 1; }
  grep -q 'en_zh-Hans' <<<"$files" || { echo "模型目录里没有 en_zh-Hans，下载的模型没保住：$files"; return 1; }
  out="$(dweb /translate -X POST -H 'Content-Type: application/json' \
    --data "{\"from\":\"en\",\"to\":\"zh-Hans\",\"text\":\"$marker\"}")" ||
    { echo "翻译请求失败：$out"; return 1; }
  jq -er '.result // empty' <<<"$out" | grep -q . || { echo "重装后翻译不出结果：$out"; return 1; }
}
