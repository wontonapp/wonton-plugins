# AI 资料笔记本（Open Notebook）

把网址、文件、视频链接丢进笔记本，AI 读完帮你答题、写摘要，还能生成播客。本插件用上游官方镜像
[lfnovo/open_notebook](https://hub.docker.com/r/lfnovo/open_notebook)（单容器版，内含 SurrealDB、API、
worker 和网页），按版本摘要固定，不改上游镜像。

- 上游文档：https://github.com/lfnovo/open-notebook/tree/main/docs
- 上游许可证：MIT；插件包的许可证见 [LICENSE](LICENSE)，图标来源和许可见 [NOTICE](NOTICE)。

## 云吞里的行为

- 模型：问问题、摘要、播客都走云吞的模型网关。插件把网关写成上游「OpenAI Compatible」服务商的
  环境变量回退（`OPENAI_COMPATIBLE_BASE_URL` / `OPENAI_COMPATIBLE_API_KEY`，见上游
  `open_notebook/ai/key_provider.py`）；装好后在上游设置页发现并添加网关里的模型即可。
  网关缺某类模型（如语音合成）时对应功能用不了，应用如实报错。
- 登录：应用只有一个口令（无多用户）。口令由云吞生成，主人在插件详情页「应用密码」查看；家人共用同一份
  笔记本，口令由主人分发。
- 数据：`/app/data`（上传的资料、抽取缓存）和 `/mydata`（内嵌 SurrealDB 的数据库）都挂在插件数据目录下，
  升级、重装不丢；卸载选删数据才清掉。
- 网页入口只经网关暴露 8502；应用内 `/api/*` 由上游 Next.js 反代到容器内的 5055，5055 不对外。
- 健康检查：上游 `GET /health`、`GET /api/config`（带真实数据库查询的 `dbStatus`）和网页 8502。

## 上游镜像版本

上游单容器镜像（`*-single` 标签）上游已声明 v2 将移除（见上游文档
[single-container.md](https://github.com/lfnovo/open-notebook/blob/main/docs/1-INSTALLATION/single-container.md)），
v1 内继续更新；上游推荐的新装是 Docker Compose 双容器版。云吞插件按包内 compose 就能迁双容器，
等上游移除单容器版时再迁。
