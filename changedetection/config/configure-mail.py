"""一次性把全局邮件通知指到主人的内部邮箱（信进 Wonton 私聊）。

应用只认 datastore 里的通知 URL，没有 SMTP 环境变量；对外稳定可用的口子是它自己的 REST API
（docs/api-spec.yaml 的 /notifications，POST 重复加同一个 URL 会被应用按原样去重）。等应用健康后
调它；在 datastore 里留一个标记，之后不再运行——用户在界面里改掉或删掉通知，重装/升级不会改回来。
"""
import json
import os
import sys
import time
import urllib.error
import urllib.request

DATASTORE = os.environ.get("DATASTORE", "/datastore")
MARKER = os.path.join(DATASTORE, ".wonton-mail-configured")
API_URL = "http://changedetection:5000/api/v1/notifications"
# 核心注入的这几样都只含主机名、数字和邮箱，遇到别的字符说明值不对，拼出来的 URL 也不可信。
ALLOWED = frozenset("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789.@_:-")


def fail(message):
    print(f"configure-mail: {message}", file=sys.stderr)
    sys.exit(1)


def env(name):
    value = os.environ.get(name, "").strip()
    if not value:
        fail(f"{name} 为空：核心没注入安装变量")
    if not set(value) <= ALLOWED:
        fail(f"{name} 含拼 mailto:// 不该有的字符：{value!r}")
    return value


def api_token():
    """应用的 API 默认开着密钥校验，密钥在 datastore 的 JSON 里。不校验时返回 None。"""
    for name in ("changedetection.json", "url-watches.json"):
        try:
            with open(os.path.join(DATASTORE, name), encoding="utf-8") as f:
                application = json.load(f).get("settings", {}).get("application", {})
        except FileNotFoundError:
            continue
        except (OSError, ValueError) as e:
            fail(f"读不了 {name}：{e}")
        if not application.get("api_access_token_enabled"):
            return None
        token = application.get("api_access_token")
        if not token:
            fail(f"{name} 开着 API 校验却没有 api_access_token")
        return token
    fail(f"{DATASTORE} 里没有 datastore JSON：应用没正常启动过")


def add_notification_url(url, token):
    body = json.dumps({"notification_urls": [url]}).encode()
    headers = {"Content-Type": "application/json"}
    if token:
        headers["x-api-key"] = token
    last = ""
    # depends_on 已等应用健康；重试只兜应用刚重启的那一下。
    for attempt in range(5):
        try:
            with urllib.request.urlopen(urllib.request.Request(API_URL, data=body, headers=headers, method="POST"), timeout=30) as response:
                if response.status == 201:
                    return
                last = f"HTTP {response.status}: {response.read().decode(errors='replace')}"
        except urllib.error.HTTPError as e:
            last = f"HTTP {e.code}: {e.read().decode(errors='replace')}"
        except OSError as e:
            last = str(e)
        time.sleep(3)
    fail(f"应用没接受通知 URL（{last}）")


def main():
    if os.path.exists(MARKER):
        print("configure-mail: 已配置过，不改动")
        return
    url = "mailto://{}:{}/?from={}&to={}".format(
        env("WONTON_SMTP_HOST"), env("WONTON_SMTP_PORT"), env("WONTON_MAIL_FROM"), env("WONTON_OWNER_EMAIL"))
    add_notification_url(url, api_token())
    try:
        tmp = f"{MARKER}.tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            f.write(url + "\n")
        os.replace(tmp, MARKER)
    except OSError as e:
        fail(f"写不了标记 {MARKER}：{e}")
    print(f"configure-mail: 全局邮件通知已设为 {url}")


if __name__ == "__main__":
    main()
