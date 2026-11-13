# 电影下载（Radarr）

把想看的电影加进片单，Radarr 会自动搜索、下载、按电影名整理进媒体库，配好 Jellyfin 打开就能看。下载客户端（qBittorrent 插件）和通知邮件装好即接好。

## 用法

1. 装好后打开插件网页，把想看的电影加进片单（搜索片名 → 选质量 → 加入）。
2. 装了 qBittorrent 插件的，下载自动进它的队列，下完自动整理进共享文件夹 `media` 的 `movies/` 目录。
3. 电影下好、质量升级或有健康问题时，收件人的 Wonton 私聊里会收到一封通知邮件。

## 配套插件

| 插件 | 关系 |
| --- | --- |
| qBittorrent | 下载客户端，把种子下到共享文件夹 `downloads`，Radarr 从那里导入 |
| Prowlarr | 索引站统一管理，自动把索引器同步给 Radarr |
| Jellyfin | 播放 `media` 共享文件夹里的电影库 |
| Seerr | 家里人在上面求片，Radarr 负责下载（待插件间依赖支持） |

## 说明

- 网页经云吞网关打开，家人要用先在 App 里共享；应用自己不再设账号密码。
- 搜索元数据（api.radarr.video、TMDB）需要能访问外网，装的时候会确认这项权限。
- 电影数据保存在家里（应用数据和 `media` 共享文件夹），卸载时选保留就还在。

## 图标

`icon.png` 用 Radarr 官方标志（[GitHub 组织头像](https://github.com/Radarr)，取自 radarr.video），放在白底上缩放而成；Radarr 名称与标志是 Radarr 项目的商标，见 [NOTICE](NOTICE)。
