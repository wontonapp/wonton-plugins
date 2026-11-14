# 云吞插件：思源笔记

把[思源笔记](https://github.com/siyuan-note/siyuan)装进云吞：本地优先的个人知识库，块级引用、双链和
Markdown 所见即所得，浏览器即开即用，手机浏览器是移动版布局。

开发流程见[插件开发指南](https://github.com/wontonapp/wonton-docs/blob/main/develop.md)，
规则见[插件规范](https://github.com/wontonapp/wonton-docs/blob/main/plugin-spec.md)。

## 用法

- 第一次打开是思源自带的锁屏：**访问授权码由云吞安装时生成**，打开页面的引导（`/`，即 `/welcome`）
  会把它显示出来，复制回锁屏即可进入（可勾选「记住」，之后不再输入）；建议登录后在思源
  「设置 - 关于」里改成自己的。
- 笔记、图片附件和索引存在这台服务器的插件数据目录（`workspace/`）。备份就是备份这个目录。
- 不申请出网权限：集市主题和插件下载、云同步、AI 写作这些要连外网的功能用不了；笔记数据不出家门。

## 登录为什么不是 OIDC

思源上游支持 OIDC（`SIYUAN_OIDC_*`），但它的网页登录要求回调地址是 https：内核
`model/oidc.go` 对非本机请求强制 https（`Public OIDC redirect URL must use HTTPS`），而云吞给
插件的网页地址是 http。单容器实测：按云吞的注入方式配 `SIYUAN_OIDC_*`（http 回调），容器启动即
退出（`Docker access authentication configuration is invalid`，退出码 22），插件装不上；就算跳过
启动校验，身份提供方登记的回调也是 http，思源发出的 https 回调对不上，登录链走不完。
顺带说明：issuer 只在 loopback/`.localhost` 域名下接受 http，生产域名上 http 的
`SIYUAN_OIDC_ISSUER_URL` 本身也会被思源拒绝——所以这不只是云吞开发环境的问题。

所以按云吞的 `sso: none` 接：网关把关，应用侧用思源自己的锁屏密码（访问授权码）。等核心网关有
https 公开地址后，改法是：`webui.sso` 换成 `oidc`、`oidc.redirect_paths` 配
`/api/system/oidc/callback`、compose 里把 `SIYUAN_ACCESS_AUTH_CODE` 换成
`SIYUAN_OIDC_*` 三件套（issuer、client id、client secret）并设
`SIYUAN_OIDC_REDIRECT_URL=${WONTON_APP_URL}/api/system/oidc/callback`，引导页可以下掉。

## 镜像

- `b3log/siyuan`：上游官方镜像（amd64/arm64 多架构，Docker Hub），按清单列表摘要固定；
  许可证 AGPL-3.0-or-later。
- `nginx`：上游官方镜像（nginx 官方 Docker 镜像，amd64/arm64 多架构），按清单列表摘要固定；
  许可证 BSD-2-Clause。用途只有一个：`/` 和 `/welcome` 出引导页（把访问授权码交给经网关登录的
  主人），其余请求原样反代给思源（保留 Host、转发 WebSocket）。

## 图标

`icon.svg` 来自思源笔记官方仓库的 `app/stage/icon.svg`，原样使用；它是云南彼立练迪科技有限公司
（b3log.org）的商标。来源、许可全文和商标说明见 [NOTICE](NOTICE)。
