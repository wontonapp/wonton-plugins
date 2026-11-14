# 把 OIDC 提供方配置和账号口令写进 app.db（表名和列名来自上游 cps/ub.py、cps/config_sql.py 的 v4.0.8 源码）。
# 这些值上游只存在 app.db，没有环境变量；应用启动时把 oauthProvider 读进内存蓝图（generate_oauth_blueprints），
# 所以必须在应用启动前（本 oneshot 排在应用服务之前）写好。
#
# 每次创建容器都重写、不用"已存在就跳过"：保留数据重装后 WONTON_OIDC_CLIENT_SECRET 是新的
# （插件规范"OIDC"），不能拿旧密钥。应用启动后读到的就是写好的值。
import os
import sqlite3

from werkzeug.security import generate_password_hash

oidc = {k: os.environ[f"WONTON_OIDC_{k}"] for k in
        ("CLIENT_ID", "CLIENT_SECRET", "ISSUER", "AUTH_URL", "TOKEN_URL", "USERINFO_URL")}
assert all(oidc.values()), f"核心没有注入 OIDC 变量：{oidc}"
secret = os.environ["WONTON_SECRET_CLIENT_PASSWORD"]
owner = os.environ["WONTON_OWNER_NAME"]
password_hash = generate_password_hash(secret)

db = sqlite3.connect("/config/app.db")
# 代登录的旧开关关掉（本插件不用请求头认人）；登录类型显式复位成标准登录（0，上游默认值）：切成 OAuth（2）
# 会触发上游强制 SESSION_COOKIE_SECURE=True，纯 HTTP 的网关和局域网浏览器收不到会话，OIDC 反而走不通；
# 登录入口由清单 webui.path 直指 /login/generic，不依赖登录页上的按钮。
# OIDC 自动建的家人给 下载+网页阅读 角色（上游默认 0，家人登录后什么都干不了）；
# groups 里的 admin 组给主人管理员（上游默认开，写明）。
db.execute(
    """UPDATE settings SET
         config_login_type = 0,
         config_allow_reverse_proxy_header_login = 0,
         config_reverse_proxy_auto_create_users = 0,
         config_default_role = 258,
         config_enable_oauth_group_admin_management = 1"""
)

# generic 提供方行：上游 generate_oauth_blueprints 要求 github/google/generic 三行都在（缺前两行会整体
# 放弃 OAuth），且登录页模板按 id=3 认 generic。先清掉不是 id=3 的旧 generic 行（腾出 id），再照上游的
# 种子补齐 github/google（inactive），最后整行重写 generic（INSERT OR REPLACE，密钥会换）。
# 端点直接写死（不用 metadata_url，省一次应用启动时的联网抓取）；scope 带 groups，groups 里才有 admin 组。
db.execute("DELETE FROM oauthProvider WHERE provider_name = 'generic' AND id != 3")
db.execute("INSERT OR IGNORE INTO oauthProvider (id, provider_name, active) VALUES (1, 'github', 0)")
db.execute("INSERT OR IGNORE INTO oauthProvider (id, provider_name, active) VALUES (2, 'google', 0)")
db.execute(
    """INSERT OR REPLACE INTO oauthProvider
         (id, provider_name, active, oauth_client_id, oauth_client_secret,
          oauth_base_url, oauth_authorize_url, oauth_token_url, oauth_userinfo_url,
          metadata_url, scope, username_mapper, email_mapper, oauth_admin_group, login_button)
       VALUES (3, 'generic', 1, ?, ?, ?, ?, ?, ?, NULL, 'openid profile email groups',
               'preferred_username', 'email', 'admin', 'OpenID Connect')""",
    (oidc["CLIENT_ID"], oidc["CLIENT_SECRET"], oidc["ISSUER"], oidc["AUTH_URL"],
     oidc["TOKEN_URL"], oidc["USERINFO_URL"]),
)

# 内置 admin 是上游用公开默认口令建的（constants.DEFAULT_PASSWORD）：换成核心生成的口令停用它，
# 管理能力走主人的 OIDC 账号（admin 组）。主人的账号由应用在他首次 OIDC 登录时自动建（口令为空），
# 同步成同一个口令，lan.client 显示的账号密码才能连 OPDS；没登录过就不存在，下次创建容器再同步。
db.execute("UPDATE user SET password = ? WHERE lower(name) = 'admin'", (password_hash,))
db.execute("UPDATE user SET password = ? WHERE lower(name) = ?", (password_hash, owner.lower()))
db.commit()

row = db.execute(
    "SELECT id, active, oauth_client_id, scope FROM oauthProvider WHERE provider_name = 'generic'"
).fetchone()
settings_row = db.execute(
    "SELECT config_login_type, config_default_role, config_enable_oauth_group_admin_management FROM settings"
).fetchone()
seeded = db.execute(
    "SELECT count(*) FROM oauthProvider WHERE id IN (1, 2) AND provider_name IN ('github', 'google')"
).fetchone()[0]
owners = db.execute("SELECT count(*) FROM user WHERE lower(name) = ?", (owner.lower(),)).fetchone()[0]
db.close()

if row != (3, 1, oidc["CLIENT_ID"], "openid profile email groups"):
    raise SystemExit(f"[wonton-sso] 提供方没写进去：{row}")
if settings_row != (0, 258, 1) or seeded != 2:
    raise SystemExit(f"[wonton-sso] 设置没写进去：{settings_row}（种子行 {seeded}）")
print(f"[wonton-sso] OIDC 已配置（{oidc['ISSUER']}），默认角色 下载+网页阅读，admin 已停用；"
      + ("主人账号已同步口令" if owners == 1 else "主人还没登录过，账号口令待首次登录后同步"))
