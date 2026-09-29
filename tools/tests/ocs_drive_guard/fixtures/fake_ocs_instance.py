#!/usr/bin/env python3
"""自家无头「OCS 实例」夹具（真宿主是 GUI，本测试禁止上屏）。

描述符字段与传输协议照 src/app/control/transport.rs:44 写：
    {"protocol":1,"session_id":…,"pid":…,"port":…,"token":…,"executable":…}
127.0.0.1、一条连接一条 JSON 请求、校验 token+session_id、答完即关写端（对端是 read-to-EOF）。

两种用法（都由 run_tests.py 驱动）：
  1) 被 tools/ocs_session.py 经 OCS_BIN 替身（fixtures/wrap_ocs_bin.sh）以 --new-instance 拉起
     —— 这就是「自家无头实例」；pid 用本进程 pid，会话工具的身份校验才能通过。
  2) run_tests.py 直接 Popen 起第二个实例（--sid/--dir/--log 显式给），用来验证 --session 只驱动一个。

每个 connect / request 都写进 OCS_FAKE_LOG（JSON 行），测试据此算「有没有被连上」。
"""
import argparse, json, os, socket, sys, time

p = argparse.ArgumentParser()
p.add_argument("--dir")
p.add_argument("--sid")
p.add_argument("--log")
args, _unknown = p.parse_known_args()          # --new-instance 之类直接忽略

sid = os.environ.get("OCS_FAKE_SID") or args.sid or os.urandom(16).hex()
log_path = os.environ.get("OCS_FAKE_LOG") or args.log
directory = args.dir or os.path.join(os.environ["XDG_CONFIG_HOME"], "OpenCADStudio", "automation")
logf = open(log_path, "a", buffering=1) if log_path else None


def note(**kw):
    if logf:
        logf.write(json.dumps(dict(kw, at=round(time.time(), 3)), ensure_ascii=False) + "\n")


listener = socket.socket()
listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
listener.bind(("127.0.0.1", 0))
listener.listen(16)
port = listener.getsockname()[1]
token = os.urandom(32).hex()
os.makedirs(directory, exist_ok=True)
os.chmod(directory, 0o700)
path = os.path.join(directory, sid + ".json")
fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
os.write(fd, json.dumps({"protocol": 1, "session_id": sid, "pid": os.getpid(), "port": port,
                         "token": token, "executable": os.path.realpath("/proc/self/exe")}).encode())
os.close(fd)
note(event="start", sid=sid, pid=os.getpid(), port=port, xdg=os.environ.get("XDG_CONFIG_HOME"),
     descriptor=path, argv=sys.argv[1:])

while True:
    conn, _ = listener.accept()
    note(event="connect", sid=sid)
    with conn:
        fh = conn.makefile("rb")
        try:
            req = json.loads(fh.readline())
        except Exception:
            req = None
        if not isinstance(req, dict) or req.get("token") != token or req.get("session_id") != sid:
            note(event="reject", sid=sid)
            continue
        op = req.get("op")
        note(event="request", sid=sid, op=op, request_id=req.get("request_id"), cmd=req.get("cmd"))
        if op == "hello":
            resp = {"ok": True, "session_id": sid, "document_id": 1, "revision": 0,
                    "selection": [], "documents": [{"document_id": 1, "name": "fake.dwg"}]}
        elif op == "state":
            resp = {"ok": True, "session_id": sid, "mode": "gui", "enabled": True, "revision": 0,
                    "document_id": 1, "selection": [],
                    "documents": [{"id": 1, "title": "fake.dwg", "path": "/tmp/fake.dwg",
                                   "start": False}]}
        elif op == "query":
            resp = {"ok": True, "entities": []}
        else:
            resp = {"ok": True, "code": 0, "status": "done",
                    "state": {"revision": 1, "document_id": 1, "selection": []}}
        conn.sendall((json.dumps(resp) + "\n").encode())
        conn.shutdown(socket.SHUT_WR)
