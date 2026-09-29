#!/usr/bin/env python3
"""伪造的 OCS 宿主实例 —— `tools/tests/ocs_session_guard/` 的测试夹具（真宿主是 GUI，测试禁止上屏）。

描述符字段完全照 src/app/control/transport.rs:44 真格式写：
    {"protocol":1,"session_id":…,"pid":…,"port":…,"token":…,"executable":…}
协议也照 transport.rs：127.0.0.1、一条连接一条 JSON 请求、校验 token+session_id。

用于三种角色（都由 run_tests.py 驱动）：
  1) 被 tools/ocs_session.py 经 OCS_BIN 替身脚本（fixtures/wrap_ocs_bin.sh）当「自己启动的宿主」拉起
     （默认用本进程 pid 写描述符，于是归属校验能通过）
  2) 共享位置里的「别人的实例」（--dir 指共享 automation 目录）
  3) 装假：--pid-override（描述符里的 pid 对不上本进程）、--extra-sid/--extra-pid（多一个「别人的」会话）
"""
import argparse, json, os, socket, sys, threading, time

p = argparse.ArgumentParser()
p.add_argument("--dir")
p.add_argument("--sid")
p.add_argument("--pid-override", type=int, default=0)
p.add_argument("--extra-sid")
p.add_argument("--extra-pid", type=int, default=0)
p.add_argument("--log")
args, _unknown = p.parse_known_args()          # --new-instance 之类直接忽略

sid = os.environ.get("OCS_FAKE_SID") or args.sid or os.urandom(16).hex()
pid = int(os.environ.get("OCS_FAKE_PID_OVERRIDE") or args.pid_override or 0) or os.getpid()
extra_sid = os.environ.get("OCS_FAKE_EXTRA_SID") or args.extra_sid
extra_pid = int(os.environ.get("OCS_FAKE_EXTRA_PID") or args.extra_pid or 0)
log_path = os.environ.get("OCS_FAKE_LOG") or args.log
directory = args.dir or os.path.join(os.environ["XDG_CONFIG_HOME"], "OpenCADStudio", "automation")

logf = open(log_path, "a", buffering=1) if log_path else None


def note(**kw):
    if logf:
        logf.write(json.dumps(dict(kw, at=round(time.time(), 3)), ensure_ascii=False) + "\n")


def serve(session_id, desc_pid):
    listener = socket.socket()
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    listener.listen(16)
    port = listener.getsockname()[1]
    token = os.urandom(32).hex()
    os.makedirs(directory, exist_ok=True)
    os.chmod(directory, 0o700)
    path = os.path.join(directory, session_id + ".json")
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    os.write(fd, json.dumps({"protocol": 1, "session_id": session_id, "pid": desc_pid,
                             "port": port, "token": token,
                             "executable": os.path.realpath("/proc/self/exe")}).encode())
    os.close(fd)
    note(event="start", sid=session_id, desc_pid=desc_pid, real_pid=os.getpid(), port=port,
         xdg=os.environ.get("XDG_CONFIG_HOME"), plugins=os.environ.get("OCS_PLUGINS_DIR"),
         argv=sys.argv[1:], descriptor=path)
    while True:
        conn, _ = listener.accept()
        note(event="connect", sid=session_id)
        with conn:
            fh = conn.makefile("rb")
            try:
                req = json.loads(fh.readline())
            except Exception:
                req = None
            if not isinstance(req, dict) or req.get("token") != token \
                    or req.get("session_id") != session_id:
                note(event="reject", sid=session_id)
                continue
            op = req.get("op")
            note(event="request", sid=session_id, op=op,
                 request_id=req.get("request_id"), cmd=req.get("cmd"))
            if op == "hello":
                resp = {"ok": True, "session_id": session_id, "document_id": 1, "revision": 0,
                        "selection": [], "documents": [{"document_id": 1, "name": "fake.dwg"}]}
            elif op == "state":
                resp = {"ok": True, "session_id": session_id, "document_id": 1, "revision": 0,
                        "selection": [], "documents": []}
            elif op == "query":
                resp = {"ok": True, "entities": []}
            else:
                resp = {"ok": True, "code": 0, "status": "done",
                        "state": {"revision": 1, "document_id": 1, "selection": []}}
            conn.sendall((json.dumps(resp) + "\n").encode())
            # 真宿主写完一行就把整条连接关掉（src/app/control/transport.rs 的线程尾巴）
            # ⇒ 对端是 read-to-EOF，必须给出 EOF，否则被当成坏实例跳过
            conn.shutdown(socket.SHUT_WR)


if extra_sid:
    threading.Thread(target=serve, args=(extra_sid, extra_pid or os.getpid()), daemon=True).start()
try:
    serve(sid, pid)
except KeyboardInterrupt:
    pass
