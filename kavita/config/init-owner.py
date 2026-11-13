"""装好后的账号与发信设置引导（Kavita 健康后运行）。

Kavita 的第一个注册用户自动是管理员，而 OIDC 登录按邮箱关联已有账号，预置账号不会拿到管理员角色。
所以这里用官方接口注册管理员（用户名、邮箱、密码都来自核心注入），再打开自动登录，让用户经核心
单点登录进来时不再看到 Kavita 自己的登录页。

Kavita 的 SMTP 没有环境变量，只能经设置接口写进数据库，这里代填成核心注入的发信服务器，并把 HostName
设成插件的公开地址——Kavita 用它生成邮件里的链接，且 HostName 为空时上游认为"邮件未配置"
（IsEmailSetup 连 HostName 一起判断），找回密码等入口全部关着。

账号邮箱要和注入的内部邮箱对齐：老实例里存的是用户当时填的邮箱，OIDC 登录按邮箱认号，对不上会
另建一个没有管理员权限的账号。Kavita 改自己的邮箱没有免确认的接口（管理员改他人邮箱的接口明确
跳过本人），走它自己的改邮箱流程完成：申请改邮箱 → 用 resend-confirmation-email 取回确认链接 →
匿名接口 confirm-email-update 确认。申请会触发一封发往旧地址的通知邮件，旧地址发不出去，后台
任务会失败几次，只发生在对账这一次。

每次创建容器重跑，都已存在时幂等跳过。
"""

import json
import os
import sys
import urllib.error
import urllib.request
from urllib.parse import parse_qs, urlparse

BASE = "http://kavita:5000"


def request(method, path, payload=None, token=None):
    body = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request(BASE + path, data=body, method=method)
    req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req) as resp:
            return resp.status, resp.read().decode()
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()


def json_body(raw):
    try:
        return json.loads(raw)
    except ValueError:
        return raw


# 第一个管理员。已有管理员时接口返回 400（本地化的拒绝文案），此时靠登录验证是否已是我们建过的账号。
status, raw = request("POST", "/api/account/register", {
    "username": os.environ["WONTON_OWNER_NAME"],
    "email": os.environ["WONTON_OWNER_EMAIL"],
    "password": os.environ["WONTON_SECRET_OWNER_PASSWORD"],
})
if status == 200:
    print("已注册管理员", os.environ["WONTON_OWNER_NAME"])
else:
    print("注册未新建账号（HTTP %d），用登录验证" % status)

status, raw = request("POST", "/api/account/login", {
    "username": os.environ["WONTON_OWNER_NAME"],
    "password": os.environ["WONTON_SECRET_OWNER_PASSWORD"],
})
if status != 200:
    sys.exit(f"注册和登录都失败（HTTP {status}）：{raw[:200]}"
             "——密码不对（在应用里改过密码？）或账号被禁用时，先把密码改回插件生成的值再重装")
print("管理员账号可用")
token = json_body(raw)["token"]


def current_user():
    status, raw = request("GET", "/api/account", token=token)
    if status != 200:
        sys.exit(f"读取当前账号失败（HTTP {status}）：{raw[:200]}")
    return json_body(raw)


def login_and_verify_email(target):
    # 改邮箱后保险起见重新登录，确认新值真的落库
    status, raw = request("POST", "/api/account/login", {
        "username": os.environ["WONTON_OWNER_NAME"],
        "password": os.environ["WONTON_SECRET_OWNER_PASSWORD"],
    })
    if status != 200:
        sys.exit(f"改邮箱后登录失败（HTTP {status}）：{raw[:200]}")
    status, raw = request("GET", "/api/account", token=json_body(raw)["token"])
    if status != 200 or json_body(raw).get("email") != target:
        got = json_body(raw).get("email") if status == 200 else f"HTTP {status}"
        sys.exit(f"改邮箱后账号邮箱仍是 {got}，应为 {target}")


smtp_host = os.environ.get("WONTON_SMTP_HOST", "")
smtp_port = os.environ.get("WONTON_SMTP_PORT", "")
mail_from = os.environ.get("WONTON_MAIL_FROM", "")
app_url = os.environ.get("WONTON_APP_URL", "")
owner_email = os.environ["WONTON_OWNER_EMAIL"]

# GET /api/settings 返回的 OIDC Secret 是打码的，Kavita 在写入时按长度还原。
status, raw = request("GET", "/api/settings", token=token)
if status != 200:
    sys.exit(f"读取服务器设置失败（HTTP {status}）：{raw[:200]}")
settings = json_body(raw)

if not smtp_host or not smtp_port.isdigit() or not mail_from:
    sys.exit(f"发信变量不完整（host={smtp_host!r} port={smtp_port!r} from={mail_from!r}）："
             "notify 权限没生效或核心不支持 spec 0.11")

changed = []
smtp = settings["smtpConfig"]
smtp_target = {
    "host": smtp_host,
    "port": int(smtp_port),
    "senderAddress": mail_from,
    # 核心的发信服务器不认证不加密，凭据清掉（只有两者都非空 Kavita 才会发 AUTH）
    "userName": "",
    "password": "",
    "enableSsl": False,
}
for key, want in smtp_target.items():
    if smtp.get(key) != want:
        smtp[key] = want
        changed.append(f"smtp.{key}")

if app_url and settings["hostName"].rstrip("/") != app_url.rstrip("/"):
    settings["hostName"] = app_url.rstrip("/")
    changed.append("hostName")

if settings["oidcConfig"]["autoLogin"]:
    print("AutoLogin 已开启")
else:
    settings["oidcConfig"]["autoLogin"] = True
    changed.append("autoLogin")

if changed:
    status, raw = request("POST", "/api/settings", settings, token=token)
    if status != 200:
        sys.exit(f"写入服务器设置失败（HTTP {status}）：{raw[:200]}")
    print("已更新服务器设置：", "、".join(changed))
else:
    print("服务器设置已是目标值")

me = current_user()
if (me.get("email") or "") == owner_email:
    print("账号邮箱已是", owner_email)
else:
    if not app_url:
        sys.exit("账号邮箱需要对账，但没有 WONTON_APP_URL（HostName 为空时 Kavita 认为邮件未配置，"
                 "确认令牌拿不到）：核心没注入公开地址？")
    user_id = me["id"]
    status, raw = request("POST", "/api/account/update/email", {
        "email": owner_email,
        "password": os.environ["WONTON_SECRET_OWNER_PASSWORD"],
    }, token=token)
    if status != 200:
        sys.exit(f"申请改邮箱失败（HTTP {status}）：{raw[:200]}（密码不对或新邮箱被占用时在这里报）")
    status, raw = request("POST", f"/api/account/resend-confirmation-email?userId={user_id}", None, token=token)
    if status != 200:
        sys.exit(f"取邮箱确认链接失败（HTTP {status}）：{raw[:200]}")
    link = (json_body(raw) or {}).get("emailLink") or ""
    qs = parse_qs(urlparse(link).query)
    if "token" not in qs or not qs["token"][0]:
        sys.exit(f"确认链接里没有令牌：{link[:200]}")
    status, raw = request("POST", "/api/account/confirm-email-update", {
        "token": qs["token"][0],
        "email": owner_email,
    })
    if status != 200:
        sys.exit(f"确认新邮箱失败（HTTP {status}）：{raw[:200]}")
    login_and_verify_email(owner_email)
    print("账号邮箱已对齐为", owner_email)
