# Bot 的浏览器：Debian 仓库里的 Chromium，配上能实时观看和接手的画面（Xvfb + x11vnc + noVNC），
# 和把 Chromium 只肯绑在 127.0.0.1 的 CDP 端口转到插件网络的 nginx（headful Chromium 不认
# --remote-debugging-address，见 docker-compose.yml 里的注释）。全部组件来自 Debian 仓库，
# 本镜像不含应用代码，amd64 和 arm64 都能构建。
#
# 可复现：基础镜像按摘要固定，apt 源固定到同一时刻的 snapshot（包版本随之固定）。换 snapshot
# 时刻要重新验证 Chromium 的实测行为（CDP Host 改写、ws 地址拼接，见 cdp-relay.conf）。
FROM debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4

ARG SNAPSHOT=20261103T082542Z
RUN sed -i "s|http://deb.debian.org/debian-security|http://snapshot.debian.org/archive/debian-security/${SNAPSHOT}|g; s|http://deb.debian.org/debian|http://snapshot.debian.org/archive/debian/${SNAPSHOT}|g" /etc/apt/sources.list.d/debian.sources \
 && apt-get -o Acquire::Check-Valid-Until=false update \
 && apt-get -o Acquire::Check-Valid-Until=false install -y --no-install-recommends \
      chromium \
      xvfb \
      x11vnc \
      novnc \
      websockify \
      python3-numpy \
      nginx \
      supervisor \
      ca-certificates \
      fonts-noto-cjk \
      fonts-noto-color-emoji \
 && rm -rf /var/lib/apt/lists/*

COPY supervisord.conf /etc/wonton/supervisord.conf
COPY cdp-relay.conf /etc/wonton/cdp-relay.conf
COPY index.html /usr/share/novnc/index.html
COPY run.sh start-chromium.sh /usr/local/bin/
RUN chmod 0755 /usr/local/bin/run.sh /usr/local/bin/start-chromium.sh

# compose 里以 user: "${PUID}:${PGID}" 运行整个容器。
CMD ["/usr/local/bin/run.sh"]
