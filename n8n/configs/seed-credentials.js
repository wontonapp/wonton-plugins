// 一次性服务：把模型网关写成 n8n 凭据，用户在 AI 节点里选它就能用，不用自己填 key。
// 在主进程启动前跑：import/export 命令会自己建库、跑迁移，退出码 0 才放行主服务。
// 幂等：已存在同名凭据就跳过（不覆盖用户改过的值）。
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');

const NAME = 'Wonton model gateway';
const baseUrl = process.env.WONTON_MODEL_BASE_URL || '';
const apiKey = process.env.WONTON_MODEL_API_KEY || '';

// 没接上网关时核心注入的是空串：不预置，等接上后核心重建容器时再跑到这里。
if (!baseUrl || !apiKey) {
  console.log('model gateway not connected, nothing to seed');
  process.exit(0);
}

// 空库时 export:credentials 以 "No credentials found" 退出 1、不写文件：这是"没有已存凭据"，不是错误。
const exported = '/tmp/existing-credentials.json';
const listed = require('node:child_process').spawnSync('n8n', ['export:credentials', '--all', `--output=${exported}`], { stdio: 'inherit' });
const existing = listed.status === 0 && fs.existsSync(exported) ? JSON.parse(fs.readFileSync(exported, 'utf8')) : [];
if (existing.some((c) => c.name === NAME)) {
  console.log('gateway credential already present');
  process.exit(0);
}

// openAiApi 的 url 字段就是 Base URL，官方支持 OpenAI 兼容端点；明文 data 由 import 自动加密入库。
const payload = [{ name: NAME, type: 'openAiApi', data: { apiKey, url: baseUrl } }];
fs.writeFileSync('/tmp/seed-credentials.json', JSON.stringify(payload));
execFileSync('n8n', ['import:credentials', '--input=/tmp/seed-credentials.json'], { stdio: 'inherit' });
console.log('gateway credential seeded');
