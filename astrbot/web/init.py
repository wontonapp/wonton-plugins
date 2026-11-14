"""首次启动前写好 AstrBot 的本地配置。

三件事，都只动自己引入的条目，用户在管理面板里改过的其他内容原样保留：

1. 管理面板登录密码：上游把密码存成 pbkdf2_sha256（600000 轮、16 字节盐）加一份 md5
   回退，这里调用上游自己的哈希函数，把插件密钥（App 详情页可查看）写成面板密码。
   只在 AstrBot 这次加载刚生成过初始密码时写入——那正是首次安装或配置被清空的时候；
   用户之后在面板里改的密码不会被覆盖。写完把「要求改密码」的标记去掉，
   用户用 App 里查到的密码直接登录。
2. 模型网关：接上时写入一个 OpenAI 兼容供应商，密钥在配置里只存 $WONTON_MODEL_API_KEY
   引用，应用加载时从环境变量取值，网关凭据不落盘；没接上时删掉这个供应商，
   模型要用户自己在面板里配，网关装好后核心重建容器会把它写回来。
3. 工具总线：data/mcp_server.json 里登记一个 streamable_http 服务，指向核心的
   MCP 总线。等待用户确认的敏感工具调用可能要等十几分钟，超时按 30 分钟设。
"""

import json
import os
import sys
import tempfile

DATA_DIR = "/AstrBot/data"
MCP_CONFIG_PATH = os.path.join(DATA_DIR, "mcp_server.json")
GATEWAY_PROVIDER_ID = "wonton-gateway"
GATEWAY_MCP_NAME = "wonton"

# 敏感工具调用要等用户在 App 里确认，客户端超时给足（秒）。
MCP_TIMEOUT_SECONDS = 1800


def fail(message: str) -> None:
    print(f"[wonton-init] {message}", file=sys.stderr)
    sys.exit(1)


def log(message: str) -> None:
    print(f"[wonton-init] {message}", flush=True)


def env(name: str) -> str:
    return os.environ.get(name, "") or ""


def main() -> None:
    # 导入即完成 cmd_config.json 的创建或加载；上游在首次部署时会生成一份随机初始密码
    # 并在配置对象上留标记，靠这个标记识别「这是刚生成的，可以换成插件密钥」。
    from astrbot.core import astrbot_config as cfg
    from astrbot.dashboard.password_state import (
        PASSWORD_CHANGE_REQUIRED_KEY,
        PASSWORD_STORAGE_UPGRADED_KEY,
        set_dashboard_password_hashes,
    )

    changed = False

    if getattr(cfg, "_generated_dashboard_password", None) is not None:
        secret = env("WONTON_SECRET_DASHBOARD_PASSWORD")
        if not secret:
            fail("插件密钥 WONTON_SECRET_DASHBOARD_PASSWORD 没有值，无法设置面板登录密码")
        try:
            set_dashboard_password_hashes(cfg, secret)
        except ValueError as exc:
            fail(f"生成密码哈希失败：{exc}")
        dashboard = cfg["dashboard"]
        dashboard[PASSWORD_CHANGE_REQUIRED_KEY] = False
        dashboard[PASSWORD_STORAGE_UPGRADED_KEY] = True
        changed = True
        log("已把插件密钥写成管理面板的登录密码（用户名 astrbot）")

    # ---- 模型网关 ----
    base_url = env("WONTON_MODEL_BASE_URL")
    api_key = env("WONTON_MODEL_API_KEY")
    default_model = env("WONTON_MODEL_DEFAULT")
    gateway_connected = bool(base_url and api_key and default_model)

    providers = cfg.get("provider")
    if not isinstance(providers, list):
        providers = []
        cfg["provider"] = providers

    def is_gateway(entry: object) -> bool:
        return isinstance(entry, dict) and entry.get("id") == GATEWAY_PROVIDER_ID

    if gateway_connected:
        gateway = next((p for p in providers if is_gateway(p)), None)
        if gateway is None:
            # 字段同上游面板「OpenAI Compatible」模板；model 只在建条目时填一次。
            providers.append(
                {
                    "id": GATEWAY_PROVIDER_ID,
                    "provider": "openai",
                    "type": "openai_chat_completion",
                    "provider_type": "chat_completion",
                    "enable": True,
                    "key": ["$WONTON_MODEL_API_KEY"],
                    "api_base": base_url,
                    "model": default_model,
                    "timeout": 120,
                    "proxy": "",
                    "custom_headers": {},
                }
            )
            agent_runner = cfg.get("agent_runner") or {}
            model_config = (agent_runner.get("config") or {}).get("model") or {}
            if isinstance(model_config, dict):
                model_config["provider_id"] = GATEWAY_PROVIDER_ID
            changed = True
            log(f"已接入模型网关，默认模型 {default_model}")
        elif gateway.get("api_base") != base_url:
            gateway["api_base"] = base_url
            changed = True
            log("已更新模型网关地址")
    elif any(is_gateway(p) for p in providers):
        cfg["provider"] = [p for p in providers if not is_gateway(p)]
        agent_runner = cfg.get("agent_runner") or {}
        model_config = (agent_runner.get("config") or {}).get("model") or {}
        if isinstance(model_config, dict) and model_config.get("provider_id") == GATEWAY_PROVIDER_ID:
            model_config["provider_id"] = ""
        changed = True
        log("模型网关没接上，已移除网关供应商；模型要在面板里自己配")

    if changed:
        cfg.save_config()
        log("已写入 cmd_config.json")

    # ---- 工具总线 ----
    mcp_url = env("WONTON_MCP_URL")
    if not mcp_url:
        fail("WONTON_MCP_URL 没有注入：tools 权限应带有核心工具总线的地址")
    wonton_mcp = {
        "url": mcp_url,
        "transport": "streamable_http",
        "active": True,
        "timeout": MCP_TIMEOUT_SECONDS,
        "sse_read_timeout": MCP_TIMEOUT_SECONDS,
    }

    mcp_config = None
    if os.path.exists(MCP_CONFIG_PATH):
        try:
            with open(MCP_CONFIG_PATH, encoding="utf-8-sig") as f:
                mcp_config = json.load(f)
        except (OSError, ValueError) as exc:
            fail(f"解析 {MCP_CONFIG_PATH} 失败：{exc}")
        if not isinstance(mcp_config, dict) or not isinstance(mcp_config.get("mcpServers", {}), dict):
            fail(f"{MCP_CONFIG_PATH} 的 mcpServers 不是对象，不敢改写")
    else:
        mcp_config = {"mcpServers": {}}

    servers = mcp_config["mcpServers"]
    if servers.get(GATEWAY_MCP_NAME) != wonton_mcp:
        servers[GATEWAY_MCP_NAME] = wonton_mcp
        fd, tmp_path = tempfile.mkstemp(dir=DATA_DIR, prefix=".mcp_server.", suffix=".tmp")
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(mcp_config, f, ensure_ascii=False, indent=4)
        os.replace(tmp_path, MCP_CONFIG_PATH)
        log("已把核心工具总线写进 mcp_server.json")

    log("初始化完成")


if __name__ == "__main__":
    main()
