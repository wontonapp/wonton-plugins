#!/bin/sh
set -eu

# 镜像里的 SearXNG 设置带着一个公开的 secret_key，SearXNG 用它签 session；换成核心生成的。
# 启用默认关闭的 Bing、Yahoo、Yandex：Google、DDG、Startpage 常被 CAPTCHA/限流，经验证这套组合经出口代理仍可用。
# 两项都写进 /etc/searxng/settings.yml，落在容器的可写层里。
/usr/local/searxng/searx-pyenv/bin/python - <<'PY'
import os, yaml

path = '/etc/searxng/settings.yml'
settings = yaml.safe_load(open(path))
settings.setdefault('server', {})['secret_key'] = os.environ['SEARXNG_SECRET']
# SearXNG 不读 HTTP(S)_PROXY，用自己被代理设置；地址由核心注入，没配代理时为空，保持直连。
proxy = os.environ.get('WONTON_PROXY_URL')
if proxy:
    settings.setdefault('outgoing', {})['proxies'] = {'all://': proxy}
engines = settings.setdefault('engines', [])
for name in ('bing', 'yahoo', 'yandex'):
    if not any(engine.get('name') == name for engine in engines if isinstance(engine, dict)):
        engines.append({'name': name, 'disabled': False})
yaml.safe_dump(settings, open(path, 'w'))
PY

# 补丁原因与接受范围：https://github.com/geraldpeng6/wonton-plugin-vane/pull/2#issuecomment-6602296059
# Vane 1.12.2 只保护首个工具分片，后续分片的参数仍为空时 partial-json 的 parse("") 会抛 "is empty"。
# 给后续分片补默认空对象；两个分支都必须受保护，缺失或重复打补丁时要能识别。
for chunk in /home/vane/.next/server/chunks/136.js /home/vane/.next/server/chunks/68.js; do
  sed -i 's/parse)(b\.arguments)/parse)(b.arguments||"{}")/g' "$chunk"
  if grep -Fq 'parse)(b.arguments)' "$chunk" ||
    [ "$(grep -Fo 'parse)(b.arguments||"{}")' "$chunk" | wc -l)" -ne 2 ]; then
    echo "工具参数的补丁没打上：$chunk" >&2
    exit 1
  fi
done

# 上游先列内置模型，再列 seed 的自定义模型；只写 seed 不能改变网页首次选择的英文模型。
# 将默认列表首项的名称和模型键一起换成多语种模型，不修改相关度阈值。
for chunk in /home/vane/.next/server/chunks/136.js /home/vane/.next/server/chunks/68.js; do
  sed -i 's#name:"all-MiniLM-L6-v2",key:"Xenova/all-MiniLM-L6-v2"#name:"paraphrase-multilingual-MiniLM-L12-v2",key:"Xenova/paraphrase-multilingual-MiniLM-L12-v2"#g' "$chunk"
  if grep -Fq 'Xenova/all-MiniLM-L6-v2' "$chunk" ||
    [ "$(grep -Fo 'name:"paraphrase-multilingual-MiniLM-L12-v2",key:"Xenova/paraphrase-multilingual-MiniLM-L12-v2"' "$chunk" | wc -l)" -ne 1 ]; then
    echo "embedding 模型的替换没打上：$chunk" >&2
    exit 1
  fi
done

# 搜索必须传播实际所选 embedding 的错误；默认模型预热只负责冷启动，不代替运行时检查。
node /seed/patch-search.cjs

node --input-type=module - <<'JS'
import { pipeline } from '@huggingface/transformers';

try {
  await pipeline('feature-extraction', 'Xenova/paraphrase-multilingual-MiniLM-L12-v2', { dtype: 'fp32' });
  console.log('本地 embedding 模型已就绪');
} catch (error) {
  console.error('本地 embedding 模型不可用，请检查模型缓存和宿主出网连接：', error);
  process.exit(1);
}
JS

exec /home/vane/entrypoint.sh
