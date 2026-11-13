"""把核心注入的 OIDC 客户端写进 Kavita 的 appsettings。

Kavita 每次启动都从 appsettings 读 Authority/ClientId/Secret 同步进数据库（Seed.SetOidcSettingsFromDisk），
所以这里每次创建容器都要重写一遍：保留数据重装时客户端密钥会换。只改 OpenIdConnectSettings 和缺省的
TokenKey，其余键（Port、BaseUrl、InstallId 等）原样保留。

ASPNETCORE_ENVIRONMENT=Development 时 Kavita.Common 的 Configuration 静态初始化把配置文件定死为
appsettings.Development.json（缺失时整份配置报错回默认值），所以两个文件都要写同样的 OIDC 节。
"""

import json
import os

CONFIG_DIR = "/config"

oidc = {
    "Authority": os.environ["WONTON_OIDC_ISSUER"],
    "ClientId": os.environ["WONTON_OIDC_CLIENT_ID"],
    "Secret": os.environ["WONTON_OIDC_CLIENT_SECRET"],
    # Hydra 的 discovery scopes_supported 只有 openid/offline 系列，Kavita 会按它过滤掉
    # profile/email/groups；CustomScopes 无条件追加（上游 GetValidScopes），认人的 email 靠这里要回来。
    "CustomScopes": ["profile", "email", "groups"],
}

# ASPNETCORE_ENVIRONMENT=Development 时 Kavita 不自动生成 TokenKey，这里固定给一个随机值。
token_key = os.environ["WONTON_SECRET_JWT_TOKEN"]


def merge(filename):
    """改写 config/<filename> 的 OpenIdConnectSettings 和缺省的 TokenKey，其余键保留。"""
    path = os.path.join(CONFIG_DIR, filename)
    data = {}
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            data = json.load(f)

    data["OpenIdConnectSettings"] = dict(oidc)
    data.setdefault("TokenKey", token_key)
    if "Port" not in data:
        data["Port"] = 5000
    if "BaseUrl" not in data:
        data["BaseUrl"] = "/"

    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)
    os.replace(tmp, path)


for name in ("appsettings.json", "appsettings.Development.json"):
    merge(name)
print("OpenIdConnectSettings 已写入 appsettings*.json")
