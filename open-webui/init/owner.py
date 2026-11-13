"""预建 OIDC 用户账号，并让核心注入的设置在保留数据重装后生效。

Open WebUI 0.13.0 启动时将默认设置写进 config 表，此后数据库里的值优先于环境变量。
模型网关可选（spec：“models”是“有网关就用”）：模型连接的数据库值不再整体清除，而是合并——
网关的连接（地址指向核心别名 models.wonton.internal 的那些）按这次注入的值重建，用户自己在
设置里加的其他连接原样保留；网关没接上时摘掉网关的连接，用户的连接照旧。
`ui.default_models` 和 `webui.url` 仍由核心管理，照旧清除后让注入值生效。
用户按不变的 OIDC sub 识别，不按邮箱合并账号。
"""

import asyncio
import json
import os
import sqlite3
import sys
import time
import uuid
from urllib.parse import urlsplit

# 核心在插件网络里的模型网关别名（核心配置的 PLUGIN_MODELS_HOST）：注入的网关地址都以它为主机名，
# 用户不会自己加指向这个别名的连接。
GATEWAY_HOST = "models.wonton.internal"

data = os.environ["DATA_DIR"]
name = os.environ["WONTON_OWNER_NAME"]
email = os.environ["WONTON_OWNER_EMAIL"].lower()

os.makedirs(os.path.join(data, "home"), exist_ok=True)
os.chdir("/app/backend")
sys.path.insert(0, "/app/backend")

# Open WebUI 0.13.0 的 config 模块导入时执行迁移，与应用使用同一入口，不再显式调用一次。
import open_webui.config  # noqa: E402, F401

db = sqlite3.connect(os.path.join(data, "webui.db"), timeout=30)
db.row_factory = sqlite3.Row
db.execute("pragma busy_timeout = 30000")

# 这次注入的网关连接；三个变量都非空才算接上（没接上时核心注入的是空字符串）。
gateway_url = os.environ.get("OPENAI_API_BASE_URLS", "").strip()
gateway_key = os.environ.get("OPENAI_API_KEYS", "")
gateway_connected = bool(gateway_url)

row = db.execute("select value from config where key = 'openai.api_base_urls'").fetchone()
if row is not None:
    try:
        urls = json.loads(row["value"])
        keys_row = db.execute(
            "select value from config where key = 'openai.api_keys'"
        ).fetchone()
        keys = json.loads(keys_row["value"]) if keys_row is not None else []
    except (TypeError, ValueError):
        sys.exit("数据库里的模型连接设置不是有效的 JSON，不覆盖，交给用户在设置里修")
    if not isinstance(urls, list) or not isinstance(keys, list):
        sys.exit("数据库里的模型连接设置不是列表，不覆盖，交给用户在设置里修")

    def is_gateway(url: str) -> bool:
        try:
            return urlsplit(url).hostname == GATEWAY_HOST
        except ValueError:
            return False

    # 摘掉网关的旧连接和空项（网关没接上时的初始值），剩下的是用户自己的。
    kept = [
        (url, keys[i] if i < len(keys) else "")
        for i, url in enumerate(urls)
        if url and not is_gateway(url)
    ]
    if gateway_connected:
        kept = [(gateway_url, gateway_key)] + [
            (url, key) for url, key in kept if url != gateway_url
        ]
    db.executemany(
        "delete from config where key = ?",
        [("openai.api_base_urls",), ("openai.api_keys",)],
    )
    db.executemany(
        "insert into config (key, value) values (?, ?)",
        [
            ("openai.api_base_urls", json.dumps([url for url, _ in kept])),
            ("openai.api_keys", json.dumps([key for _, key in kept])),
        ],
    )

db.executemany(
    "delete from config where key = ?",
    [(key,) for key in ("ui.default_models", "webui.url")],
)

exists = db.execute(
    "select 1 from user where json_extract(oauth, '$.oidc.sub') = ?", (name,)
).fetchone()
if exists:
    print(f"用户账号已存在：{name}")
    db.commit()
    sys.exit(0)

from open_webui.utils.auth import get_password_hash  # noqa: E402

user_id = str(uuid.uuid4())
now = int(time.time())
oauth = json.dumps({"oidc": {"sub": name}})
try:
    db.execute(
        "insert into user (id, name, email, role, oauth, profile_image_url, created_at,"
        " updated_at, last_active_at) values (?, ?, ?, 'admin', ?, '/user.png', ?, ?, ?)",
        (user_id, name, email, oauth, now, now, now),
    )
    db.execute(
        "insert into auth (id, email, password, active, session_stamp) values (?, ?, ?, 1, ?)",
        (user_id, email, asyncio.run(get_password_hash(str(uuid.uuid4()))), str(uuid.uuid4())),
    )
except sqlite3.IntegrityError:
    sys.exit(f"邮箱 {email} 已经被别的账号占着，建不了用户账号")
db.commit()
print(f"建好了用户账号：{name} <{email}>")
