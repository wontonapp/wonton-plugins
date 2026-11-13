#!/usr/bin/env python3
"""发布 tag 的校验：`<id>/v<版本>`，版本要和插件清单（x-wonton.version）一致。

输出 kind / id / version 到 GITHUB_OUTPUT；写错了退出 1，发错 tag 不能悄悄发出别的版本。
"""

import os
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent


def manifest_version(plugin: str) -> str:
    manifest = ROOT / plugin / "docker-compose.yml"
    if not manifest.is_file():
        sys.exit(f"没有 {manifest.relative_to(ROOT)}")
    text = manifest.read_text(encoding="utf-8")
    block = re.search(r"^x-wonton:\n((?:[ \t]+.*\n?)+)", text, re.M)
    found = re.search(r"^  version:\s*['\"]?([^'\"\s]+)", block.group(1), re.M) if block else None
    if not found:
        sys.exit(f"{manifest.relative_to(ROOT)} 的 x-wonton 没有 version")
    return found.group(1)


def main() -> None:
    ref = sys.argv[1]
    name, _, version = ref.rpartition("/v")
    if not name or not version or "/" in name:
        sys.exit(f"tag 要写成 <id>/v<版本>：{ref}")
    expected = manifest_version(name)
    if expected != version:
        sys.exit(f"tag 的版本 {version} 和仓库里的 {expected} 不同")
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
        stream.write(f"kind=plugin\nid={name}\nversion={version}\n")


if __name__ == "__main__":
    main()
