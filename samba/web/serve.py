# 连接信息页：stdlib http.server，无框架。
# 地址取注入的 WONTON_LAN_ADDRESSES；密码读核心写进容器的 /run/wonton/samba-pass。
# 页面上有 SMB 密码，而插件网络不是信任边界（别的出网插件经用户代理也能连到这个端口），
# 所以只认网关代登录带来的 X-Wonton-Web-Key：没有这个头或值不对就 403，/healthz 除外。
# 取不到值时如实显示原因，不假装正常。
import hmac
import html
import os
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = 8080
SHARES = ["photos", "documents", "music", "downloads", "transfers"]
PASS_PATH = "/run/wonton/samba-pass"
KEY_ENV = "WONTON_WEB_KEY"
KEY_HEADER = "X-Wonton-Web-Key"
WEB_KEY = os.environ.get(KEY_ENV, "")

PAGE = """<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Samba</title>
<style>
  :root { color-scheme: light dark; }
  body {
    margin: 0; padding: 16px;
    font: 17px/1.45 system-ui, -apple-system, "PingFang SC", "Noto Sans CJK SC", sans-serif;
    background: var(--bg); color: var(--fg);
    --bg: #f5f5f7; --fg: #1d1d1f; --card: #ffffff; --line: #d2d2d7; --accent: #0a84ff; --muted: #6e6e73; --err: #c62b28;
  }
  @media (prefers-color-scheme: dark) {
    body { --bg: #000; --fg: #f5f5f7; --card: #1c1c1e; --line: #38383a; --accent: #0a84ff; --muted: #98989d; --err: #ff6961; }
  }
  main { max-width: 26em; margin: 0 auto; }
  h2 { font-size: 15px; font-weight: 500; color: var(--muted); margin: 20px 4px 6px; overflow-wrap: anywhere; }
  .card { background: var(--card); border-radius: 12px; overflow: hidden; }
  .row {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    padding: 11px 14px; min-height: 28px;
  }
  .row + .row { border-top: 1px solid var(--line); }
  .name { font-weight: 600; white-space: nowrap; }
  .col { display: flex; flex-direction: column; align-items: flex-end; min-width: 0; }
  .col > * { max-width: 100%; overflow-wrap: anywhere; text-align: right; }
  .smb { color: var(--accent); text-decoration: none; font-size: 15px; }
  button {
    font: 15px/1 inherit; color: var(--accent); background: none; border: 0; padding: 4px 2px;
  }
  .mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 14px; word-break: break-all; }
  .err { color: var(--err); padding: 11px 14px; }
</style>
</head>
<body>
<main>
@BODY@
<p id="copy-error" class="err" role="alert" hidden></p>
</main>
<script>
async function copy(text, el) {
  const error = document.getElementById("copy-error");
  error.hidden = true;
  try {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const t = document.createElement("textarea");
      t.value = text; document.body.appendChild(t); t.select();
      try {
        if (!document.execCommand("copy")) throw new Error("copy failed");
      } finally {
        t.remove();
      }
    }
  } catch {
    error.textContent = "复制失败，请长按内容手动复制。";
    error.hidden = false;
    error.scrollIntoView({ block: "nearest" });
    return;
  }
  const old = el.textContent;
  el.textContent = "已复制";
  setTimeout(() => { el.textContent = old; }, 900);
}
</script>
</body>
</html>"""


def uri_host(addr):
    # URI 里的 IPv6 字面量要加方括号。
    return f"[{addr}]" if ":" in addr else addr


def unc_host(addr):
    # Windows 的 IPv6 字面量：冒号换连字符，加 .ipv6-literal.net。
    return addr.replace(":", "-") + ".ipv6-literal.net" if ":" in addr else addr


def rows():
    addrs = [a.strip() for a in os.environ.get("WONTON_LAN_ADDRESSES", "").split(",") if a.strip()]
    owner = os.environ.get("WONTON_OWNER_NAME")
    try:
        password = open(PASS_PATH).read().strip()
        if not password:
            raise ValueError("empty")
        pass_err = None
    except (OSError, ValueError) as e:
        pass_err = f"{type(e).__name__}: {e}"
        password = None

    out = []
    if not addrs:
        out.append('<div class="card"><div class="err">核心没有注入局域网地址'
                   '（WONTON_LAN_ADDRESSES 为空），不知道把端口开在了哪里。</div></div>')
    else:
        for addr in addrs:
            items = []
            uhost = html.escape(uri_host(addr))
            for s in SHARES:
                unc = html.escape(f"\\\\{unc_host(addr)}\\{s}")
                items.append(
                    f'<div class="row"><span class="name">{html.escape(s)}</span>'
                    f'<span class="col">'
                    f'<a class="smb" href="smb://{uhost}/{s}">smb://{uhost}/{s}</a>'
                    f'<button data-copy="{unc}" onclick="copy(this.dataset.copy,this)">{unc}</button>'
                    f"</span></div>"
                )
            out.append(f"<h2>{html.escape(addr)}</h2><div class=\"card\">{''.join(items)}</div>")

    acct = ""
    if not owner:
        acct += '<div class="err">核心没有注入用户名（WONTON_OWNER_NAME 为空）。</div>'
    else:
        acct += (f'<div class="row"><span class="name">用户名</span><span class="col">'
                 f'<span class="mono">{html.escape(owner)}</span>'
                 f'<button data-copy="{html.escape(owner, quote=True)}" onclick="copy(this.dataset.copy,this)">复制</button></span></div>')
    if pass_err is not None:
        acct += f'<div class="err">读不到密码文件：{html.escape(pass_err)}</div>'
    else:
        acct += (f'<div class="row"><span class="name">密码</span><span class="col">'
                 f'<span class="mono">{html.escape(password)}</span>'
                 f'<button data-copy="{html.escape(password, quote=True)}" onclick="copy(this.dataset.copy,this)">复制</button></span></div>')
    out.append(f'<h2>账号</h2><div class="card">{acct}</div>')
    return "".join(out)


class Handler(BaseHTTPRequestHandler):
    # 网关和核心的探测都是 HTTP/1.1，默认的 HTTP/1.0 会让它们按连接关闭断开。
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        if self.path == "/healthz":
            self.send_response(204)
            self.end_headers()
            return
        # 头名不区分大小写；比较用 compare_digest，不靠异常路径。
        supplied = self.headers.get(KEY_HEADER, "")
        if not WEB_KEY or not hmac.compare_digest(supplied, WEB_KEY):
            self.send_error(403)
            return
        if self.path in ("/", ""):
            body = PAGE.replace("@BODY@", rows()).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self.send_error(404)

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    # 没有密钥就没有能核对的凭据，起来也只会放错人：拒绝启动。
    if not WEB_KEY:
        print(f"环境变量 {KEY_ENV} 为空：网页密钥没有注入，拒绝启动", file=sys.stderr)
        raise SystemExit(1)
    server = ThreadingHTTPServer(("0.0.0.0", PORT), Handler)
    server.daemon_threads = True
    server.serve_forever()
