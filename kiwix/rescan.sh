#!/bin/sh
# 每 30 秒扫描共享文件夹「文档」的「离线百科」子文件夹，用上游 kiwix-manage 重建 kiwix 库文件。
# kiwix-serve 带 --monitorLibrary，库文件一换就自动重载，新放入的 .zim 不用重启。
#
# 空库（一个 .zim 都没读到）时：写空的库骨架让 kiwix-serve 照常运行（实测 3.8.2 对空的
# <library> 元素不退出），并把「还没有内容」说明页复制进 state/web/ ——nginx 以这个文件的
# 有无决定首页显示说明页还是转发；有内容时撤掉说明页。
#
# 坏文件只影响自己：逐个 add，kiwix-manage 对损坏的 .zim 报错退出 1，不会写坏库文件。
set -u
set -f

LIBRARY_DIR=/library
LIBRARY=$LIBRARY_DIR/library.xml
SCAN_DIR=/documents/离线百科
FP=$LIBRARY_DIR/.scan-fp
HEARTBEAT=$LIBRARY_DIR/.rescan-heartbeat
EMPTY_PAGE=/srv/empty.html
EMPTY_MARK=/state/web/empty.html
INTERVAL=30

skeleton() {
  printf '<?xml version="1.0" encoding="UTF-8"?>\n<library version="20110515"></library>\n'
}

# 指纹 = 所有 .zim 的大小、修改时间和路径。文件增删或被替换都会变；隐藏目录（如回收站）不算。
scan_key() {
  find "$SCAN_DIR" -name '.*' -prune -o -type f -name '*.zim' -exec stat -c '%s %Y %n' {} \; 2>/dev/null | sort | md5sum | cut -d' ' -f1
}

list_zims() {
  find "$SCAN_DIR" -name '.*' -prune -o -type f -name '*.zim' -print 2>/dev/null | sort
}

while :; do
  key=$(scan_key)
  if [ "$(cat "$FP" 2>/dev/null)" != "$key" ] || [ ! -f "$LIBRARY" ]; then
    rm -f "$LIBRARY.new"
    find "$SCAN_DIR" -name '.*' -prune -o -type f -name '*.zim' -print 2>/dev/null | sort > /tmp/zimlist
    ok=0
    while IFS= read -r f; do
      # kiwix-manage 存的是相对库文件目录的路径（实测一律相对化，--zimPathToSave 也不例外），
      # 库文件固定在 /library、共享文件夹固定在 /documents，两个容器挂载点一致，相对路径成立。
      if kiwix-manage "$LIBRARY.new" add "$f" >/dev/null 2>&1; then
        ok=$((ok + 1))
      else
        echo "rescan: 跳过读不了的 $f"
      fi
    done < /tmp/zimlist
    if [ "$ok" -gt 0 ]; then
      mv "$LIBRARY.new" "$LIBRARY"
      rm -f "$EMPTY_MARK"
    else
      skeleton > "$LIBRARY.new" && mv "$LIBRARY.new" "$LIBRARY"
      cp "$EMPTY_PAGE" "$EMPTY_MARK"
    fi
    printf '%s\n' "$key" > "$FP"
  fi
  touch "$HEARTBEAT"
  sleep "$INTERVAL"
done
