#!/bin/sh
# 等 Xvfb 的 socket 出现再启动，避免启动竞态让 supervisor 反复重启。
n=0
while [ ! -S /tmp/.X11-unix/X99 ] && [ "$n" -lt 100 ]; do
  n=$((n + 1))
  sleep 0.2
done
[ -S /tmp/.X11-unix/X99 ] || exit 1
# 每次重建容器主机名都变，旧主机留下的 SingletonLock 会让 Chromium 弹“profile 被另一台电脑占用”
# 的对话框等人点，DevTools 端口一直不开。同一份数据目录只有一个容器在用，启动前清掉锁文件。
rm -f /config/profile/SingletonLock /config/profile/SingletonCookie /config/profile/SingletonSocket
# 显式窗口位置和大小：容器里没有窗口管理器，--start-maximized 不生效。
# 不开 --remote-allow-origins：CDP 中转按 Host 白名单挡网页（见 cdp-relay.conf），
# DevTools 的 Origin 校验保持默认，没带 Origin 的客户端（Playwright）照样连得上。
# --no-sandbox 是必须的：Wonton 的 compose 子集不给 cap_add/security_opt，容器里
# unshare(CLONE_NEWUSER) 被 Docker 默认 seccomp 拦住（实测 EPERM），Chromium 的沙箱
# 起不来就自杀（SIGTRAP）；容器本身是唯一的一层隔离。
exec chromium \
  --user-data-dir=/config/profile \
  --no-sandbox \
  --no-first-run \
  --no-default-browser-check \
  --hide-crash-restore-bubble \
  --disable-crash-reporter \
  --window-position=0,0 \
  --window-size=1280,800 \
  --remote-debugging-port=19222 \
  about:blank \
  "$@"
