# shellcheck shell=bash
# scripts/smoke.sh / dev.sh 探针的一项一项探测，跟着插件走。没写的项会 FAIL 并给出写法，不会默认放行。
#
# Excalidraw 是纯静态应用：nginx 直接发构建好的页面，没有服务端逻辑。
#
# 页面：web_assert 从标准输入读页面，命中的确是应用的真实页面时返回 0。
# 身份：应用没有任何账号体系（登录页、当前用户接口都不存在），网关注入的头也没有消费方，
#       写 IDENTITY_NA=1，并在 SMOKE_NOTE 里写原因。两个变量由冒烟套件 source 后读取。
# 数据：服务端不存任何用户数据——画布只存在浏览器的 localStorage 和导出文件里，
#       升级、重装、删容器都不涉及服务端数据，写 DATA_NA=1。
export IDENTITY_NA=1
export SMOKE_NOTE="应用没有账号体系（纯静态画布，数据只存在浏览器本地），身份项只验证网关把关"
export DATA_NA=1

web_assert() { grep -q '<title>Excalidraw Whiteboard</title>'; }   # 应用页面的标题
