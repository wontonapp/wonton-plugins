#!/usr/bin/env python3
"""按改动的路径算受影响的插件子目录，写进 GITHUB_OUTPUT。

改了 scripts/ 或工作流本身，就是全部插件受影响（套件和检查规则只有一份）；只改 <id>/ 的，只算那一个插件。
手动触发算全部。用法（工作流里）：EVENT / BASE_SHA / HEAD_SHA 从 github 上下文传进来。
"""

import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
SHARED = ("scripts/", ".github/")


def changed_files() -> list[str]:
    event = os.environ.get("EVENT", "")
    if event == "workflow_dispatch":
        return [str(path.relative_to(ROOT)) for path in ROOT.rglob("*")]
    base_sha = os.environ.get("BASE_SHA", "")
    head_sha = os.environ.get("HEAD_SHA", "")
    if event == "pull_request":
        if not base_sha:
            sys.exit("pull_request 事件没有 base.sha")
        return diff(base_sha, head_sha)
    # push：新分支的 before 全是 0，和第一个父提交比；第一个提交没有父时把所有文件都算上。
    if base_sha and set(base_sha) != {"0"}:
        return diff(base_sha, head_sha)
    parent = subprocess.run(["git", "rev-parse", "--verify", "-q", f"{head_sha}^"],
                            capture_output=True, text=True)
    if parent.returncode == 0:
        return diff(parent.stdout.strip(), head_sha)
    return [str(path.relative_to(ROOT)) for path in ROOT.rglob("*") if path.is_file()]


def diff(base_sha: str, head_sha: str) -> list[str]:
    result = subprocess.run(["git", "diff", "--name-only", "--no-renames", base_sha, head_sha],
                            capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(f"git diff 失败：{result.stderr.strip()}")
    return result.stdout.splitlines()


def plugin_dirs() -> list[str]:
    if not ROOT.is_dir():
        return []
    return sorted(path.name for path in ROOT.iterdir()
                  if path.is_dir() and not path.name.startswith(".")
                  and (path / "docker-compose.yml").exists())


def main() -> None:
    files = changed_files()
    shared = any(name.startswith(SHARED) for name in files)
    plugins = plugin_dirs()
    if not shared:
        hit = {name.split("/")[0] for name in files if "/" in name}
        plugins = [name for name in plugins if name in hit]
    output = Path(os.environ["GITHUB_OUTPUT"])
    with output.open("a", encoding="utf-8") as stream:
        stream.write(f"plugins={json.dumps(plugins)}\n")
        stream.write(f"shared={'true' if shared else 'false'}\n")


if __name__ == "__main__":
    main()
