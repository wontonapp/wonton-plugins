#!/bin/sh
# nvidia runtime 注入的是 libcuda.so.1；triton 编译内核要链接 libcuda.so，缺就补个软链再启动。
if [ -e /usr/lib/x86_64-linux-gnu/libcuda.so.1 ] && [ ! -e /usr/lib/x86_64-linux-gnu/libcuda.so ]; then
    ln -s libcuda.so.1 /usr/lib/x86_64-linux-gnu/libcuda.so
fi
# CMD 已经是完整的启动命令（python3 main.py …），原样透传。
exec "$@"
