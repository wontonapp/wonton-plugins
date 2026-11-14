# frozen_string_literal: true

# 幂等初始化，每次创建容器都会重跑：
# 1. 建主人管理员账号（provider/uid 与核心 OIDC 的 sub 一致，首次登录直接对上；email 用注入的内部邮箱）。
#    OIDC 自动注册的账号不是管理员，管理实例设置需要管理员，所以提前建。
# 2. 删上游种子在空库时建的 demo@dawarich.app（口令公开在 GitHub 仓库里）。主人账号建好之后
#    User.none? 永远不成立，种子不会再把它建回来。
# 3. 把主人账号当前的接口密钥写到 /run/api_key（核心收存，经 MCP 头带给上游）。应用里重新生成
#    密钥后不在这里改回去：等下次重建容器时按当前值重新收存。

owner_email = ENV.fetch('WONTON_OWNER_EMAIL')
owner_uid = ENV.fetch('WONTON_OWNER_NAME')

owner = User.unscoped.find_by(provider: 'openid_connect', uid: owner_uid)
unless owner
  password = SecureRandom.hex(32)
  owner = User.create!(
    email: owner_email,
    password: password,
    password_confirmation: password,
    admin: true,
    status: :active,
    active_until: 100.years.from_now,
    provider: 'openid_connect',
    uid: owner_uid
  )
end

User.unscoped.where(email: 'demo@dawarich.app').where.not(id: owner.id).destroy_all

File.write('/run/api_key', owner.api_key)

puts "Wonton: owner account ready (#{owner.email}), api key collected, seeded demo account removed"
