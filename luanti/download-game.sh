#!/bin/sh
# 从 ContentDB 下载设置里选中的游戏，解压到数据卷的 games/ 下；已存在就跳过（幂等）。
# 一次性服务：每次创建容器都重跑（安装、升级、重建），装过的游戏不重复下载。
set -eu

# 核心不支持 settings（规范 v0.12）时不注入 WONTON_SETTING_*，
# 回退到清单默认值；核心支持时这里拿到的是设置值，行为不变。
GAME="${WONTON_SETTING_GAME:-mineclonia}"
[ -n "${WONTON_SETTING_GAME:-}" ] || echo "settings 没注入，用清单默认游戏：$GAME"
DATA="/var/lib/minetest"
# gameid（= 解压出来的目录名，引擎按目录名认游戏）→ ContentDB 的包。
# 包名和“维护中”状态 2026-11-12 在 content.luanti.org 核对过。
case "$GAME" in
	mineclonia) PKG=ryvnf/mineclonia ;;
	mineclone2) PKG=Wuzzy/mineclone2 ;;
	minetest_game) PKG=Luanti/minetest_game ;;
	*)
		echo "settings 的 game 值不在下载脚本名单里：$GAME" >&2
		exit 1
		;;
esac

DEST="$DATA/games/$GAME"
if [ -d "$DEST" ]; then
	echo "游戏已存在，跳过下载：$DEST"
	exit 0
fi

mkdir -p "$DATA/games"

# 最新 release 的 id 在包接口的 "release" 字段里，下载地址按 release id 拼。
# 注意下载地址会 302 到第二个域 cdn.content.luanti.org：出网代理要两个域都放行。
API_URL="https://content.luanti.org/api/packages/$PKG/"
if ! api_json="$(curl -fsSL "$API_URL" 2>&1)"; then
	echo "取不到 $API_URL：$api_json（查出网代理是否放行 content.luanti.org）" >&2
	exit 1
fi
RELEASE="$(printf '%s' "$api_json" | grep -o '"release":[0-9]*' | grep -o '[0-9]*' || true)"
if [ -z "$RELEASE" ]; then
	echo "$API_URL 的响应里没有 release 字段：$(printf '%s' "$api_json" | head -c 200)" >&2
	exit 1
fi

DL_URL="https://content.luanti.org/packages/$PKG/releases/$RELEASE/download/"
if ! curl -fsSL --retry 3 --connect-timeout 20 -o /tmp/game.zip "$DL_URL" 2>&1; then
	echo "下载失败：$DL_URL（上面是 curl 的报错；这个地址会 302 到 cdn.content.luanti.org，两个域都要能过出网代理）" >&2
	exit 1
fi

# 先解到临时目录，确认包里是单个 games/<gameid> 目录再就位；半途失败不留半个游戏目录。
TMP="$DATA/games/.downloading-$GAME"
rm -rf "$TMP"
mkdir -p "$TMP"
unzip -q /tmp/game.zip -d "$TMP"
rm -f /tmp/game.zip
if [ ! -d "$TMP/$GAME" ]; then
	rm -rf "$TMP"
	echo "下载包里没有 $GAME/ 目录（ContentDB 的目录结构和预期不符）" >&2
	exit 1
fi
mv "$TMP/$GAME" "$DEST"
rmdir "$TMP"
echo "游戏就绪：$DEST"
