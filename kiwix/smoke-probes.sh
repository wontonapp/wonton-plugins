# shellcheck shell=bash disable=SC2034
# kiwix 的冒烟探测。思路：
# - 全新安装时共享文件夹还没有 .zim：首页应是插件的「还没有内容」说明页（无内容如实显示）。
# - data_write 把一个很小的测试 ZIM 放进共享文件夹的「离线百科」子文件夹（插件的“数据”就在
#   共享文件夹里，插件本身不存任何用户数据），等扫描服务把它上架，再把搜索词记进 $SMOKE_STATE。
# - data_check 在 reinstall 之后证明：目录里列着这本书、首页不再是说明页、全文搜索有结果。
# - MCP：工具经总线来自 mcp/openapi.json（全 GET 只读）；目录工具不需要参数，用它做真实调用。
#
# 变量：SMOKE_WEB_URL 是插件网页入口（已去尾斜杠）；SMOKE_PLUGIN_ID/SMOKE_INSTANCE 由套件设好。
# 插件没有用户体系（IDENTITY_NA=1）。

# 测试 ZIM：Alpine Linux Wiki，约 3.1 MB，自带全文索引。dev.sh probes 时在仓库旁的
# test-data/ 里就有；smoke 把工作区复制到临时目录，找不到就走代理现下载。
TEST_ZIM_NAME=alpinelinux_en_all_maxi_2026-10.zim
TEST_ZIM_URL=https://download.kiwix.org/zim/other/$TEST_ZIM_NAME
TEST_BOOK_TITLE="Alpine Linux Wiki"
TEST_SEARCH_WORD=apk
SCAN_SUBDIR=离线百科

web_assert() { # 全新安装（空库）是说明页；有内容时是 kiwix 真实页面。两个分支断的是同一件事：网页经网关可达。
  if grep -q '还没有内容'; then
    return 0
  fi
  # 实测（3.8.2）：升级重建容器后紧跟的第一次页面请求可能只拿到页面前缀（连接早关，curl 仍算成功），
  # <title> 那段不在正文里；重取一次再断言，还不命中就照实失败（空库和有书时 / 都是同一份 SPA，
  # <title>Welcome to Kiwix Server</title> 恒在，实测核对过）。
  if grep -q 'Welcome to Kiwix Server'; then
    return 0
  fi
  local body
  body="$(web_get /)" || return 1
  grep -q 'Welcome to Kiwix Server' <<<"$body"
}

# 本实例里这个服务的容器（按核心打的标签找，smoke 与 dev.sh probes 两个环境都成立）
_container() { # <插件 id> <服务名>
  local inst="${SMOKE_INSTANCE:-}" args=(--filter "label=wonton.plugin=$1" --filter "label=wonton.service=$2")
  if [[ -z "$inst" && -n "${SMOKE_BASE:-}" ]]; then
    inst="${SMOKE_BASE#http://}"; inst="${inst%%.*}"
  fi
  [[ -z "$inst" ]] || args+=(--filter "label=wonton.instance=${inst//-/}")
  docker ps --format '{{.Names}}' "${args[@]}" | head -1
}

_test_zim() { # 输出测试 ZIM 的本机路径，没有就现下载到临时文件
  local local_copy="$SMOKE_DIR/../test-data/$TEST_ZIM_NAME"
  if [[ -s "$local_copy" ]]; then
    printf '%s\n' "$local_copy"
    return 0
  fi
  local dst="/tmp/pb-kiwix-$TEST_ZIM_NAME"
  if [[ ! -s "$dst" ]]; then
    # GitHub 的托管机能直连外网；本地开发环境直连不通时，经代理再试。先直连，失败再退代理。
    curl -fsSL --max-time 120 -o "$dst.part" "$TEST_ZIM_URL" ||
      curl -fsSL --max-time 120 --proxy "${https_proxy:-${HTTPS_PROXY:-http://127.0.0.1:7890}}" \
        -o "$dst.part" "$TEST_ZIM_URL" ||
      { rm -f "$dst.part"; echo "下载测试 ZIM 失败：$TEST_ZIM_URL"; return 1; }
    mv "$dst.part" "$dst"
  fi
  printf '%s\n' "$dst"
}

_share_dir() { # 共享文件夹 documents 在宿主上的路径：从容器挂载里读，不猜数据目录布局
  local c
  c="$(_container "$SMOKE_PLUGIN_ID" rescan)" || return 1
  [[ -n "$c" ]] || { echo "找不到 rescan 容器"; return 1; }
  docker inspect --format '{{range .Mounts}}{{if eq .Destination "/documents"}}{{.Source}}{{end}}{{end}}' "$c"
}

catalog_has() { # <要找的文字>：经网关查 kiwix 目录（首页列表的数据源）
  web_get /catalog/v2/entries | grep -qF "$1"
}

_place_zim() { # 放测试 ZIM 并等扫描服务上架（MCP 探针和 data_write 共用：目录里有书，工具才有东西可查）
  local zim share_dir dst i
  zim="$(_test_zim)" || return 1
  share_dir="$(_share_dir)" || { echo "读不到共享文件夹在宿主上的路径"; return 1; }
  dst="$share_dir/$SCAN_SUBDIR"
  mkdir -p "$dst" || { echo "建不了 $dst"; return 1; }
  cp "$zim" "$dst/$TEST_ZIM_NAME" || { echo "复制测试 ZIM 进共享文件夹失败"; return 1; }
  # 等扫描服务（30 秒一轮）把它上架；等着的时间不该超过两分钟。
  for ((i = 0; i < 24; i++)); do
    if catalog_has "$TEST_BOOK_TITLE"; then
      return 0
    fi
    sleep 5
  done
  echo "放了 $TEST_ZIM_NAME，两分钟内目录里没出现 $TEST_BOOK_TITLE"
  return 1
}

# MCP 探针：目录工具不需要参数；tools/list 里它必须标 readOnlyHint=true（探针套件核对），真实调用返回书目录。
MCP_TOOL=get_catalog_v2_entries
MCP_ARGS={}
mcp_assert() { grep -qF "$TEST_BOOK_TITLE"; }

before_mcp() { _place_zim; }

data_write() {
  _place_zim || return 1
  printf '%s\n' "$TEST_SEARCH_WORD" > "$SMOKE_STATE"
}

data_check() {
  local marker id
  marker="$(cat "$SMOKE_STATE")"
  [[ "$marker" == "$TEST_SEARCH_WORD" ]] || { echo "标记对不上：$marker"; return 1; }
  catalog_has "$TEST_BOOK_TITLE" || { echo "目录里没有 $TEST_BOOK_TITLE"; return 1; }
  # 首页不再是「还没有内容」说明页，而是 kiwix 自己的页面
  web_get / | grep -q 'Welcome to Kiwix Server' || { echo "首页不是 kiwix 页面（还是说明页？）"; return 1; }
  id="$(web_get /catalog/v2/entries | grep -oE 'urn:uuid:[a-f0-9-]+' | head -1)"
  [[ -n "$id" ]] || { echo "目录里取不到书的 uuid"; return 1; }
  # 全文搜索：查一个词要有结果（结果页是 “Results <b>1-25</b> of <b>N</b> for …”，N>0；
  # 零结果页是 “No results were found for …”，实测 3.8.2）。
  web_get "/search?books.id=${id#urn:uuid:}&pattern=$TEST_SEARCH_WORD" | grep -Eq 'of <b>[1-9][0-9]*</b>' \
    || { echo "搜索 $TEST_SEARCH_WORD 没有结果"; return 1; }
}

export IDENTITY_NA=1
export SMOKE_NOTE="sso none：kiwix-serve 没有账号体系，网页只在网关登录后面可达"
