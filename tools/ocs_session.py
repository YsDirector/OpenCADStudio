#!/usr/bin/env python3
"""OCS MCP 单会话驱动：**在一个 MCP 客户端里**跑交互命令的 start+input。

为什么必须单会话：宿主 `src/app/control/mod.rs` 的自动化门要求
`control.owner == Some((tab, client_id))` 才允许 `input`，而 MCP 服务器给**每个客户端实例**
随机生成 `client_id`（`src/mcp.rs:326`）。用 `mcporter call ...` 这类"一次调用一个新连接"的方式驱动，
每次 client_id 都不同 → 交互命令的下一步永远返回 `command_busy`。
本脚本开一个 `OpenCADStudio --mcp` 子进程并保持它，因此 `run/start` 之后的 `input` 能通过。
（另一个坑：插件命令要在 `on_text_input` 里返回 `CommandStep::Ignored`，宿主才会把非关键字
token 当十六进制句柄喂进实体拾取步骤 —— 那种写法可以直接 `run "ZX 62"` 一把过。）

用法：
    python3 /tmp/ocs_mcp.py <<'PY'
    newdoc
    run CENTERMARK
    input entity DD 315,350,0
    cancel
    run D2G
    query line 3中心线层
    PY

安全边界（默认只连「自己启动的那一个实例」）：
    ★ 实例发现根 = `config_dir()/automation` —— `src/config.rs:28`（Linux 取 $XDG_CONFIG_HOME，
      缺省 $HOME/.config）→ `src/mcp.rs:267-269` 读该目录；宿主写描述符见
      `src/app/control/transport.rs:32-57`（字段 session_id / pid / port / token / executable）。
      发现链路没有其它外部状态：socket 是 127.0.0.1 上的随机端口；端口/锁只用于双击转发
      （`src/io/single_instance.rs:110-121`），而宿主带 `--new-instance` 直接跳过它。
    ★ 所以默认把 $XDG_CONFIG_HOME 指到私有临时目录（0700），宿主与 MCP 子进程都在那里互相发现，
      绝不碰 ~/.config/OpenCADStudio/automation/ 里别人的实例 —— 用户正在画图时也不会被打进他的图。
    ★ 启动后校验归属：描述符里的 pid 必须属于本脚本 spawn 的子进程；不符即报错退出，
      绝不「降级」去用别人的 session。退出时杀掉自己启动的宿主（不留常驻进程）。

环境变量：
    OCS_BIN=<路径>               宿主二进制（默认按仓库根/target/release 推导）
    OCS_ALLOW_EXISTING=1         ★ 逃生态：不隔离，允许连「现有实例」（含用户正在用的 GUI）。
                                 风险：run/input/save/capture 会直接打进那张正在编辑的图，
                                 仅在你确认目标实例时使用；默认关。
    OCS_SESSION_STATE_DIR=<目录>  指定私有状态目录（默认 mktemp -d；调试/测试用）
    OCS_KEEP_HOST=1              退出时保留自己启动的宿主（默认 SIGTERM 掉）
"""
import atexit, glob, json, os, queue, shutil, subprocess, sys, tempfile, threading, time

if any(arg in ("-h", "--help") for arg in sys.argv[1:]):
    print(__doc__)          # 含逃生态风险说明；不建目录、不起进程
    raise SystemExit(0)

# 宿主二进制：优先 $OCS_BIN，否则按「本脚本所在仓库根/target/release」推导
_OCS_DEFAULT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                            "target", "release", "OpenCADStudio")
OCS = os.environ.get("OCS_BIN", _OCS_DEFAULT)
ENV = dict(os.environ)
ENV.setdefault("WAYLAND_DISPLAY", "wayland-0")
ENV.setdefault("XDG_RUNTIME_DIR", "/run/user/1000")


def _flag(name):
    return os.environ.get(name, "").strip().lower() in ("1", "true", "yes", "on")


ALLOW_EXISTING = _flag("OCS_ALLOW_EXISTING")   # 逃生态，见文件头
KEEP_HOST = _flag("OCS_KEEP_HOST")

# ── 只认自己启动的实例：把实例发现根指到私有目录 ──────────────────────────────
_STATE_DIR_OVERRIDE = os.environ.get("OCS_SESSION_STATE_DIR") or None
STATE_DIR = None
_STATE_DIR_OWNED = False
if not ALLOW_EXISTING:
    STATE_DIR = _STATE_DIR_OVERRIDE or tempfile.mkdtemp(prefix="ocs-session-state-")
    _STATE_DIR_OWNED = _STATE_DIR_OVERRIDE is None
    os.makedirs(STATE_DIR, exist_ok=True)
    os.chmod(STATE_DIR, 0o700)
    ENV["XDG_CONFIG_HOME"] = STATE_DIR       # ← 唯一的发现开关：src/config.rs:28 + src/mcp.rs:267
    if "OCS_PLUGINS_DIR" not in ENV:
        # 插件装在 $XDG_CONFIG_HOME/OpenCADStudio/plugins（src/plugin/external.rs:141-160）：
        # 隔离会把它一起挡掉，默认模式下 OCSM 命令就没了 ⇒ 显式指回用户真实的插件目录。
        _real_plugins = os.path.join(
            os.environ.get("XDG_CONFIG_HOME") or os.path.join(os.path.expanduser("~"), ".config"),
            "OpenCADStudio", "plugins")
        if os.path.isdir(_real_plugins):
            ENV["OCS_PLUGINS_DIR"] = _real_plugins


def state_automation_dir(state_dir):
    """私有状态目录里的实例发现目录（与 config_dir()/automation 同构）。"""
    return os.path.join(state_dir, "OpenCADStudio", "automation")


def read_state_descriptors(state_dir):
    """读私有目录里的实例描述符（格式见 src/app/control/transport.rs:44）。"""
    found = {}
    for path in sorted(glob.glob(os.path.join(state_automation_dir(state_dir), "*.json"))):
        try:
            with open(path, "r", encoding="utf-8") as fh:
                desc = json.load(fh)
        except (OSError, ValueError):
            continue
        if isinstance(desc, dict) and isinstance(desc.get("session_id"), str):
            desc["_path"] = path
            found[desc["session_id"]] = desc
    return found


def parent_pid(pid):
    """/proc 优先、ps 兜底；拿不到返回 None（= 归属无法判定）。"""
    try:
        with open("/proc/%d/stat" % pid, "rb") as fh:
            return int(fh.read().rsplit(b")", 1)[1].split()[1])
    except (OSError, IndexError, ValueError):
        pass
    try:
        out = subprocess.run(["ps", "-o", "ppid=", "-p", str(pid)],
                             capture_output=True, text=True, timeout=5)
        return int(out.stdout.strip())
    except (OSError, ValueError, subprocess.SubprocessError):
        return None


def owned_by(pid, ancestor):
    """pid == ancestor 或 ancestor 在其父链上 → True；False = 判定为外人；None = 判定不了。"""
    seen = set()
    cur = pid
    while cur and cur > 1 and cur not in seen:
        if cur == ancestor:
            return True
        seen.add(cur)
        cur = parent_pid(cur)
        if cur is None:
            return None
    return False


def pid_alive(pid):
    """Linux 读 /proc，其它平台用 ps；判定不了就当「活」。

    不能用 os.kill(pid, 0)：Windows 上它等价 TerminateProcess —— 会把目标进程真杀掉。
    """
    if os.path.isdir("/proc"):
        return os.path.exists("/proc/%d" % pid)
    try:
        out = subprocess.run(["ps", "-o", "pid=", "-p", str(pid)],
                             capture_output=True, text=True, timeout=5)
        if out.returncode == 0:
            return bool(out.stdout.strip())
    except (OSError, subprocess.SubprocessError):
        pass
    return True      # 判定不了 ⇒ 按「活」处理（落在报错一侧，安全）


FOREIGN_GRACE = 3.0   # 私有目录里冒出别人的活实例后，给自家宿主多久写出描述符再报错


HOST = None        # 自己启动的宿主进程（隔离模式下才有）
HOST_DESC = None   # 它写出来的描述符


def start_owned_host(timeout=45.0):
    """自己起宿主（参数与 src/mcp.rs:start_gui 一致），只认「它自己」写的那份描述符。"""
    global HOST, HOST_DESC
    directory = state_automation_dir(STATE_DIR)
    os.makedirs(directory, exist_ok=True)
    os.chmod(directory, 0o700)
    log_path = os.path.join(directory, "gui.log")
    log = open(log_path, "ab")
    HOST = subprocess.Popen([OCS, "--new-instance"], stdin=subprocess.DEVNULL, stdout=log,
                            stderr=log, env=ENV)
    deadline = time.time() + timeout
    foreign, foreign_at = [], None
    while time.time() < deadline:
        if HOST.poll() is not None:
            raise SystemExit("# 宿主启动失败：%s --new-instance 退出码 %s（日志 %s）"
                             % (OCS, HOST.returncode, log_path))
        found = read_state_descriptors(STATE_DIR)
        for desc in found.values():
            try:
                pid = int(desc.get("pid"))
            except (TypeError, ValueError):
                continue
            if owned_by(pid, HOST.pid) is True:
                HOST_DESC = desc
                return
        # 私有目录里冒出「别人的活实例」：宽限 FOREIGN_GRACE 秒等自家宿主写描述符，之后报错
        # （不等满 45s 超时，更不降级去用别人的会话）
        outsiders = [(d.get("session_id"), d.get("pid")) for d in found.values()
                     if isinstance(d.get("pid"), int) and pid_alive(d["pid"])]
        if outsiders:
            if foreign_at is None:
                foreign, foreign_at = outsiders, time.time()
            elif time.time() - foreign_at > FOREIGN_GRACE:
                raise SystemExit(
                    "# 实例归属校验失败：私有目录 %s 里出现了不属于本脚本（宿主 pid=%s）的活会话 %s；"
                    "拒绝连接（绝不降级去用别人的实例）。确认目标后可用 OCS_ALLOW_EXISTING=1 显式连现有实例。"
                    % (STATE_DIR, HOST.pid, "、".join("%s(pid=%s)" % f for f in foreign)))
        time.sleep(0.2)
    raise SystemExit("# 宿主启动超时：%ss 内没在 %s 写出自己的描述符（日志 %s）"
                     % (timeout, directory, log_path))


def cleanup():
    """退出时只收拾自己的东西：自己启动的宿主、MCP 子进程、自己的临时目录。"""
    if HOST is not None and not KEEP_HOST:
        try:
            HOST.terminate()
            try:
                HOST.wait(timeout=5)
            except subprocess.TimeoutExpired:
                HOST.kill()
        except OSError:
            pass
    mcp = globals().get("proc")
    if mcp is not None:
        try:
            mcp.terminate()
        except OSError:
            pass
    if _STATE_DIR_OWNED and STATE_DIR:
        shutil.rmtree(STATE_DIR, ignore_errors=True)


atexit.register(cleanup)

if STATE_DIR is not None:
    print("# 隔离模式：只认自己启动的实例（state_dir=%s plugins=%s；要连现有实例用 OCS_ALLOW_EXISTING=1）"
          % (STATE_DIR, ENV.get("OCS_PLUGINS_DIR", "-")))
    start_owned_host()
else:
    print("# ★ 逃生态 OCS_ALLOW_EXISTING=1：允许连现有实例，写操作可能直接打进用户正在编辑的图纸",
          file=sys.stderr)

proc = subprocess.Popen([OCS, "--mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                        stderr=subprocess.DEVNULL, text=True, bufsize=1, env=ENV)
q = queue.Queue()
def reader():
    for line in proc.stdout:
        q.put(line.strip())
threading.Thread(target=reader, daemon=True).start()

_id = [int(__import__('time').time()) % 1_000_000]  # 跨进程唯一，避免 request_id_reused
def send(method, params=None, notify=False):
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        msg["params"] = params
    if not notify:
        _id[0] += 1
        msg["id"] = _id[0]
    proc.stdin.write(json.dumps(msg) + "\n")
    proc.stdin.flush()
    if notify:
        return None
    want = msg["id"]
    deadline = time.time() + 60
    while time.time() < deadline:
        try:
            line = q.get(timeout=max(0.1, deadline - time.time()))
        except queue.Empty:
            break
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        if obj.get("id") == want:
            return obj
    return {"error": {"message": "timeout"}}

def call_tool(name, args):
    r = send("tools/call", {"name": name, "arguments": args})
    if "error" in r:
        raise SystemExit(f"{name} 失败：{r['error']}")
    # MCP 返回 content[0].text = JSON 字符串
    content = r.get("result", {}).get("content") or []
    if content and content[0].get("type") == "text":
        try:
            return json.loads(content[0]["text"])
        except json.JSONDecodeError:
            return content[0]["text"]
    return r.get("result")

send("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                    "clientInfo": {"name": "ocs-session", "version": "1"}})
send("notifications/initialized", None, notify=True)

# 隔离模式下宿主已经是自己起的 ⇒ 不许 MCP 再起一个（launch_if_none=false）；
# 逃生态保持原行为（没有实例时才允许宿主自己起一个）。
sess = call_tool("ocs_sessions", {"launch_if_none": STATE_DIR is None})
rows = sess["result"] if isinstance(sess, dict) and "result" in sess else sess
if isinstance(rows, dict):
    rows = [rows]
if STATE_DIR is not None:
    # ── 身份校验：拿到的 session 必须属于本脚本启动的宿主（pid 对不上就报错，绝不静默改用）──
    descs = read_state_descriptors(STATE_DIR)
    outsiders = []
    for row in rows:
        row_sid = row.get("session_id")
        desc = descs.get(row_sid)
        try:
            pid = int(desc.get("pid")) if desc else None
        except (TypeError, ValueError):
            pid = None
        if desc is None or owned_by(pid, HOST.pid) is not True:
            outsiders.append("%s(pid=%s)" % (row_sid, pid))
    if outsiders or HOST_DESC["session_id"] not in [r.get("session_id") for r in rows]:
        raise SystemExit(
            "# 实例归属校验失败：%s 里出现了不属于本脚本（宿主 pid=%s）的会话 %s；拒绝连接"
            "（绝不降级去用别人的实例）。确认目标后可用 OCS_ALLOW_EXISTING=1 显式连现有实例。"
            % (STATE_DIR, HOST.pid, "、".join(outsiders) or "自己那份描述符缺失"))
    rows = [r for r in rows if r.get("session_id") == HOST_DESC["session_id"]]
sid = rows[0]["session_id"]
doc = rows[0]["document_id"]
print(f"# session={sid} doc={doc} rev={rows[0]['revision']} "
      f"pid={HOST.pid if HOST else 'existing'}")

def show(d, tag=""):
    if not isinstance(d, dict):
        print(f"  {tag} ← {str(d)[:120]}")
        return {}
    st = d.get("state") or {}
    cmd = st.get("command") or {}
    print(f"  {tag} status={d.get('status')} code={d.get('code')} err={d.get('error')} "
          f"rev={st.get('revision')} geo={st.get('geometry_revision')} "
          f"prompt={(cmd.get('prompt') or '')[:50]!r} sel={st.get('selection')}")
    return st

for raw in sys.stdin.read().splitlines():
    line = raw.strip()
    if not line or line.startswith("#"):
        continue
    op, *rest = line.split(None, 1)
    arg = rest[0] if rest else ""
    if op == "run":
        d = call_tool("ocs_execute", {"ocs_session_id": sid,
                                      "request": {"request_id": f"mcp-{_id[0]}", "op": "run", "cmd": arg}})
        show(d, f"run {arg!r}")
    elif op == "input":
        parts = arg.split()
        kind = parts[0]
        req = {"request_id": f"mcp-{_id[0]}", "op": "input", "kind": kind}
        if kind == "entity":
            req["handle"] = parts[1]
            if len(parts) > 2:
                req["point"] = [float(x) for x in parts[2].split(",")]
        elif kind == "text" or kind == "token":
            req["text"] = " ".join(parts[1:])
        elif kind == "point":
            # `input point 40,60` → 世界坐标点（ZOOM 窗口角点、拾取点等）
            req["point"] = [float(x) for x in parts[1].split(",")] + [0.0]
            req["space"] = "wcs"
        elif kind == "enter":
            pass  # 回车（kind=enter 本身即语义）
        d = call_tool("ocs_execute", {"ocs_session_id": sid, "request": req})
        show(d, f"input {arg!r}")
    elif op == "newdoc":
        # `newdoc`：新建一张空白图纸（不然新起的会话里没有任何文档，run 会报 no_document）
        d = call_tool("ocs_execute", {"ocs_session_id": sid,
                                      "request": {"request_id": f"mcp-{_id[0]}", "op": "new"}})
        show(d, "newdoc")
    elif op == "cancel":
        d = call_tool("ocs_execute", {"ocs_session_id": sid,
                                      "request": {"request_id": f"mcp-{_id[0]}", "op": "cancel"}})
        show(d, "cancel")
    elif op == "query":
        parts = arg.split()
        params = {}
        if parts:
            params["type"] = parts[0]
        if len(parts) > 1:
            params["layer"] = parts[1]
        params["detail"] = "geometry"
        d = call_tool("ocs_read", {"ocs_session_id": sid, "op": "query", "parameters": params})
        for e in (d.get("entities") or []):
            print("   ", e.get("handle"), e.get("type"), e.get("start"), e.get("end"),
                  e.get("layer"), e.get("name"))
        print(f"    ({len(d.get('entities') or [])} 个)")
    elif op == "select":
        parts = arg.split()
        if parts and parts[0] == "type":
            req = {"request_id": f"mcp-{_id[0]}", "op": "select", "type": parts[1]}
        else:
            req = {"request_id": f"mcp-{_id[0]}", "op": "select", "handles": parts}
        d = call_tool("ocs_execute", {"ocs_session_id": sid, "request": req})
        show(d, f"select {arg!r}")
    else:
        print(f"  ? 未知指令 {line!r}")

# 收尾由 atexit 的 cleanup() 负责（退出/报错都会跑：杀宿主 + 杀 MCP 子进程 + 删自己的临时目录）
