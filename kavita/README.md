# 云吞插件：Kavita

[Kavita](https://www.kavitareader.com) 是自托管的漫画和电子书阅读服务器。这个插件把它装进云吞：
登录走云吞账号（单点登录），打开就是自己的书架；媒体放在共享文件夹 `media` 里，入库扫描只读访问。

安装后第一次打开，插件会用你的云吞用户名和邮箱在 Kavita 里建好管理员账号并打开自动登录，
你不会看到 Kavita 自己的登录页。管理员密码由云吞生成保管，正常使用用不到；想直接登 Kavita
时可以在插件详情里看说明。

## 说明

- 媒体文件：放进共享文件夹 `media`，在 Kavita 网页里建书库时选择 `/media` 下的目录。
- 应用发的信（找回密码、邀请、Send To 设备）会出现在收件人的「Wonton」私聊里，SMTP 已自动配好。
  收件人只能是家里人：发给外部地址（比如 Kindle 的 @kindle.com）在发送时就失败。

- 换核心时容器会重建。浏览器里的单点登录走不完回调：OIDC 回调是跨站 POST，被网关的会话校验挡住回 401
  ——核心侧修（[wontonapp/wonton#154](https://github.com/wontonapp/wonton/issues/154)，改法见
  [wontonapp/wonton#183](https://github.com/wontonapp/wonton/pull/183)），修好前打不开。
- 图标使用 Kavita 官方标志（原样使用，仅加白色底），来源和许可见 [NOTICE](NOTICE)。
- 开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)，
  规则见[插件规范](https://github.com/wontonapp/wonton-docs/blob/main/plugin-spec.md)。
