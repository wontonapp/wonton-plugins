#!/bin/sh
# 思源服务的健康检查：带着访问授权码（HTTP Basic，用户名固定是工作区名 workspace）调
# lsNotebooks，走到工作区的 SQLite 索引并返回笔记本列表，而不是只看进程活着。
# 授权码经 SIYUAN_ACCESS_AUTH_CODE 注入；-Y off 是必须的：busybox wget 不看 no_proxy，
# 宿主给容器注入的 http_proxy 会把回环请求发去代理（规范「网络」一节）。
auth="$(printf 'workspace:%s' "$SIYUAN_ACCESS_AUTH_CODE" | base64)"
exec wget -Y off -qO- \
    --header='Content-Type: application/json' \
    --header="Authorization: Basic $auth" \
    --post-data='{}' \
    http://127.0.0.1:6806/api/notebook/lsNotebooks
