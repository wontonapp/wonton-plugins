# shellcheck shell=bash
# draw.io 的冒烟探测：网页是编辑器本身，且页面与配置都不引用站外地址。
#
# 应用没有账号体系（sso: none，网页只在本机网关登录后面可达），身份项不适用；
# 图存在用户浏览器的 localStorage、导出的文件里，服务器不存任何数据，数据项不适用。
export IDENTITY_NA=1
export DATA_NA=1
export SMOKE_NOTE="应用没有账号体系，网页只在本机网关登录后面可达；图存在用户浏览器的 localStorage 和导出的文件里，服务器不存数据"

web_assert() {
  local page pre post refs ext line
  page="$(cat)"

  # 页面是 draw.io 编辑器本身
  grep -q 'class="geEditor' <<<"$page" || { echo "页面没有 geEditor（不是 draw.io 编辑器）"; return 1; }
  grep -q 'js/bootstrap.js' <<<"$page" || { echo "页面没有 bootstrap.js"; return 1; }

  # 浏览器会去取的资源必须都指向本站：<script src>、<link href>、<img src>。
  # <link rel="canonical"> 和 itemprop 的图片 meta 是纯元数据，浏览器不取，不算引用（README 有说明）。
  refs="$(
    {
      grep -oE '<script[^>]+src="[^"]*"' <<<"$page" | sed -E 's/.*src="([^"]*)"/\1/'
      grep -oE '<link[^>]+href="[^"]*"' <<<"$page" | grep -v 'rel="canonical"' | sed -E 's/.*href="([^"]*)"/\1/'
      grep -oE '<img[^>]+src="[^"]*"' <<<"$page" | sed -E 's/.*src="([^"]*)"/\1/'
    } | grep -E '^https?://' | grep -v "^$SMOKE_WEB_ORIGIN" || true
  )"
  [[ -z "$refs" ]] || { echo "页面引用了站外资源：$refs"; return 1; }

  # 入口脚本按环境变量生成的配置：CSP 收紧到本站（出现任何 https:// 都说明放行了站外源），
  # 云端集成、实时协作、导出服务、图标搜索、日志上报全部关闭。
  pre="$(dweb /js/PreConfig.js)" || pre="$(dweb /js/PreConfig.js)" || { echo "取 PreConfig.js 失败"; return 1; }
  post="$(dweb /js/PostConfig.js)" || post="$(dweb /js/PostConfig.js)" || { echo "取 PostConfig.js 失败"; return 1; }
  line="$(grep -F "s.setAttribute('content'" <<<"$pre")"
  [[ -n "$line" ]] || { echo "PreConfig.js 没有注入 CSP"; return 1; }
  if grep -qE 'https?://' <<<"$line"; then echo "CSP 放行了站外源：$line"; return 1; fi
  for pat in \
    "window.EXPORT_URL = null" \
    "urlParams['sync'] = 'manual'" \
    "urlParams['db'] = '0'" \
    "urlParams['gh'] = '0'" \
    "urlParams['tr'] = '0'" \
    "urlParams['gapi'] = '0'" \
    "urlParams['od'] = '0'" \
    "urlParams['gl'] = '0'"; do
    grep -qF "$pat" <<<"$pre" || { echo "PreConfig.js 缺少 $pat（集成没关掉）"; return 1; }
  done
  for pat in "window.ICONSEARCH_PATH = null" "window.ICON_SERVICE_PATH = null" "EditorUi.enableLogging = false"; do
    grep -qF "$pat" <<<"$post" || { echo "PostConfig.js 缺少 $pat"; return 1; }
  done

  # 生成的配置和 PWA manifest（浏览器打开页面就会取）不引用站外地址；
  # DRAWIO_SERVER_URL 是本站公开地址，允许。
  ext="$(
    {
      grep -oE 'https?://[^"'\'' ]+' <<<"$pre$post"
      dweb /images/manifest.json | grep -oE 'https?://[^"'\'' ]+'
    } | grep -v "^$SMOKE_WEB_ORIGIN" | sort -u || true
  )"
  [[ -z "$ext" ]] || { echo "配置或 manifest 引用了站外地址：$ext"; return 1; }
}
