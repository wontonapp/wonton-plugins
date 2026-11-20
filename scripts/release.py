#!/usr/bin/env python3
"""一条命令发版：确认插件在 main 上的版本已经合入、检查全绿，再串行打 tag、等发布成功、把版本收录进商店。

在 wonton-plugins 仓库根目录跑，只发本仓库的插件。商店里还指向私有仓库（`wonton-plugin-<id>` 之类）的条目
会顺带迁到本仓库路径，所以这类 PR 带 remove-plugin 标签。

    scripts/release.py <插件id>… --store ../wonton-store [--dry-run] [--upgrade-steps]

顺序（任意一步失败就停，报哪一步、为什么，不往下走）：
  1. 每个插件：main 上最新的提交改过它、来自已合的 PR、那个提交上它的检查全绿、版本比上一个 tag 新。
  2. 逐个打并推 tag，等发布工作流跑完，确认成功、镜像摘要已写回。
  3. 在商店 PR 里改 update.sh、跑 update.sh 重新生成 index.json、提 PR，等检查绿了合并。
  4. --upgrade-steps 额外打印升级测试机的步骤（不碰服务器）。

发布过的 tag 不移动、不删除；失败后修好原因，重跑同一条命令接着做。
--dry-run 不推远端、不改商店，但会 git fetch --tags，并可能删掉本地遗留的同名 tag（远端没有它时）。
"""

import argparse
import contextlib
import json
import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

REPO = "wontonapp/wonton-plugins"
STORE = "wontonapp/wonton-store"
PUBLISH = f"repos/{REPO}/actions/workflows/publish.yml/runs"
GOOD = {"success", "skipped", "neutral"}
BRANCH = "main"

UPGRADE_STEPS = """\
升级测试机（可选），二选一：
  A. 在 App 里对每个插件卸载（保留数据）后重新安装。
  B. 停掉核心，跑下面的脚本改已装插件记录，再启动新核心：
       python3 refresh-records.py <数据目录>/core/plugins
     脚本见 https://github.com/wontonapp/wonton/issues/62 的评论。
升级前两个检查：
  docker network inspect $(docker network ls -q) --format '{{.Name}} {{.IPAM.Config}}' | grep -v IPRange
  grep -rn WONTON_OWNER_ <数据目录>/core/plugins
"""


class Stop(Exception):
    """一步没做成；main 打印是哪一步后退出 1。"""


@dataclass
class Plan:
    plugin: str
    version: str
    tag: str
    commit: str
    images: list[str]
    released: bool = False


@dataclass
class Runner:
    root: Path
    store: Path
    poll: float
    timeout: float
    idle_timeout: float
    base: str = ""
    step: str = "准备"

    @contextlib.contextmanager
    def doing(self, step: str):
        self.step = step
        print(f"\n== {step}")
        yield


def sh(argv: list[str], cwd: Path, timeout: float | None = None) -> str:
    try:
        done = subprocess.run(
            argv, cwd=cwd, text=True, capture_output=True, timeout=timeout
        )
    except FileNotFoundError:
        raise Stop(f"找不到命令 {argv[0]}") from None
    except subprocess.TimeoutExpired:
        raise Stop(f"{' '.join(argv)} 超过 {timeout} 秒没结束") from None
    if done.returncode:
        raise Stop(f"{' '.join(argv)} 失败：{(done.stderr or done.stdout).strip()}")
    return done.stdout


def gh(*argv: str, timeout: float = 120) -> str:
    return sh(["gh", *argv], Path.cwd(), timeout)


def gh_json(path: str) -> dict | list:
    return json.loads(gh("api", path))


def manifest_version(text: str) -> str:
    block = re.search(r"^x-wonton:\n((?:[ \t]+.*\n?)+)", text, re.M)
    found = re.search(r"^  version:\s*['\"]?([^'\"\s]+)", block.group(1), re.M) if block else None
    if not found:
        raise Stop("清单的 x-wonton 没有 version")
    return found.group(1)


def show(root: Path, ref: str, path: str) -> str:
    return sh(["git", "show", f"{ref}:{path}"], root)


def natural(text: str) -> list:
    return [(0, int(part)) if part.isdigit() else (1, part) for part in re.split(r"[.\-+]", text)]


def remote_tag(root: Path, tag: str) -> bool:
    out = sh(["git", "ls-remote", "--tags", "origin", f"refs/tags/{tag}"], root)
    return bool(out.strip())


def image_names(root: Path, plugin: str, ref: str = f"origin/{BRANCH}") -> list[str]:
    try:
        listing = sh(["git", "ls-tree", "-r", "--name-only", ref, "--", f"{plugin}/images"], root)
    except Stop:
        return []
    return sorted({p.split("/")[2] for p in listing.splitlines() if p.endswith("/Dockerfile")})


def fetch_tag(root: Path, tag: str) -> None:
    sh(["git", "fetch", "--force", "origin", f"refs/tags/{tag}:refs/tags/{tag}"], root)


def check_publish_workflow(root: Path) -> None:
    """脚本靠 tag 触发 publish、靠它的 concurrency 串行；工作流改了先停下。"""
    text = show(root, f"origin/{BRANCH}", ".github/workflows/publish.yml")
    problems = []
    if not re.search(r"^    tags: \[?['\"]\*/v\*['\"]\]?", text, re.M):
        problems.append("push.tags 不是 */v*")
    if not re.search(r"^  group: publish$", text, re.M):
        problems.append("concurrency.group 不是 publish")
    if not re.search(r"^  cancel-in-progress: false$", text, re.M):
        problems.append("concurrency.cancel-in-progress 不是 false")
    if problems:
        raise Stop("publish.yml 的发布假设变了：" + "；".join(problems))


def merged_pr(commit: str) -> dict:
    pulls = gh_json(f"repos/{REPO}/commits/{commit}/pulls")
    for pull in pulls:
        if pull.get("merged_at") and pull.get("base", {}).get("ref") == BRANCH:
            return pull
    raise Stop(f"main 上的 {commit[:7]} 找不到已合的 PR（版本改动可能没合，或者提交是直接推的）")


def checks_green(plugin: str, commit: str) -> None:
    runs = gh_json(f"repos/{REPO}/commits/{commit}/check-runs?per_page=100")["check_runs"]
    mine = [r for r in runs if r["name"] in (f"插件 {plugin}", f"冒烟 {plugin}")]
    if not mine:
        raise Stop(f"main 上的 {commit[:7]} 还没跑 {plugin} 的检查")
    running = [r["name"] for r in mine if r.get("status") != "completed"]
    if running:
        raise Stop(f"main 上的 {commit[:7]} 检查还在跑：" + "、".join(running))
    bad = [f'{r["name"]}={r.get("conclusion")}' for r in mine if r.get("conclusion") not in GOOD]
    if bad:
        raise Stop(f"main 上的 {commit[:7]} 检查没绿：" + "、".join(bad))
    summary = [r for r in runs if r["name"] == "check 汇总"]
    for run in summary:
        if run.get("status") != "completed":
            raise Stop(f"main 上的 {commit[:7]} 汇总检查还在跑")
        if run.get("conclusion") not in GOOD:
            raise Stop(f"main 上的 {commit[:7]} 汇总检查没绿：{run.get('conclusion')}")


def verify_tag_content(root: Path, plan: Plan, commit: str) -> None:
    """tag 指向的清单要和版本一致；有自建镜像的插件还要求每个镜像都按摘要固定。"""
    compose = show(root, commit, f"{plan.plugin}/docker-compose.yml")
    version = manifest_version(compose)
    if version != plan.version:
        raise Stop(f"tag 指向的清单版本是 {version}，不是 {plan.version}")
    for name in plan.images:
        pattern = re.compile(r"^\s*image:\s*(?:&\S+\s+)?\S+/" + re.escape(name) + r"(?::[^@\s]+)?(?:@sha256:[0-9a-f]{64})?(\s*(?:#.*)?)$")
        lines = [line for line in compose.splitlines() if pattern.match(line)]
        if not lines:
            raise Stop(f"tag 指向的清单没有引用 images/{name} 的镜像")
        unpinned = [line.strip() for line in lines if "@sha256:" not in line]
        if unpinned:
            raise Stop(f"镜像摘要没写回 {plan.plugin}/docker-compose.yml：" + "；".join(unpinned))


def plan_plugin(root: Path, plugin: str) -> Plan:
    base = f"origin/{BRANCH}"
    try:
        text = show(root, base, f"{plugin}/docker-compose.yml")
    except Stop:
        raise Stop(f"main 上没有插件目录 {plugin}/") from None
    version = manifest_version(text)
    tag = f"{plugin}/v{version}"
    commit = sh(["git", "rev-list", "-n1", base, "--", plugin], root).strip()
    if not commit:
        raise Stop(f"main 上没有改过 {plugin} 的提交")
    plan = Plan(plugin, version, tag, commit, image_names(root, plugin))

    remote = remote_tag(root, tag)
    if remote:
        fetch_tag(root, tag)
        verify_tag_content(root, plan, tag)
        plan.released = True
        return plan

    local = f"refs/tags/{tag}"
    if subprocess.run(["git", "rev-parse", "-q", "--verify", local], cwd=root,
                      capture_output=True).returncode == 0:
        sh(["git", "tag", "-d", tag], root)
    last = sh(["git", "tag", "-l", f"{plugin}/v*", "--sort=-v:refname"], root).split("\n")
    last = [t for t in last if t and t != tag]
    if last and natural(version) <= natural(last[0].rsplit("/v", 1)[1]):
        raise Stop(f"{plugin} 的版本 {version} 不比上一个 tag {last[0]} 新：先把版本改上去再合 PR")

    return plan


def plan_releases(runner: Runner, plugins: list[str]) -> list[Plan]:
    root = runner.root
    if sh(["git", "status", "--porcelain"], root).strip():
        raise Stop("工作区有没提交的改动：先提交或收起来")
    with runner.doing("预检"):
        sh(["git", "fetch", "--tags", "origin", BRANCH], root)
        check_publish_workflow(root)
        runner.base = sh(["git", "rev-parse", f"origin/{BRANCH}"], root).strip()
        print(f"main = {runner.base}")
        plans = []
        for plugin in plugins:
            plan = plan_plugin(root, plugin)
            if not plan.released:
                pull = merged_pr(plan.commit)
                checks_green(plugin, plan.commit)
                print(f"  {plan.tag}：PR #{pull['number']} 已合，{plan.commit[:7]} 的检查全绿")
            else:
                print(f"  {plan.tag}：已经发过，跳过打 tag，只更新商店")
            plans.append(plan)
    return plans


def wait_idle(runner: Runner) -> None:
    deadline = time.monotonic() + runner.idle_timeout
    while True:
        runs = gh_json(f"{PUBLISH}?per_page=50&event=push")["workflow_runs"]
        busy = [r for r in runs if r.get("status") in ("queued", "in_progress", "waiting", "requested", "pending")]
        if not busy:
            return
        if time.monotonic() > deadline:
            raise Stop("发布并发组一直有运行，等了 %d 秒" % runner.idle_timeout)
        print("  并发组还有运行：" + "、".join(r["head_branch"] for r in busy) + "，等它跑完")
        time.sleep(runner.poll)


def wait_run(runner: Runner, tag: str) -> dict:
    deadline = time.monotonic() + runner.timeout
    appear = time.monotonic() + 120
    while True:
        runs = gh_json(f"{PUBLISH}?per_page=50&event=push")["workflow_runs"]
        mine = [r for r in runs if r.get("head_branch") == tag]
        if mine:
            run = max(mine, key=lambda r: r["created_at"])
            if run.get("status") == "completed":
                return run
            print(f"  发布运行 {run['id']}：{run.get('status')}")
        elif time.monotonic() > appear:
            raise Stop(f"推了 tag {tag} 但发布工作流没起来")
        if time.monotonic() > deadline:
            raise Stop(f"发布运行超过 {int(runner.timeout)} 秒没结束，见 https://github.com/{REPO}/actions/workflows/publish.yml")
        time.sleep(runner.poll)


def release_one(runner: Runner, plan: Plan) -> None:
    if plan.released:
        return
    with runner.doing(f"{plan.tag}：确认 main 没动"):
        sh(["git", "fetch", "origin", f"{BRANCH}:refs/remotes/origin/{BRANCH}"], runner.root)
        moved = sh(["git", "rev-parse", f"origin/{BRANCH}"], runner.root).strip()
        if moved != runner.base:
            raise Stop(f"预检之后 main 动到 {moved[:7]}：重新跑一遍，免得商店收到过时的版本")
    with runner.doing(f"{plan.tag}：等并发组空闲"):
        wait_idle(runner)
    with runner.doing(f"{plan.tag}：推 tag 并等发布"):
        sh(["git", "tag", "-a", plan.tag, "-m", plan.tag, plan.commit], runner.root)
        sh(["git", "push", "origin", f"refs/tags/{plan.tag}"], runner.root, timeout=180)
        run = wait_run(runner, plan.tag)
        if run.get("conclusion") == "cancelled":
            raise Stop(f"发布运行被取消了（并发组里挤进了别的 tag）：{run.get('html_url')}。确认没人同时在发版后重跑")
        if run.get("conclusion") != "success":
            raise Stop(f"发布运行失败（{run.get('conclusion')}）：{run.get('html_url')}")
    with runner.doing(f"{plan.tag}：确认摘要写回"):
        fetch_tag(runner.root, plan.tag)
        commit = sh(["git", "rev-parse", f"{plan.tag}^{{commit}}"], runner.root).strip()
        if plan.images:
            subject = sh(["git", "log", "-1", "--format=%s", commit], runner.root).strip()
            if not subject.startswith("chore(release):"):
                raise Stop(f"发布成功了，但 tag 没移到按摘要固定的提交上：{commit[:7]} {subject}")
        elif commit != plan.commit:
            raise Stop(f"发布成功了，但 tag 移到了别的提交：{commit[:7]}（应留在 {plan.commit[:7]}）")
        verify_tag_content(runner.root, plan, plan.tag)
        print(f"  {plan.tag} → {commit[:7]}")


def line_plugin(line: str) -> str | None:
    """update.sh 列表一行里的插件 id：新写法看 tag 的 <子目录>/v，旧写法看 wonton-plugin-<id> 的仓库名。"""
    found = re.match(r"^\s*(\S+)@(\S+)$", line)
    if not found:
        return None
    path, tag = found.groups()
    if "/v" in tag:
        return tag.rsplit("/v", 1)[0].rsplit("/", 1)[-1]
    return path.rsplit("/", 1)[-1].removeprefix("wonton-plugin-")


def rewrite_store(update_sh: str, plan: Plan) -> tuple[str, bool]:
    """把插件那一行换成新版本；没有这一行就加进列表末尾。返回文本和是不是换掉了别处的路径。"""
    lines = update_sh.split("\n")
    wanted = f"  wontonapp/wonton-plugins/{plan.plugin}@{plan.tag}"
    hits = [index for index, line in enumerate(lines) if line_plugin(line) == plan.plugin]
    if not hits:
        start = next(index for index, line in enumerate(lines) if line.strip() == "plugins=(")
        end = next(index for index in range(start + 1, len(lines)) if lines[index].strip() == ")")
        lines.insert(end, wanted)
        return "\n".join(lines), False
    for index in hits:
        listed = lines[index].strip().rsplit("@", 1)[1].rsplit("/v", 1)[-1].removeprefix("v")
        if natural(plan.version) < natural(listed):
            raise Stop(f"商店里 {plan.plugin} 已经是 {listed}，比要发的 {plan.version} 新")
    removed = any(
        lines[index].strip().split("@", 1)[0] != f"wontonapp/wonton-plugins/{plan.plugin}" for index in hits
    )
    for index in hits:
        lines[index] = wanted
    return "\n".join(lines), removed


def store_step(runner: Runner, plans: list[Plan]) -> str:
    store = runner.store
    with runner.doing("商店：准备分支"):
        if not (store / ".git").exists():
            raise Stop(f"{store} 不是 git 仓库")
        if sh(["git", "status", "--porcelain"], store).strip():
            raise Stop(f"商店工作区有没提交的改动：{store}")
        sh(["git", "fetch", "origin", BRANCH], store)
    if len(plans) == 1:
        branch = f"chore/release-{plans[0].plugin}-v{plans[0].version}"
    else:
        branch = f"chore/release-{len(plans)}-plugins"
    with runner.doing(f"商店：改 update.sh（分支 {branch}）"):
        sh(["git", "checkout", "-B", branch, f"origin/{BRANCH}"], store)
        text = (store / "update.sh").read_text(encoding="utf-8")
        removed = False
        for plan in plans:
            text, gone = rewrite_store(text, plan)
            removed = removed or gone
        (store / "update.sh").write_text(text, encoding="utf-8")
    with runner.doing("商店：跑 update.sh 重新生成 index.json"):
        sh(["./update.sh"], store, timeout=max(runner.timeout, 3600))
        if not sh(["git", "status", "--porcelain"], store).strip():
            print("  商店已经是这些版本，没有改动")
            return f"https://github.com/{STORE}"
        if not sh(["git", "status", "--porcelain", "index.json"], store).strip():
            raise Stop("update.sh 跑完了，index.json 没变化")
    with runner.doing("商店：提交、推送、开 PR"):
        sh(["git", "add", "update.sh", "index.json"], store)
        listing = "\n".join(f"- {plan.plugin} v{plan.version}（{plan.tag}）" for plan in plans)
        sh(["git", "commit", "-q", "-m", f"chore(index): 收录发版 {'、'.join(p.plugin for p in plans)}"], store)
        head = sh(["git", "rev-parse", "HEAD"], store).strip()
        sh(["git", "push", "--force", "-u", "origin", f"HEAD:refs/heads/{branch}"], store, timeout=180)
        body = (
            f"{listing}\n\n"
            "版本已在插件仓库打完 tag 并发布成功，把商店条目更新到这些 tag；index.json 由 ./update.sh 重新生成。"
        )
        title = f"chore(index): 收录 {plans[0].plugin} v{plans[0].version}" + (f" 等 {len(plans)} 个插件" if len(plans) > 1 else "")
        existing = json.loads(gh("pr", "list", "-R", STORE, "--head", branch, "--state", "open", "--json", "number"))
        if existing:
            number = existing[0]["number"]
            gh("pr", "edit", str(number), "-R", STORE, "--title", title, "--body", body)
            print(f"  接着用已有的 PR #{number}")
        else:
            number = int(gh("pr", "create", "-R", STORE, "--base", BRANCH, "--head", branch,
                            "--title", title, "--body", body, "--json", "number", "--jq", ".number").strip())
            print(f"  PR #{number}")
        if removed:
            gh("pr", "edit", str(number), "-R", STORE, "--add-label", "remove-plugin")
            print("  换了别处的路径，加了 remove-plugin 标签")
        pr_head = head
    url = f"https://github.com/{STORE}/pull/{number}"
    with runner.doing(f"商店：等检查绿（{url}）"):
        deadline = time.monotonic() + runner.timeout
        appear = time.monotonic() + 120
        while True:
            data = json.loads(gh("pr", "view", str(number), "-R", STORE, "--json", "statusCheckRollup,headRefOid"))
            pr_head = data["headRefOid"]
            rolls = data.get("statusCheckRollup") or []
            running = [r.get("name") for r in rolls if r.get("status") != "COMPLETED"]
            if not rolls:
                if time.monotonic() > appear:
                    raise Stop(f"商店 PR 起来 120 秒还没有检查：{url}")
            elif running:
                print("  还在跑：" + "、".join(str(name) for name in running if name))
            else:
                bad = [f'{r.get("name")}={r.get("conclusion")}' for r in rolls
                       if r.get("conclusion") not in GOOD and r.get("conclusion") not in ("NEUTRAL", "SKIPPED", "SUCCESS")]
                if bad:
                    raise Stop(f"商店检查没绿：{'、'.join(bad)}；PR 留着：{url}")
                break
            if time.monotonic() > deadline:
                raise Stop(f"商店检查超过 {int(runner.timeout)} 秒没全绿；PR 留着：{url}")
            time.sleep(runner.poll)
    with runner.doing(f"商店：合并 {url}"):
        gh("pr", "merge", str(number), "-R", STORE, "--squash", "--match-head-commit", pr_head, "--delete-branch", timeout=180)
    return url


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("plugins", nargs="+", help="要发版的插件 id（顶层目录名）")
    parser.add_argument("--store", default=os.environ.get("WONTON_STORE"),
                        help="wonton-store 的检出目录，也可以设 WONTON_STORE")
    parser.add_argument("--dry-run", action="store_true", help="只打印计划：不推远端、不改商店（仍会 git fetch --tags，并可能删掉本地遗留的同名 tag）")
    parser.add_argument("--upgrade-steps", action="store_true", help="最后打印升级测试机的步骤")
    parser.add_argument("--poll", type=float, default=20, help="轮询间隔秒数")
    parser.add_argument("--timeout", type=float, default=3600, help="单步等待秒数（发布、商店检查）")
    parser.add_argument("--idle-timeout", type=float, default=1800, help="等发布并发组空闲的秒数")
    args = parser.parse_args(argv)
    args.plugins = list(dict.fromkeys(args.plugins))
    if not args.store:
        parser.error("用 --store 或 WONTON_STORE 指定 wonton-store 的检出目录")

    runner = Runner(Path(__file__).resolve().parent.parent, Path(args.store).resolve(),
                    args.poll, args.timeout, args.idle_timeout)
    try:
        plans = plan_releases(runner, args.plugins)
        todo = [plan for plan in plans if not plan.released]
        print("\n计划：")
        for plan in todo:
            print(f"  打并推 {plan.tag}（{plan.commit[:7]}），等发布跑完并确认摘要写回")
        print(f"  在商店把 {len(plans)} 个插件的版本改成上面的 tag，跑 update.sh，开 PR，检查绿后合并")
        if args.dry_run:
            print("\n--dry-run：没推远端，也没改商店")
            if args.upgrade_steps:
                print("\n" + UPGRADE_STEPS)
            return 0
        for plan in todo:
            release_one(runner, plan)
        url = store_step(runner, plans)
        print(f"\n发完了：{url}")
        if args.upgrade_steps:
            print("\n" + UPGRADE_STEPS)
        return 0
    except Stop as error:
        print(f"\n停止（{runner.step}）：{error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
