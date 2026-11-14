# 家庭表格（Grist）

[English](#english)

像 Excel，又像个不用学就能用的小数据库。家里的资产、报销、课程表、藏书清单都能记进表格里：
表格之间可以互相引用，汇总自动算，还能一键生成图表。在手机或电脑的浏览器里打开就能用，
全家人各用各的账号登录，谁看得到、谁能改哪部分都可以按行、列设置规则。

装好即用，不需要再做设置：主人第一次打开就是管理员，家里其他人打开后会各自建号。

## 怎么用

- 打开插件网页，新建一张空白文档，或从首页的模板挑一张开始。
- 想让家人一起用：在文档右上角“共享”，把对方的邮箱加进去（家里人登录后的账号就是各自的内部邮箱）。

## 数据存在哪、怎么备份

- 所有数据都在插件的数据目录里：`home.sqlite3` 是账号和站点信息，`docs/` 里每张表格一个 `.grist` 文件
  （就是 SQLite 数据库，复制走就能用别的工具打开），`grist-sessions.db` 是登录会话。
- 备份时建议先在插件详情页停止插件，再整个拷走数据目录。文件属主是容器内的 `grist` 用户
  （uid/gid 1001），普通用户读不了时用 root 或 sudo 拷贝。
- Grist 自带文档级快照（文档的修改历史，在文档的“所有版本”里看），但快照存在同一个数据目录里，
  不能替代上面的整目录备份。

## 已知限制

- **公式没有沙箱**：Grist 的公式是 Python，上游默认放在隔离沙箱里运行，但那需要额外的高权限；
  本插件不申请高权限，公式以容器内的用户直接运行。日常家用没有影响，但**不要导入网上陌生人的
  表格文件**：里面的公式可能读到插件容器内的数据。
- **不出网**：插件没有申请访问外网，所以公式助手（AI 补公式）、在线模板库、自定义组件库这些
  联网功能用不了（打开会报错或列表为空），表格本身不受影响。
- 一个站点。家是一个团队，一般够用；每登录一个人还会自动有一个个人空间。
- **界面默认英文**：Grist 支持多语言，但实测 1.7.22 的官方镜像（`gristlabs/grist` 和 `grist-oss` 都一样）不提供界面加载
  翻译用的文件（请求 404），把账号资料的语言设成中文后界面仍是英文。翻译文件在镜像里齐全，等上游修复后不需要改插件。

## 登录说明

登录由云吞统一管理：打开网页走云吞的登录，进到 Grist 就是你的账号（账号按内部邮箱认）。
在 Grist 里点“退出登录”只退出 Grist 自己的会话，再刷新页面会自动登回来；要真正退出，
退出云吞的登录。

## 权限

- `owner.identity`：Grist 首次启动时按主人的内部邮箱预建管理员账号，需要知道主人的邮箱。
  管理员是谁由这个邮箱决定，装好后不要手动改。

## 许可证和商标

- Grist 社区版（本插件用的镜像 `gristlabs/grist-oss`）以 [Apache-2.0](https://github.com/gristlabs/grist-core)
  发布；插件包本身是 MIT。
- Grist 和 Grist 图标是 Grist Labs 的商标；图标取自上游仓库，见 [NOTICE](NOTICE)。

## English

A spreadsheet crossed with a little database: track household assets, expenses, timetables and
collections in linked tables with automatic totals and charts. Works in the browser on phone and
computer; everyone in the family signs in with their own account, and access rules can be set per
row, column and table.

- No setup needed: the owner becomes the installation admin on first sign-in; other family members
  get their own accounts when they sign in.
- Data lives in the plugin data directory: `home.sqlite3` (accounts/site), `docs/` (one `.grist`
  file per document, plain SQLite), `grist-sessions.db` (sessions). Back up the whole directory
  with the plugin stopped; files are owned by uid/gid 1001 inside the container.
- Known limits: Python formulas run **without a sandbox** (the upstream default sandbox needs
  privileges this plugin does not request) — do not import documents made by strangers. The plugin
  has no internet access, so the AI formula assistant, the online template gallery and the custom
  widget gallery cannot connect (they error out or show empty lists); documents themselves are
  unaffected. **The interface is English-only for now**: the 1.7.22 upstream images (both
  `gristlabs/grist` and `grist-oss`) do not serve the translation files the client loads (404), so
  switching the account language to Chinese still shows English. The translation files are bundled,
  so no plugin change is needed once upstream fixes it.
- Sign-in is handled by Wonton; “Log Out” inside Grist only clears Grist's own session and you are
  signed back in on the next page load.
- The image `gristlabs/grist-oss` is Grist Community edition, Apache-2.0; Grist and its logo are
  trademarks of Grist Labs (see NOTICE).
