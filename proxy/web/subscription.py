import hmac
import ipaddress
import json
import os
import socket
import struct
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs, urlsplit
from urllib.request import ProxyHandler, Request, build_opener

DATA = Path('/data')
URL_FILE = DATA / 'subscription.url'
CONFIG_FILE = DATA / 'config.json'
SECRET = os.environ['CONTROLLER_SECRET']
CONTROLLER = 'http://proxy:9090'
LOCK = threading.Lock()
DIRECT = build_opener(ProxyHandler({}))
MAX_BYTES = 4 * 1024 * 1024


def atomic_write(path, content):
    fd, name = tempfile.mkstemp(dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def docker_prefixes():
    prefixes = []
    for line in Path('/proc/net/route').read_text().splitlines()[1:]:
        fields = line.split()
        if fields[1] == '00000000':
            continue
        address, mask = (socket.inet_ntoa(struct.pack('<I', int(fields[i], 16))) for i in (1, 7))
        network = ipaddress.IPv4Network(f'{address}/{mask}')
        if not network.is_private:
            raise RuntimeError('Docker network must use a private IPv4 subnet')
        prefixes.append(str(network))
    if not prefixes:
        raise RuntimeError('Docker IPv4 subnet is unavailable')
    return prefixes + ['127.0.0.1/32', '::1/128']


def configuration(subscribed):
    config = {
        'mixed-port': 7890,
        'allow-lan': True,
        'bind-address': '*',
        'authentication': [f"{os.environ['LAN_USER']}:{os.environ['LAN_PASSWORD']}"],
        'skip-auth-prefixes': docker_prefixes(),
        'external-controller': '0.0.0.0:9090',
        'secret': SECRET,
        'mode': 'rule',
        'log-level': 'warning',
        'profile': {'store-selected': True},
        'proxy-groups': [{'name': 'PROXY', 'type': 'select', 'proxies': ['REJECT']}],
        'rules': ['MATCH,PROXY'],
    }
    if subscribed:
        config['proxy-providers'] = {'subscription': {
            'type': 'http',
            'url': 'http://subscription:8080/provider',
            # The provider transport must not enter its own empty PROXY group during bootstrap.
            'proxy': 'DIRECT',
            'path': './providers/subscription.yaml',
            'interval': 3600,
            'header': {'Authorization': [f'Bearer {SECRET}']},
        }}
        config['proxy-groups'] = [{
            'name': 'PROXY', 'type': 'select', 'use': ['subscription'], 'empty-fallback': 'REJECT',
        }]
    return config


def controller(path, method='GET', payload=None):
    headers = {'Authorization': f'Bearer {SECRET}'}
    data = None if payload is None else json.dumps(payload).encode()
    if data is not None:
        headers['Content-Type'] = 'application/json'
    request = Request(CONTROLLER + path, data=data, headers=headers, method=method)
    with DIRECT.open(request, timeout=45) as response:
        body = response.read(MAX_BYTES + 1)
        if len(body) > MAX_BYTES:
            raise ValueError('Controller response is too large')
        return json.loads(body) if body else None


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass  # Request paths, remote errors and form bodies can contain subscription credentials.

    def reply(self, status, body, content_type='application/json', reason=None):
        if isinstance(body, dict):
            body = json.dumps(body, ensure_ascii=False).encode()
        self.send_response(status, reason)
        self.send_header('Content-Type', content_type)
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Cache-Control', 'no-store')
        self.end_headers()
        self.wfile.write(body)

    def authorized(self):
        return hmac.compare_digest(self.headers.get('Authorization', ''), f'Bearer {SECRET}')

    def do_GET(self):
        if self.path == '/healthz':
            self.reply(200 if CONFIG_FILE.is_file() else 503, {'ready': CONFIG_FILE.is_file()})
            return
        if not self.authorized():
            self.reply(403, {'error': 'Gateway credentials required'})
            return
        if self.path == '/subscription/':
            self.reply(200, Path('/wonton/subscription.html').read_bytes(), 'text/html; charset=utf-8')
        elif self.path == '/subscription/status':
            try:
                proxies = controller('/providers/proxies')['providers'].get('subscription')
                self.reply(200, {'configured': URL_FILE.is_file(), 'nodes': len(proxies['proxies']) if proxies else 0})
            except (OSError, ValueError, KeyError, TypeError):
                self.reply(503, {'error': 'Proxy controller unavailable'})
        elif self.path == '/provider':
            self.provider()
        else:
            self.reply(404, {'error': 'Not found'})

    def provider(self):
        try:
            if not URL_FILE.is_file():
                self.reply(409, {'error': 'Subscription is not configured'})
                return
            # Subscription servers use this protocol identifier to return Clash-compatible YAML.
            request = Request(URL_FILE.read_text(), headers={'User-Agent': 'clash'})
            with build_opener().open(request, timeout=30) as response:
                body = response.read(MAX_BYTES + 1)
                if len(body) > MAX_BYTES:
                    self.reply(502, {'error': 'Subscription exceeds 4 MiB'}, reason='Subscription exceeds 4 MiB')
                    return
                self.reply(200, body, 'application/yaml')
        except HTTPError as error:
            message = f'Subscription server returned HTTP {error.code}'
            self.reply(502, {'error': message}, reason=message)
        except (OSError, ValueError, URLError):
            self.reply(502, {'error': 'Subscription transport unavailable'}, reason='Subscription transport unavailable')

    def do_POST(self):
        if self.path != '/subscription/save':
            self.reply(404, {'error': 'Not found'})
            return
        if not self.authorized():
            self.reply(403, {'error': 'Gateway credentials required'})
            return
        if self.headers.get('Origin') != os.environ['APP_URL']:
            self.reply(403, {'error': 'Same-origin request required'})
            return
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if not 0 < length <= 8192:
                raise ValueError()
            url = parse_qs(self.rfile.read(length).decode(), strict_parsing=True)['url'][0]
            parts = urlsplit(url)
            if parts.scheme not in ('http', 'https') or not parts.hostname or any(c.isspace() for c in url):
                raise ValueError()
        except (KeyError, ValueError, UnicodeError):
            self.reply(400, {'error': 'Enter an HTTP or HTTPS subscription URL'})
            return
        if not LOCK.acquire(blocking=False):
            self.reply(409, {'error': 'Subscription update already running'})
            return
        try:
            atomic_write(URL_FILE, url.encode())
            if 'subscription' not in controller('/providers/proxies')['providers']:
                atomic_write(CONFIG_FILE, json.dumps(configuration(True)).encode())
                controller('/configs', 'PUT', {'path': '/data/config.json'})
            controller('/providers/proxies/subscription', 'PUT')
            provider = controller('/providers/proxies/subscription')
            if not provider['proxies']:
                self.reply(422, {'error': 'Subscription has no nodes'})
            else:
                self.reply(200, {'saved': True, 'nodes': len(provider['proxies'])})
        except HTTPError as error:
            # Only our own fixed reason phrases cross this boundary; never echo upstream YAML errors.
            message = 'Proxy rejected the subscription format; URL saved'
            try:
                detail = json.loads(error.read(MAX_BYTES)).get('message', '')
                if detail.startswith('502 Subscription '):
                    message = detail.removeprefix('502 ') + '; URL saved'
            except (ValueError, OSError):
                message = f'Proxy controller returned HTTP {error.code}; URL saved'
            self.reply(502, {'error': message})
        except (OSError, ValueError, KeyError, TypeError):
            self.reply(503, {'error': 'Proxy controller unavailable; URL saved, update not confirmed'})
        finally:
            LOCK.release()


os.umask(0o077)
if len(SECRET) < 32:
    raise RuntimeError('Controller secret must contain at least 16 random bytes')
if len(sys.argv) > 1 and sys.argv[1] == 'init':
    atomic_write(CONFIG_FILE, json.dumps(configuration(URL_FILE.is_file())).encode())
else:
    if not CONFIG_FILE.is_file():
        raise RuntimeError('Proxy configuration is missing')
    ThreadingHTTPServer(('0.0.0.0', 8080), Handler).serve_forever()
