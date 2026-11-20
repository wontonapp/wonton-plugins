# wonton-plugins

Wonton 的官方插件仓库：一个仓库，多个插件，每个插件一个顶层 `<id>/` 子目录。开发套件、CI 和检查规则在
仓库根共用一份；每个子目录仍是一个独立插件包，插件 id、版本、许可证和商店条目都分开，核心把每个子目录
当一个插件。

```
<id>/                每个插件一个目录：docker-compose.yml（清单）、README、LICENSE、NOTICE、icon、网页素材
scripts/             开发套件，按 WONTON_PLUGIN=<id> 指定插件目录运行
.github/workflows/   检查（按改动路径分作业）、发布（推 tag 触发）
.github/changed.py   算一个 PR 改了哪些插件
.github/tag.py       校验发布 tag <id>/v<版本> 和清单版本一致
.leak-allow          公开仓库泄漏检查的豁免名单
```

## 新增一个插件

1. 建顶层目录 `<id>/`，放 `docker-compose.yml`（清单见[插件规范](https://github.com/wontonapp/wonton-docs/blob/main/plugin-spec.md)）、README、LICENSE、图标和插件引用的文件。镜像用上游官方镜像并按摘要固定。
2. `WONTON_PLUGIN=<id> scripts/check.sh` 通过，`WONTON_PLUGIN=<id> scripts/dev.sh install` 装得上。
3. 提 PR；CI 只检查改动的插件目录。合并后向 `wontonapp/wonton-store` 提 PR 收录。

## 脚本

```sh
export WONTON_PLUGIN=<id>
scripts/check.sh            # 检查插件包（--official 按官方索引的要求）
scripts/dev.sh up           # 起开发环境；install / reinstall / status / web / smoke 同见脚本头部
scripts/dev.sh smoke        # 端到端冒烟：装得上、网页打得开、身份能用、工具 Bot 调得到、升级保数据
```

### 需要显卡的插件

清单声明 `requirements.gpu.need: required`（或申请 `devices.gpu`）时，没有 GPU 的冒烟机不会安装插件，作业会显示“跳过：需要显卡，没跑”，退出码为 `77`。在已配置 Docker GPU runtime 的机器上手动运行：

```bash
WONTON_PLUGIN=<id> scripts/smoke-gpu.sh <id>
WONTON_PLUGIN=<id> scripts/dev.sh smoke
```

不要为此安装系统驱动，也不要修改宿主机服务；手动命令只使用当前 Docker target。

状态放在 `.wonton/<id>/`，几个插件可以同时各起各的。

## 检查

`check.yml` 先按改动的路径算出受影响的插件：改 `scripts/` 或工作流就是全部；只改 `<id>/` 只检查那一个。
每个插件一个作业跑 `plugin check`（非官方；官方要求由发布流程发版前的 `--official` 把关）和自带脚本的
ShellCheck。仓库级检查（源文件行数、泄漏检查、ADR 编号）每个 PR 都跑，和路径无关。Dependabot 的镜像
升级 PR 和手动触发会额外跑受影响插件的端到端冒烟。

## 发布

把子目录清单的 `version` 改成新版本、写好 `release_notes`，提交进 `main` 并合 PR。然后在仓库根目录、
有 `gh` 登录、旁边有 wonton-store 检出的地方跑：

```sh
scripts/release.py <id>… --store ../wonton-store
```

它先做预检（那个提交来自已合的 PR、插件检查和 `check 汇总` 全绿、版本比上一个 tag 新），再逐个推 tag、
等发布工作流成功并把镜像摘要写回，最后在商店改条目、开 PR、等检查绿了合并。任一步失败就停，报出哪一步和
原因；修好后重跑同一条命令，已发过的插件跳过、不碰它的 tag。

命令只发本仓库的插件。商店里还指向私有仓库（`geraldpeng6/wonton-plugin-<id>` 之类来源）的条目会顺带迁到
`wontonapp/wonton-plugins/<id>@<id>/v<版本>`，这类 PR 因此带 `remove-plugin` 标签。

`--dry-run` 只打印计划和预检结果：不推远端、不改商店，但会 `git fetch --tags`，并可能删掉本地遗留的同名
tag（远端没有它时）。`--upgrade-steps` 在最后打印升级测试机的步骤。预检和商店两段也可以手动做，推 tag 就是：

```sh
git tag -a <id>/v<版本> -m <id>/v<版本> && git push origin <id>/v<版本>
```

发布工作流校验 tag 和清单版本一致，按官方索引的要求检查插件包；有自建镜像（`<id>/images/`）的先构建
多架构镜像、推送到 ghcr.io、把摘要写回 compose，tag 移到摘要提交上。之后向 `wontonapp/wonton-store`
提 PR 把商店条目改成新版本；`scripts/release_test.py` 用本地裸库和假 `gh` 跑这些行为，不连网。

## Dependabot

`docker-compose` 生态按 `/*` 通配覆盖所有插件目录，`group-by: dependency-name` 把同一个上游镜像在多个
插件里的升级合成一个 PR；PR 只跑改动插件的检查和冒烟。

## 许可证

本仓库是 MIT（见 [LICENSE](LICENSE)）。各插件子目录里上游程序和图标的许可证写在各自的 LICENSE / NOTICE。
