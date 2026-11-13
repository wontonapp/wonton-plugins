// 云吞插件预置的 settings.js：首次安装时由一次性服务复制进数据目录，之后不覆盖用户改动。
//
// 密码和凭据加密密钥不在文件里，每次启动从环境变量现取：换了密钥重启即生效，文件不用动。
// 铁则（失败即停）：密钥没注入就抛错拒启，不给空密码兜底。
const password = process.env.WONTON_SECRET_ADMIN_PASSWORD;
const credentialSecret = process.env.WONTON_SECRET_CREDENTIAL_SECRET;
if (!password || !credentialSecret) {
    throw new Error("WONTON_SECRET_ADMIN_PASSWORD 或 WONTON_SECRET_CREDENTIAL_SECRET 没有注入，拒绝启动");
}

module.exports = {
    // 核心网关从插件网络访问，必须监听所有网卡（Node-RED 默认就是 0.0.0.0，写明防上游改动）。
    uiHost: "0.0.0.0",
    credentialSecret,
    adminAuth: {
        type: "credentials",
        users: [
            {
                username: "admin",
                // bcryptjs 在镜像的 NODE_PATH 里（/usr/src/node-red/node_modules）。
                password: require("bcryptjs").hashSync(password, 8),
                permissions: "*"
            }
        ]
    }
};
