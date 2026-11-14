// Vane 的配置文件由应用改写；每次创建插件容器前按固定 provider id 更新网关凭据。
//
// Vane 的设置界面也写这个文件，所以只补自己的两项：按固定的 provider id 找，找不到才建；模型和旧值留着，
// 只把网关地址和凭据换成这次注入的。文件解析不了时不覆盖，交给安装失败。
import fs from 'node:fs';
import path from 'node:path';

const CHAT_PROVIDER_ID = '00000000-0000-4000-8000-000000000001';
const EMBEDDING_PROVIDER_ID = '00000000-0000-4000-8000-000000000002';
const EMBEDDING_MODEL = 'Xenova/paraphrase-multilingual-MiniLM-L12-v2';
// sha256("{}")：应用按这个 hash 认出环境里已经建过的 Transformers provider，不再重复建一个。
const EMPTY_HASH = '44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a';
const SEARXNG_URL = 'http://127.0.0.1:8080';

const fail = (message) => {
  console.error(message);
  process.exit(1);
};

const env = (name) => process.env[name] ?? '';

// 模型网关可选（spec：“models”是“有网关就用”）：没接上时三个变量都是空字符串，
// 只摘掉网关的 chat provider（按固定 id 找），用户自己加的 provider 和本地 embedding 照旧；
// 网关装好后核心会重建容器，seed 重跑，网关的 provider 再回来。
const baseURL = env('WONTON_MODEL_BASE_URL');
const apiKey = env('WONTON_MODEL_API_KEY');
const chatModel = env('WONTON_MODEL_DEFAULT');
const gatewayConnected = Boolean(baseURL && apiKey && chatModel);

const configPath = path.join(process.env.DATA_DIR || '/home/vane/data', 'config.json');
fs.mkdirSync(path.dirname(configPath), { recursive: true });

const empty = () => ({
  version: 1,
  setupComplete: false,
  preferences: {},
  personalization: {},
  modelProviders: [],
  search: { searxngURL: '' },
});

let config;
if (fs.existsSync(configPath)) {
  try {
    config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
  } catch (error) {
    fail(`读不了解析 ${configPath}：${error.message}`);
  }
} else {
  config = empty();
}

const provider = (id, init) => {
  let found = config.modelProviders.find((p) => p.id === id);
  if (!found) {
    found = { id, chatModels: [], embeddingModels: [], config: {}, hash: '', ...init };
    config.modelProviders.push(found);
  }
  return found;
};

const ensureModel = (models, key) => {
  if (!models.some((model) => model.key === key)) models.push({ name: key, key });
};

config.version ??= 1;
config.setupComplete = true;
config.search = { ...config.search };
// 内置的 SearXNG 在同一容器里；创建容器时恢复该地址，避免沿用外部实例的设置。
config.search.searxngURL = SEARXNG_URL;
config.modelProviders = Array.isArray(config.modelProviders) ? config.modelProviders : [];

// Vane 只给 api.openai.com 内置模型列表，自定义地址下列表完全来自这里；embedding 列表为空时搜索拿不到模型。
if (gatewayConnected) {
  const chat = provider(CHAT_PROVIDER_ID, { name: 'Wonton', type: 'openai' });
  chat.type = 'openai';
  chat.config = { ...chat.config, apiKey, baseURL };
  chat.chatModels = Array.isArray(chat.chatModels) ? chat.chatModels : [];
  ensureModel(chat.chatModels, chatModel);
} else {
  config.modelProviders = config.modelProviders.filter((p) => p.id !== CHAT_PROVIDER_ID);
}

const embedding = provider(EMBEDDING_PROVIDER_ID, { name: 'Wonton (local)', type: 'transformers' });
embedding.type = 'transformers';
embedding.hash = EMPTY_HASH;
// 应用启动时按环境补的这个 provider 会跟我们的重复，去掉它，模型选择才不会落到另一份上。
config.modelProviders = config.modelProviders.filter(
  (p) => p.id === EMBEDDING_PROVIDER_ID || p.hash !== EMPTY_HASH || p.type !== 'transformers',
);
embedding.embeddingModels = Array.isArray(embedding.embeddingModels) ? embedding.embeddingModels : [];
ensureModel(embedding.embeddingModels, EMBEDDING_MODEL);

const tmp = `${configPath}.tmp`;
fs.writeFileSync(tmp, `${JSON.stringify(config, null, 2)}\n`);
fs.renameSync(tmp, configPath);
console.log(
  gatewayConnected
    ? `已写入 ${configPath}：模型 ${chatModel}，本地 embedding ${EMBEDDING_MODEL}`
    : `已写入 ${configPath}：模型网关没接上，模型要用户在自己的设置里配，本地 embedding ${EMBEDDING_MODEL}`,
);
