# 云端浏览器（Neko）

在家里的服务器上打开一个浏览器，手机上就能远程看和操作，像把浏览器装在家里带着走。

可以用它打开家里网络才能访问的网页，也可以和家人一起看同一个页面。页面在服务器上加载，手机只接收画面和操作，不把浏览记录留在手机浏览器里。

第一次打开时，在插件详情页查看普通用户密码并登录 Neko；管理员密码用于管理房间。WebRTC 画面使用一个 UDP 端口，插件页打开时云吞 App 会自动把它映射到手机本机，离开页面后释放。

图标使用 Neko 上游标志，来源、版权、许可证和商标说明见 [NOTICE](NOTICE)。

通用开发流程见 [Wonton 开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)，清单字段见 [插件规范](https://github.com/wontonapp/wonton-docs/blob/main/plugin-spec.md)。
