// 把模型网关的连接写进默认用户（default-user）的设置，装好就能直接对话。
// 只在两种情况下动手，其余一律不碰（用户自己改过的连接不覆盖）：
//  1. settings.json 还不存在（首次安装，应用还没建）：以镜像自带的上游默认设置为底写进连接；
//  2. data/.wonton-model.json 存在（连接是本脚本之前写的）且地址没被改过：按本次注入的值刷新。
//     重装插件会换网关凭据，旧值会失效，所以重连后要刷新；模型名只在用户没换过时跟着刷新。
// 网关没接上（注入值为空）时不写也不删；密钥写进应用自己的 secrets.json，不落到别的用户可见的地方。
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const DATA_ROOT = '/home/node/app/data';
const USER_DIR = path.join(DATA_ROOT, 'default-user');
const SETTINGS_FILE = path.join(USER_DIR, 'settings.json');
const SECRETS_FILE = path.join(USER_DIR, 'secrets.json');
const MARKER_FILE = path.join(DATA_ROOT, '.wonton-model.json');
const UPSTREAM_DEFAULT_SETTINGS = '/home/node/app/default/content/settings.json';

const baseUrl = (process.env.WONTON_MODEL_BASE_URL || '').trim();
const apiKey = (process.env.WONTON_MODEL_API_KEY || '').trim();
const defaultModel = (process.env.WONTON_MODEL_DEFAULT || '').trim();

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    return null;
  }
}

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const tmp = `${file}.seed-tmp`;
  fs.writeFileSync(tmp, JSON.stringify(value, null, 4));
  fs.renameSync(tmp, file);
}

function applyConnection(settings, url, model) {
  settings.main_api = 'openai';
  settings.oai_settings = { ...(settings.oai_settings || {}) };
  settings.oai_settings.chat_completion_source = 'custom';
  settings.oai_settings.custom_url = url;
  settings.oai_settings.custom_model = model;
}

// 把网关凭据写进应用的 secrets.json（应用读的是 active 的那条）。
// 用户自己存过 api_key_custom 时不加不加改，应用里换密钥由用户自己来。
// 返回我们这条的 id，没写成时返回 null。
function writeSecretEntry(key, existingId) {
  const secrets = readJson(SECRETS_FILE) || {};
  const list = Array.isArray(secrets.api_key_custom) ? secrets.api_key_custom : [];
  let entry = existingId ? list.find((s) => s && s.id === existingId) : undefined;
  if (entry) {
    entry.value = key;
    entry.active = true;
  } else if (list.length === 0) {
    entry = { id: crypto.randomUUID(), value: key, label: 'api_key_custom', active: true };
    secrets.api_key_custom = [entry];
  } else {
    return null;
  }
  writeJson(SECRETS_FILE, secrets);
  return entry.id;
}

if (!baseUrl || !apiKey || !defaultModel) {
  console.log('模型网关没有接上（注入值为空），不写模型连接。');
  process.exit(0);
}

const marker = readJson(MARKER_FILE);
const settings = readJson(SETTINGS_FILE);

if (!settings) {
  const seeded = readJson(UPSTREAM_DEFAULT_SETTINGS) || {};
  applyConnection(seeded, baseUrl, defaultModel);
  const secretId = writeSecretEntry(apiKey, null);
  writeJson(SETTINGS_FILE, seeded);
  writeJson(MARKER_FILE, { url: baseUrl, model: defaultModel, secret_id: secretId });
  console.log('已把模型网关的连接写进默认用户的设置。');
  process.exit(0);
}

const oai = settings.oai_settings || {};
if (marker && oai.custom_url === marker.url) {
  applyConnection(settings, baseUrl, oai.custom_model || defaultModel);
  const secretId = writeSecretEntry(apiKey, marker.secret_id);
  writeJson(SETTINGS_FILE, settings);
  writeJson(MARKER_FILE, { url: baseUrl, model: settings.oai_settings.custom_model, secret_id: secretId ?? marker.secret_id });
  console.log('已按本次注入的值刷新模型网关连接。');
  process.exit(0);
}

const upstream = readJson(UPSTREAM_DEFAULT_SETTINGS);
// 连接相关的设置（接口选择和整个 oai_settings）和上游默认值逐一相同，才算用户还没配过。
const sorted = (v) =>
  Array.isArray(v) ? v.map(sorted)
    : v && typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, sorted(v[k])]))
    : v;
const untouched =
  upstream &&
  settings.main_api === upstream.main_api &&
  JSON.stringify(sorted(oai)) === JSON.stringify(sorted(upstream.oai_settings));
if (untouched) {
  applyConnection(settings, baseUrl, defaultModel);
  const secretId = writeSecretEntry(apiKey, null);
  writeJson(SETTINGS_FILE, settings);
  writeJson(MARKER_FILE, { url: baseUrl, model: defaultModel, secret_id: secretId });
  console.log('模型连接还是出厂默认，已写进模型网关的连接。');
  process.exit(0);
}

console.log('模型连接已由用户配置，不改动。');
