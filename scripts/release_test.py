#!/usr/bin/env python3
"""scripts/release.py 的行为测试：PATH 前面放假的 gh，GitHub 侧用本地状态模拟，仓库是本地裸库，不连网、不发真请求。

用法：python3 scripts/release_test.py [--keep]
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_DIR = Path(__file__).resolve().parent.parent
BASE = Path(tempfile.mkdtemp(prefix="release-test."))
PASSED = 0
FAILED = []

COMPOSE = """\
services:
  app:
    image: {image}
x-wonton:
  id: {plugin}
  name: {plugin}
  version: {version}
  release_notes: 修了点东西
"""

PUBLISH_YML = """\
on:
  push:
    tags: ['*/v*']
concurrency:
  group: publish
  cancel-in-progress: false
jobs: {{}}
"""

STORE_UPDATE = """\
#!/usr/bin/env bash
set -euo pipefail
plugins=(
  wontonapp/wonton-plugins/alpha@alpha/v0.1.0
  geraldpeng6/wonton-plugin-beta@v0.2.0
  geraldpeng6/wonton-plugin-files@v0.2.3
)
python3 - <<'PY'
import json, re
text = open("update.sh", encoding="utf-8").read()
block = re.search(r"^plugins=\\(\\n(.*?)^\\)", text, re.S | re.M)
rows = [line.strip() for line in block.group(1).splitlines() if line.strip()]
plugins = [{"source": row.rsplit("@", 1)[0], "tag": row.rsplit("@", 1)[1]} for row in rows]
open("index.json", "w", encoding="utf-8").write(json.dumps({"plugins": plugins}, indent=2) + "\\n")
PY
"""

FAKE_GH = '''\
#!/usr/bin/env python3
import os
import sys

sys.path.insert(0, os.environ["FAKE_GH_LIB"])
import fake_gh

sys.exit(fake_gh.main(sys.argv[1:]))
'''


def git(*args: str, cwd: Path) -> str:
    return subprocess.run(["git", *args], cwd=cwd, text=True, capture_output=True, check=True).stdout


class Fixture:
    """一个插件仓库、一个商店仓库和一台假 GitHub。场景写在 scenario.json 里。"""

    def __init__(self, name: str):
        self.dir = BASE / name
        self.dir.mkdir()
        self.state = self.dir / "state"
        self.state.mkdir()
        self.scenario = {
            "checks": "green",
            "pr_merged": True,
            "busy_polls": 0,
            "publish": {"delay": 2, "conclusion": "success", "digest": True},
            "store_checks": {"green_after": 2},
            "move_main": False,
        }
        self.origin = self.dir / "origin.git"
        self.store_origin = self.dir / "store-origin.git"
        self.work = self.dir / "work"
        self.store = self.dir / "store"
        git("init", "-q", "--bare", "-b", "main", str(self.origin), cwd=self.dir)
        git("init", "-q", "--bare", "-b", "main", str(self.store_origin), cwd=self.dir)
        self._seed()
        self._timer = 0

    def save(self) -> None:
        (self.dir / "scenario.json").write_text(json.dumps(self.scenario), encoding="utf-8")

    def env(self) -> dict:
        env = dict(os.environ)
        env.update(
            PATH=f"{self.dir / 'bin'}:/usr/bin:/bin",
            FAKE_GH_LIB=str(self.dir),
            FAKE_STATE=str(self.state),
            FAKE_SCENARIO=str(self.dir / "scenario.json"),
            FAKE_ORIGIN=str(self.origin),
            FAKE_STORE_ORIGIN=str(self.store_origin),
            WONTON_STORE=str(self.store),
            GIT_AUTHOR_NAME="测试",
            GIT_AUTHOR_EMAIL="test@example.com",
            GIT_COMMITTER_NAME="测试",
            GIT_COMMITTER_EMAIL="test@example.com",
        )
        return env

    def now(self) -> str:
        self._timer += 1
        return f"2026-11-20T12:00:{self._timer:02d}Z"

    def commit(self, message: str, cwd: Path | None = None) -> None:
        where = cwd or self.work
        git("add", "-A", cwd=where)
        git("commit", "-q", "-m", message, cwd=where)

    def _write_compose(self, plugin: str, version: str, image: str | None = None, images: list[str] | None = None) -> None:
        target = self.work / plugin
        target.mkdir(parents=True, exist_ok=True)
        repo = f"ghcr.io/wontonapp/wonton-plugins/{plugin}"
        image = image or f"docker.io/library/nginx:1.27@sha256:{'1' * 64}"
        (target / "docker-compose.yml").write_text(COMPOSE.format(plugin=plugin, version=version, image=image), encoding="utf-8")
        (target / "README.md").write_text(f"# {plugin}\n", encoding="utf-8")
        for name in images or []:
            path = target / "images" / name
            path.mkdir(parents=True, exist_ok=True)
            (path / "Dockerfile").write_text("FROM scratch\n", encoding="utf-8")
            compose = target / "docker-compose.yml"
            text = compose.read_text(encoding="utf-8").replace(
                "  app:\n    image: docker.io/library/nginx:1.27@" + "sha256:" + "1" * 64 + "\n",
                f"  app:\n    image: {repo}/{name}:{version}\n",
            )
            compose.write_text(text, encoding="utf-8")

    def _seed(self) -> None:
        git("clone", "-q", str(self.origin), str(self.work), cwd=self.dir)
        (self.work / "scripts").mkdir()
        shutil.copy(REPO_DIR / "scripts" / "release.py", self.work / "scripts" / "release.py")
        (self.work / ".github" / "workflows").mkdir(parents=True)
        (self.work / ".github" / "workflows" / "publish.yml").write_text(PUBLISH_YML, encoding="utf-8")
        for plugin in ("alpha", "beta", "files", "gamma"):
            self._write_compose(plugin, "0.1.0", images=["web"] if plugin in ("beta", "files") else None)
        self.commit("chore: 初始")
        for plugin, version in (("alpha", "0.2.0"), ("beta", "0.3.0"), ("files", "0.2.4")):
            self._write_compose(plugin, version, images=["web"] if plugin in ("beta", "files") else None)
            self.commit(f"feat({plugin}): 升到 {version}")
        self._write_compose("gamma", "0.1.0")
        git("tag", "-a", "gamma/v0.2.0", "-m", "gamma/v0.2.0", cwd=self.work)
        git("push", "-q", "origin", "main", "refs/tags/gamma/v0.2.0", cwd=self.work)
        # files 已经发过：tag 指向带摘要的提交。
        git("checkout", "-q", "-b", "digest", cwd=self.work)
        compose = self.work / "files" / "docker-compose.yml"
        compose.write_text(re.sub(r"(:0\.2\.4)$", ":0.2.4@sha256:" + "a" * 64, compose.read_text(encoding="utf-8"), flags=re.M), encoding="utf-8")
        self.commit("chore(release): files/v0.2.4 的镜像按摘要固定（web）")
        git("tag", "-a", "files/v0.2.4", "-m", "files/v0.2.4", cwd=self.work)
        git("push", "-q", "origin", "refs/tags/files/v0.2.4", cwd=self.work)
        git("checkout", "-q", "main", cwd=self.work)

        git("clone", "-q", str(self.store_origin), str(self.store), cwd=self.dir)
        (self.store / "update.sh").write_text(STORE_UPDATE, encoding="utf-8")
        (self.store / "update.sh").chmod(0o755)
        (self.store / "index.json").write_text('{"plugins": []}\n', encoding="utf-8")
        self.commit("chore: 初始", cwd=self.store)
        git("push", "-q", "origin", "main", cwd=self.store)

        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        (bin_dir / "gh").write_text(FAKE_GH, encoding="utf-8")
        (bin_dir / "gh").chmod(0o755)
        (self.dir / "fake_gh.py").write_text(SIMULATOR, encoding="utf-8")
        self.save()

    def run(self, *args: str) -> subprocess.CompletedProcess:
        self.save()
        return subprocess.run(
            [sys.executable, str(self.work / "scripts" / "release.py"), *args,
             "--poll", "0.01", "--timeout", "20", "--idle-timeout", "5"],
            cwd=self.work, env=self.env(), text=True, capture_output=True, timeout=180,
        )

    def calls(self) -> list[str]:
        log = self.state / "calls.log"
        return log.read_text(encoding="utf-8").splitlines() if log.exists() else []

    def tags(self) -> list[str]:
        return sorted(git("tag", "-l", cwd=self.origin).split())

    def store_main(self) -> str:
        return git("rev-parse", "main", cwd=self.store_origin).strip()


SIMULATOR = r'''
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

STATE = Path(os.environ["FAKE_STATE"])
SCENARIO = json.loads(Path(os.environ["FAKE_SCENARIO"]).read_text(encoding="utf-8"))
ORIGIN = Path(os.environ["FAKE_ORIGIN"])
STORE_ORIGIN = Path(os.environ["FAKE_STORE_ORIGIN"])
RUNS = STATE / "runs.json"


def log(line):
    with open(STATE / "calls.log", "a", encoding="utf-8") as out:
        out.write(line + "\n")


def git(*args, cwd=None):
    return subprocess.run(["git", *args], cwd=cwd, text=True, capture_output=True, check=True).stdout


def runs():
    return json.loads(RUNS.read_text(encoding="utf-8")) if RUNS.exists() else []


def save_runs(items):
    RUNS.write_text(json.dumps(items), encoding="utf-8")


def plugin_of(sha):
    for plugin in ("alpha", "beta", "files", "gamma"):
        if sha in git("--git-dir", str(ORIGIN), "rev-list", "main", "--", plugin).split():
            return plugin
    return None


SEED_TAGS = {"files/v0.2.4", "gamma/v0.2.0"}


def new_tags(items):
    known = {run["tag"] for run in items} | SEED_TAGS
    return [tag for tag in git("--git-dir", str(ORIGIN), "tag", "-l").split() if tag not in known]


def move_main():
    work = STATE / "mover"
    git("clone", "-q", str(ORIGIN), str(work))
    (work / "alpha" / "README.md").write_text("# alpha\n\n后加的提交\n", encoding="utf-8")
    git("-c", "user.name=bot", "-c", "user.email=bot@example.com", "commit", "-qam", "docs(alpha): 后加的提交", cwd=work)
    git("push", "-q", "origin", "HEAD:refs/heads/main", cwd=work)


def publish_runs():
    items = runs()
    busy = SCENARIO["busy_polls"]
    if busy:
        for tag in new_tags(items):
            log(f"VIOLATION pushed-while-busy {tag}")
        live = [r for r in items if r["status"] != "completed"]
        SCENARIO["busy_polls"] = busy - 1
        Path(os.environ["FAKE_SCENARIO"]).write_text(json.dumps(SCENARIO), encoding="utf-8")
        live.append({"id": 100, "tag": "__busy__", "head_branch": "other/v9.9.9",
                     "status": "in_progress", "conclusion": None, "created_at": "2026-11-20T11:00:00Z",
                     "html_url": "https://example/100"})
        return live
    for tag in new_tags(items):
        items.append({"id": len(items) + 1, "tag": tag, "head_branch": tag, "status": "queued",
                      "conclusion": None, "polls": 0,
                      "created_at": f"2026-11-20T12:{len(items):02d}:00Z",
                      "html_url": f"https://example/{len(items) + 1}"})
    save_runs(items)
    for run in items:
        if run["status"] == "completed":
            continue
        run["polls"] += 1
        if run["polls"] >= SCENARIO["publish"]["delay"]:
            run["status"] = "completed"
            run["conclusion"] = SCENARIO["publish"]["conclusion"]
            if run["conclusion"] == "success" and SCENARIO["publish"]["digest"]:
                write_digest(run["tag"])
    save_runs(items)
    return items


def write_digest(tag):
    plugin = tag.split("/v")[0].split("/")[0]
    work = STATE / f"digest-{abs(hash(tag))}"
    shutil.rmtree(work, ignore_errors=True)
    git("clone", "-q", str(ORIGIN), str(work))
    git("checkout", "-q", tag, cwd=work)
    if not (work / plugin / "images").exists():
        return
    compose = work / plugin / "docker-compose.yml"
    text = compose.read_text(encoding="utf-8")
    text = re.sub(r"(:[0-9][^:@\s]*)$", r"\1@sha256:" + "a" * 64, text, flags=re.M)
    compose.write_text(text, encoding="utf-8")
    git("add", "-A", cwd=work)
    git("-c", "user.name=bot", "-c", "user.email=bot@example.com",
        "commit", "-q", "-m", f"chore(release): {tag} 的镜像按摘要固定（web）", cwd=work)
    git("push", "-q", "--force", "origin", f"HEAD:refs/tags/{tag}", cwd=work)


def check_runs(sha):
    plugin = plugin_of(sha)
    rolls = []
    if plugin:
        mode = SCENARIO["checks"] if isinstance(SCENARIO["checks"], str) else SCENARIO["checks"].get(plugin, "green")
        if mode != "absent":
            rolls.append({"name": f"插件 {plugin}",
                          "status": "completed" if mode in ("green", "failed") else "in_progress",
                          "conclusion": "success" if mode == "green" else ("failure" if mode == "failed" else None)})
    summary = SCENARIO.get("summary", "green")
    if summary != "absent":
        rolls.append({"name": "check 汇总", "status": "completed", "conclusion": "success" if summary == "green" else "failure"})
    return {"check_runs": rolls}


def api(path):
    path = path.split("?")[0]
    found = re.search(r"/commits/([0-9a-f]{40})/pulls$", path)
    if found:
        sha = found.group(1)
        if not SCENARIO["pr_merged"]:
            return []
        if SCENARIO["move_main"] and not (STATE / "moved").exists():
            (STATE / "moved").write_text("1", encoding="utf-8")
            move_main()
        return [{"number": 7, "merged_at": "2026-11-20T10:00:00Z", "base": {"ref": "main"}}]
    found = re.search(r"/commits/([0-9a-f]{40})/check-runs$", path)
    if found:
        return check_runs(found.group(1))
    if path.endswith("/actions/workflows/publish.yml/runs"):
        items = publish_runs()
        return {"workflow_runs": [
            {"id": r["id"], "head_branch": r["head_branch"], "status": r["status"],
             "conclusion": r["conclusion"], "created_at": r["created_at"], "html_url": r["html_url"]}
            for r in items
        ]}
    raise SystemExit(f"假 gh 不认识的接口：{path}")


def pr_view(number):
    data = json.loads((STATE / "pr.json").read_text(encoding="utf-8"))
    data["views"] += 1
    (STATE / "pr.json").write_text(json.dumps(data), encoding="utf-8")
    if data["views"] < SCENARIO["store_checks"]["green_after"]:
        rolls = [{"name": "重新生成并对比", "status": "IN_PROGRESS", "conclusion": None}]
    else:
        verdict = SCENARIO["store_checks"].get("conclusion", "success")
        rolls = [{"name": "重新生成并对比", "status": "COMPLETED",
                  "conclusion": "SUCCESS" if verdict == "success" else "FAILURE"}]
    return {"statusCheckRollup": rolls, "headRefOid": data["head"]}


def main(argv):
    log(("WRITE " if write(argv) else "READ  ") + " ".join(argv))
    args = [a for a in argv]
    repo = None
    if "-R" in args:
        index = args.index("-R")
        repo = args[index + 1]
        del args[index:index + 2]
    if args[0] == "api":
        print(json.dumps(api(args[1])))
        return 0
    if args[:2] == ["pr", "list"]:
        head = args[args.index("--head") + 1]
        existing = []
        if (STATE / "pr.json").exists():
            data = json.loads((STATE / "pr.json").read_text(encoding="utf-8"))
            if data["branch"] == head:
                existing.append({"number": data["number"]})
        print(json.dumps(existing))
        return 0
    if args[:2] == ["pr", "create"]:
        branch = args[args.index("--head") + 1]
        head = git("--git-dir", str(STORE_ORIGIN), "rev-parse", f"refs/heads/{branch}").strip()
        (STATE / "pr.json").write_text(json.dumps({"number": 1, "branch": branch, "head": head, "views": 0}), encoding="utf-8")
        print("1")
        return 0
    if args[:2] == ["pr", "view"]:
        print(json.dumps(pr_view(args[2])))
        return 0
    if args[:2] == ["pr", "edit"]:
        if "--add-label" in args:
            (STATE / "labels.json").write_text(json.dumps([args[args.index("--add-label") + 1]]), encoding="utf-8")
        return 0
    if args[:2] == ["pr", "merge"]:
        data = json.loads((STATE / "pr.json").read_text(encoding="utf-8"))
        git("--git-dir", str(STORE_ORIGIN), "update-ref", "refs/heads/main", data["head"])
        git("--git-dir", str(STORE_ORIGIN), "update-ref", "-d", f"refs/heads/{data['branch']}")
        (STATE / "merged").write_text("1", encoding="utf-8")
        return 0
    raise SystemExit(f"假 gh 不认识的命令：{args}")


def write(argv):
    if argv[0] == "pr":
        return len(argv) > 1 and argv[1] in ("create", "edit", "merge", "close")
    return "--method" in argv or any(a in ("-X", "--method") for a in argv)


sys.exit(main(sys.argv[1:]))
'''


def check(condition, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def expect_stop(result: subprocess.CompletedProcess, text: str) -> None:
    check(result.returncode != 0, f"应该失败，实际 0：{result.stdout}")
    check(text in result.stderr, f"报错里应该提到 {text!r}：{result.stderr}")


def waits_for_running_publish() -> None:
    fixture = Fixture("busy")
    fixture.scenario["busy_polls"] = 3
    result = fixture.run("alpha")
    check(result.returncode == 0, f"应该成功：{result.stderr}")
    check(not [line for line in fixture.calls() if line.startswith("VIOLATION")],
          f"并发组有运行时就推了 tag：{[l for l in fixture.calls() if l.startswith('VIOLATION')]}")
    check("并发组还有运行" in result.stdout, "应该打印在等并发组")
    check("alpha/v0.2.0" in fixture.tags(), "tag 应该推上去")


def publish_failure_stops_before_store() -> None:
    fixture = Fixture("publish-fail")
    fixture.scenario["publish"]["conclusion"] = "failure"
    before = fixture.store_main()
    result = fixture.run("alpha")
    expect_stop(result, "发布运行失败")
    check("alpha/v0.2.0" in fixture.tags(), "tag 已经推上去了，不删除")
    check(fixture.store_main() == before, "商店不该被改")
    check(not [line for line in fixture.calls() if "pr create" in line], "不该开商店 PR")


def red_checks_refuse() -> None:
    fixture = Fixture("red")
    fixture.scenario["checks"] = "failed"
    result = fixture.run("alpha")
    expect_stop(result, "检查没绿")
    check("alpha/v0.2.0" not in fixture.tags(), "不该推 tag")


def unmerged_pr_refuses() -> None:
    fixture = Fixture("unmerged")
    fixture.scenario["pr_merged"] = False
    result = fixture.run("alpha")
    expect_stop(result, "找不到已合的 PR")


def older_version_refuses() -> None:
    fixture = Fixture("older")
    result = fixture.run("gamma")
    expect_stop(result, "不比上一个 tag")


def missing_digest_stops() -> None:
    fixture = Fixture("no-digest")
    fixture.scenario["publish"]["digest"] = False
    before = fixture.store_main()
    result = fixture.run("beta")
    expect_stop(result, "摘要")
    check(fixture.store_main() == before, "商店不该被改")


def dry_run_writes_nothing() -> None:
    fixture = Fixture("dry")
    before = fixture.store_main()
    result = fixture.run("alpha", "beta", "--dry-run")
    check(result.returncode == 0, f"dry-run 应该成功：{result.stderr}")
    check("beta/v0.3.0" in result.stdout, "计划里应该列出要推的 tag")
    check(not [line for line in fixture.calls() if line.startswith("WRITE")], "dry-run 不该有写请求")
    check(fixture.tags() == ["files/v0.2.4", "gamma/v0.2.0"], f"不该推 tag：{fixture.tags()}")
    local = git("tag", "-l", cwd=fixture.work).split()
    check(sorted(local) == ["files/v0.2.4", "gamma/v0.2.0"], f"本地也不该建 tag：{local}")
    check(fixture.store_main() == before, "商店不该被改")


def full_flow_updates_store_and_merges() -> None:
    fixture = Fixture("flow")
    result = fixture.run("alpha", "beta", "files")
    check(result.returncode == 0, f"应该成功：{result.stderr}\n{result.stdout}")
    check("alpha/v0.2.0" in fixture.tags(), "alpha 的 tag")
    check("beta/v0.3.0" in fixture.tags(), "beta 的 tag")
    data = json.loads((fixture.state / "pr.json").read_text(encoding="utf-8"))
    check(data["views"] >= 2, "合并前应该等过检查")
    check((fixture.state / "merged").exists(), "应该合并")
    labels = json.loads((fixture.state / "labels.json").read_text(encoding="utf-8")) if (fixture.state / "labels.json").exists() else []
    check("remove-plugin" in labels, "换了旧仓库路径，要加 remove-plugin 标签")
    update = git("show", "main:update.sh", cwd=fixture.store_origin)
    check("wontonapp/wonton-plugins/alpha@alpha/v0.2.0" in update, "update.sh 里换成新 tag")
    check("geraldpeng6/wonton-plugin-files" not in update, "旧路径不该留着")
    index = json.loads(git("show", "main:index.json", cwd=fixture.store_origin))
    sources = {item["source"] for item in index["plugins"]}
    check("wontonapp/wonton-plugins/beta" in sources, "index.json 应该重新生成")
    check("升级测试机" not in result.stdout, "没加开关就不打印升级步骤")


def upgrade_steps_are_printed() -> None:
    fixture = Fixture("upgrade")
    result = fixture.run("alpha", "--upgrade-steps")
    check(result.returncode == 0, f"应该成功：{result.stderr}")
    check("refresh-records.py" in result.stdout, "应该打印改写记录的命令")
    check("docker network inspect" in result.stdout, "应该打印升级前的检查")


def already_released_only_updates_store() -> None:
    fixture = Fixture("already")
    result = fixture.run("files")
    check(result.returncode == 0, f"应该成功：{result.stderr}")
    check("已经发过" in result.stdout, "应该说跳过打 tag")
    check(len([tag for tag in fixture.tags() if tag.startswith("files/")]) == 1, "不该再推 files 的 tag")
    update = git("show", "main:update.sh", cwd=fixture.store_origin)
    check("files@files/v0.2.4" in update, "商店更新到发过的版本")


def store_check_failure_keeps_pr() -> None:
    fixture = Fixture("store-red")
    fixture.scenario["store_checks"] = {"green_after": 1, "conclusion": "failure"}
    result = fixture.run("files")
    expect_stop(result, "商店检查没绿")
    check("https://github.com/wontonapp/wonton-store/pull/1" in result.stderr, "报错要带 PR 地址")
    check(not (fixture.state / "merged").exists(), "检查红了不该合并")
    branches = git("--git-dir", str(fixture.store_origin), "for-each-ref", "refs/heads/", cwd=fixture.dir)
    check("chore/release-files-v0.2.4" in branches, "分支要留着，PR 还在")


def rerun_is_noop() -> None:
    fixture = Fixture("rerun")
    check(fixture.run("files").returncode == 0, "第一次应该成功")
    calls = len(fixture.calls())
    result = fixture.run("files")
    check(result.returncode == 0, f"重跑应该成功：{result.stderr}")
    check("没有改动" in result.stdout, "应该说商店已经是最新")
    check(not [line for line in fixture.calls()[calls:] if line.startswith("WRITE")], "重跑不该再写商店")


def main_moved_refuses() -> None:
    fixture = Fixture("moved")
    fixture.scenario["move_main"] = True
    before = fixture.tags()
    result = fixture.run("alpha")
    expect_stop(result, "main 动到")
    check(fixture.tags() == before, "main 动了就不该推 tag")


def unknown_plugin_refuses() -> None:
    fixture = Fixture("unknown")
    result = fixture.run("nosuch")
    expect_stop(result, "没有插件目录")


def main(argv: list[str]) -> int:
    global PASSED
    print(f"{len(CASES)} 个用例，临时目录 {BASE}")
    for function in CASES:
        try:
            function()
        except Exception as error:  # noqa
            FAILED.append((function.__name__, error))
            print(f"FAIL {function.__name__}: {error}")
        else:
            PASSED += 1
            print(f"ok   {function.__name__}")
    print(f"\n{PASSED} 通过，{len(FAILED)} 失败")
    if not argv or "--keep" not in argv:
        shutil.rmtree(BASE, ignore_errors=True)
    return 1 if FAILED else 0


CASES = [
    waits_for_running_publish,
    publish_failure_stops_before_store,
    red_checks_refuse,
    unmerged_pr_refuses,
    older_version_refuses,
    missing_digest_stops,
    dry_run_writes_nothing,
    full_flow_updates_store_and_merges,
    upgrade_steps_are_printed,
    already_released_only_updates_store,
    store_check_failure_keeps_pr,
    rerun_is_noop,
    main_moved_refuses,
    unknown_plugin_refuses,
]


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
