# 笔记同步（Obsidian LiveSync）

把家里电脑和手机里的 Obsidian 笔记同步到一起，像自己的百度网盘，数据保存在自己家里。

## 怎么用

1. 在 Obsidian 里安装社区插件 **Self-hosted LiveSync**。
2. 在本插件详情里打开“局域网入口”，复制显示的账号和密码。
3. 在本插件详情里打开“HTTPS 客户端入口”，复制显示的 HTTPS 地址。两种入口使用同一组账号和密码。
4. 电脑在家里使用局域网入口的地址；手机使用 HTTPS 客户端入口的地址。
5. 在 LiveSync 的连接设置里填地址、账号、密码，数据库名填 `obsidiandb`，按插件引导完成第一次同步。

手机入口需要家里已经配置好自己的域名和系统信任的 HTTPS 证书。开发栈使用测试证书时，只适合用 curl 验证请求，不适合真实手机。

上游文档：<https://github.com/vrtmrz/obsidian-livesync/blob/main/docs/setup_own_server.md>

## 数据存在哪、备份要注意什么

- 笔记保存在这个插件的数据目录里，没有别的副本。
- 备份前先在插件详情里停止插件；CouchDB 运行中直接拷贝文件可能得到不完整的备份。
- LiveSync 支持端到端加密。打开后，服务器保存的是加密后的笔记。

## 已知限制

- 手机端必须使用系统信任的 HTTPS 证书。自签的开发证书只用于开发环境里的请求验证，不能替代正式证书。
- 没有给人使用的管理网页：CouchDB 管理页不作为插件入口提供。同步只使用 LiveSync 的请求接口。

## 权限

- `network.lan`：允许电脑从家里的局域网直连同步端口。
- `network.clients`：允许主人在 Wonton 里打开 HTTPS 客户端入口，给手机和其他第三方客户端使用。
- `owner.identity`：按主人的用户名创建 CouchDB 客户端账号，密码由核心生成。

## 许可证和商标

- 插件包：MIT，见 [LICENSE](LICENSE)；素材声明见 [NOTICE](NOTICE)。
- 服务端 Apache CouchDB：Apache License 2.0（官方镜像 `couchdb`）。
- 客户端 Self-hosted LiveSync：MIT。Obsidian 是 Dynalist Inc. 的商标；本插件与上游项目无从属或背书关系。
