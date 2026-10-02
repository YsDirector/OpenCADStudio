#!/usr/bin/env python3
"""真·无头 OCS 宿主桥 —— `tools/tests/ocs_session_open_e2e.sh` 的 OCS_BIN 夹具。

为什么需要它：`tools/ocs_session.py` 隔离模式下自己拉起的宿主是 `OpenCADStudio --new-instance`，
那是 **GUI**（会开窗上屏，测试禁止）。本桥把它替身成真二进制的无头 `--serve --port`（真宿主、
真控制协议、真 MCP 链路），再写一份符合 `src/app/control/transport.rs:44` 的描述符 + 一条 TCP
中转，让 MCP 的描述符发现 / token 握手 / 请求-应答全部走真代码，只有传输层过了这层中转。

本进程由 `ocs_session.py` 当作「自己启动的宿主」spawn（经 `wrap_ocs_bin.sh`），所以描述符里的
pid 写本进程 pid —— `ocs_session.py` 的归属校验（pid 必须是它 spawn 的子进程）才会通过。
不发 GUI、不碰用户实例；`XDG_CONFIG_HOME` 由 `ocs_session.py` 指到私有隔离根。

环境变量：
    OCS_REAL              真二进制路径（必填；由 ocs_session_open_e2e.sh 导出）
    OCS_E2E_SERVE_LOG     `--serve` 的日志（缺省放 XDG_CONFIG_HOME 里，随临时目录一起删）
    OCS_E2E_SERVE_TIMEOUT `--serve` 起不来的等待上限秒数（缺省 90，仅调试用）
"""
import json, os, signal, socket, subprocess, sys, threading, time

REAL = os.environ.get("OCS_REAL")
if not REAL:
    sys.stderr.write("headless_host: OCS_REAL 未设置\n")
    sys.exit(2)
XDG = os.environ["XDG_CONFIG_HOME"]
AUTO = os.path.join(XDG, "OpenCADStudio", "automation")
LOG = os.environ.get("OCS_E2E_SERVE_LOG") or os.path.join(XDG, "headless-serve.log")
SERVE_TIMEOUT = float(os.environ.get("OCS_E2E_SERVE_TIMEOUT", "90"))


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


serve_port = free_port()
log = open(LOG, "ab", buffering=1)
child = subprocess.Popen([REAL, "--serve", "--port", str(serve_port)],
                         stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                         env=os.environ.copy())

desc_path = None


def bye(*_args):
    """退出/被杀时收拾自己的东西：--serve 子进程 + 描述符。"""
    if child.poll() is None:
        try:
            child.terminate()
            try:
                child.wait(timeout=8)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=5)
        except OSError:
            pass
    if desc_path:
        try:
            os.remove(desc_path)
        except OSError:
            pass
    sys.exit(0)


signal.signal(signal.SIGTERM, bye)
signal.signal(signal.SIGINT, bye)

# 等 --serve 能收连接：首行 ready 横幅里带宿主真实 session_id，描述符必须用它
# （src/app/control/mod.rs:522-531 会校验 session_id）。
session_id = None
deadline = time.time() + SERVE_TIMEOUT
while not session_id:
    if child.poll() is not None:
        sys.stderr.write("headless_host: %s --serve 退出码 %s（日志 %s）\n" % (REAL, child.returncode, LOG))
        sys.exit(3)
    try:
        s = socket.create_connection(("127.0.0.1", serve_port), timeout=1)
        banner = s.makefile("rb").readline()
        s.close()
        session_id = (json.loads(banner.decode("utf-8", "replace")).get("session_id") or None)
    except (OSError, ValueError):
        if time.time() > deadline:
            sys.stderr.write("headless_host: --serve 在 %.0fs 内没起来（日志 %s）\n" % (SERVE_TIMEOUT, LOG))
            sys.exit(3)
        time.sleep(0.2)

# 先 bind+listen，再写描述符 —— 保证 MCP 一发现描述符就能连上（连接进 backlog 等着即可）。
listener = socket.socket()
listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
listener.bind(("127.0.0.1", 0))
listener.listen(16)
port = listener.getsockname()[1]
token = os.urandom(32).hex()

os.makedirs(AUTO, exist_ok=True)
os.chmod(AUTO, 0o700)
desc_path = os.path.join(AUTO, session_id + ".json")
with open(desc_path, "w") as fh:
    fh.write(json.dumps({"protocol": 1, "session_id": session_id, "pid": os.getpid(), "port": port,
                         "token": token, "executable": os.path.realpath("/proc/self/exe")}))
os.chmod(desc_path, 0o600)


def one_request(conn):
    """中转：MCP 一条连接一条请求（响应读到 EOF）→ 转给 --serve 一条连接一行。"""
    try:
        line = conn.makefile("rb").readline()
        if not line:
            return
        up = socket.create_connection(("127.0.0.1", serve_port), timeout=60)
        upf = up.makefile("rb")
        upf.readline()                      # --serve 的 ready 横幅
        up.sendall(line)
        resp = upf.readline()
        up.close()
        conn.sendall(resp)
    except OSError as error:
        sys.stderr.write("headless_host: relay error %s\n" % error)
    finally:
        try:
            conn.shutdown(socket.SHUT_WR)
        except OSError:
            pass
        conn.close()


while True:
    conn, _ = listener.accept()
    threading.Thread(target=one_request, args=(conn,), daemon=True).start()
