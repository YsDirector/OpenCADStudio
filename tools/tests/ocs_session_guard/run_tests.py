#!/usr/bin/env python3
"""`tools/ocs_session.py` 的「不误连用户实例」守卫回归测试（仓库内版）。

★ 这套断言是「绝不误连别人（尤其是用户正在画图的那一个）实例」这条安全属性的**唯一**回归测试。

验什么：
  正控制   无外部实例 ⇒ 脚本从零起自己的宿主，最小 RPC（query）跑通。
  反证 A   共享位置放一个「别人的」活实例 ⇒ ★ 新脚本 connects == 0（根本没看见它），
           自己起宿主把活干完。
  红证     ★ 同一布局跑「隔离版之前」的老脚本夹具 ⇒ 它连进那个伪造实例（connects >= 1）且
           不自起宿主。同一布局下 connects==0 / connects>=1 的对比，就是整套测试的有效性来源；
           这条掉了，测试就失去意义。
  反证 B   OCS_ALLOW_EXISTING=1 才允许看见共享位置那个实例（逃生口，风险写在文件头）。
  反证 C   描述符 pid 对不上 ⇒ 立刻报错退出（绝不降级去用别人的 session）；
           私有目录里混进别人的活会话 ⇒ 发现成功但校验失败、不输出 `# session=`。
  真宿主   ★ 尽量用真宿主：OCS_BIN 直接指向 target/release/OpenCADStudio。真宿主是 GUI
           （`--new-instance`，src/mcp.rs:315），本机无显示且测试禁止上屏 ⇒ 起不来就
           **明确打印「跳过（原因）」**，★ 跳过绝不计入通过。
  收尾     零 GUI（pgrep -x OpenCADStudio 为空）、拉起过的假进程全部退出、真
           ~/.config/OpenCADStudio/automation 未被触碰（文件数 + 最新 mtime 快照）。

怎么跑：
    tools/tests/ocs_session_guard/run.sh                    # 一键（推荐）
    python3 tools/tests/ocs_session_guard/run_tests.py
  OCS_GUARD_KEEP=1 保留临时工作目录（默认跑完删干净）。零 cargo、零 GUI。

为什么这样造：
  ★ 全程隔离：自带 mktemp -d，HOME / XDG_* / OCS_* 全部指到那个临时目录（伪造描述符也在里面），
    跑完删除；不依赖 /tmp/ocsfix 之类外部残留，搬进仓库后就能独立跑。
  ★ 夹具钉死：`fixtures/ocs_session_pre_guard.py` 是 commit 6da03e1d 的 `tools/ocs_session.py`
    的**逐字快照**（sha256 记在 FIXTURE_SHA256），红证只许用它。上一批的教训：用
    `git show HEAD:tools/ocs_session.py` 取「隔离版之前」那份，隔离版一提交进 HEAD，同一条命令
    就取到了新版本，红证静默失效（41/41 直接掉到 38/41）。所以绝不再读 git 历史。
  ★ OCS_BIN 走 `fixtures/wrap_ocs_bin.sh`：`--mcp` 用真二进制（真 MCP 服务器 + 真描述符发现 /
    hello 握手链路），只有 `--new-instance` 用替身宿主（`fixtures/fake_ocs_host.py`，描述符字段
    照 src/app/control/transport.rs:44）—— 真宿主是 GUI，会开窗上屏，测试禁止。
"""
import hashlib, json, os, re, shutil, subprocess, sys, tempfile, time
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / "fixtures"
ROOT = HERE.parents[2]                       # tools/tests/ocs_session_guard → 仓库根
TOOL = ROOT / "tools" / "ocs_session.py"
REAL = ROOT / "target" / "release" / "OpenCADStudio"

# 冻结夹具的 sha256（钉死；改夹具就必须同步改这里，别指望测试会「自动适配」）
FIXTURE_SHA256 = "2c2dce157f20590f645cd445a02caaadea9f8ba6bbb0ce9948d3b6eba31d2649"

WORK = Path(os.environ.get("OCS_GUARD_WORK") or tempfile.mkdtemp(prefix="ocs-guard-"))
KEEP_WORK = os.environ.get("OCS_GUARD_KEEP", "").strip().lower() in ("1", "true", "yes", "on")
HOME = WORK / "home"
SHARED = HOME / ".config"                    # 合成「共享位置」（= 老脚本/逃生口看的地方）
AUTO = SHARED / "OpenCADStudio" / "automation"
PLUGINS = SHARED / "OpenCADStudio" / "plugins" / "opencad.ocsm"
FAKE = FIXTURES / "fake_ocs_host.py"
WRAP = WORK / "wrap_ocs_bin.sh"              # 从夹具拷进临时目录（不改仓库里那份的模式位）
ORIG = FIXTURES / "ocs_session_pre_guard.py"  # 隔离版之前的老脚本（冻结在仓库里）
REAL_OCS_CONFIG = Path.home() / ".config" / "OpenCADStudio"   # ★ 只读：种子源 + 真 automation 快照
REAL_AUTO = REAL_OCS_CONFIG / "automation"
STDIN = "query line 0层\n"

results, skips, spawn_pids = [], [], []
for _stale in WORK.glob("*.log"):        # 每次都从干净日志开始，否则重复跑会被上一轮的行数绊到
    _stale.unlink()


def _cleanup_work():
    if KEEP_WORK:
        print("\n（保留工作目录 OCS_GUARD_KEEP=1：%s）" % WORK)
        return
    shutil.rmtree(WORK, ignore_errors=True)


import atexit  # noqa: E402  （放在清理函数定义之后，只为可读性）

atexit.register(_cleanup_work)


def check(name, cond, detail=""):
    results.append((name, bool(cond), detail))
    print(("  PASS  " if cond else "  FAIL  ") + name + (("   | " + detail) if detail else ""))
    return bool(cond)


def skip(name, detail=""):
    """明确跳过：★ 不计入通过，也不计入失败（只在汇总里单列）。"""
    skips.append(name)
    print("  SKIP  " + name + (("   | " + detail) if detail else ""))


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def fixture_body(path):
    """夹具正文 = 去掉文件头注释块（说明这夹具是怎么来的）之后的逐字快照。"""
    lines = Path(path).read_text(encoding="utf-8").splitlines()
    i = 0
    while i < len(lines) and lines[i].startswith("#"):
        i += 1
    return "\n".join(lines[i:])


def env_for(wrap_log, ocs_bin=None, nodisplay=False, **extra):
    """子进程环境：私有 HOME/XDG + OCS_BIN 替身；★ 绝不把用户的真实目录交给被测脚本。"""
    env = {k: v for k, v in os.environ.items()
           if not k.startswith("OCS_") and k != "XDG_CONFIG_HOME"}
    env.pop("DISPLAY", None)
    env.update({"HOME": str(HOME), "XDG_CONFIG_HOME": str(SHARED), "OCS_BIN": str(ocs_bin or WRAP),
                "OCS_REAL": str(REAL), "OCS_FAKE_HOST": str(FAKE), "OCS_WRAP_LOG": str(wrap_log),
                # 真二进制（--mcp、真宿主探测）也绝不写用户的 ~/.local/share 之类
                "XDG_DATA_HOME": str(WORK / "xdg-data"),
                "XDG_CACHE_HOME": str(WORK / "xdg-cache"),
                "XDG_STATE_HOME": str(WORK / "xdg-state")})
    if nodisplay:
        # ★ 真宿主探测专用：把显示目标指到**不存在**的通道（并移除 DISPLAY），
        #   保证「真宿主起不来」时也绝不可能把窗口开到用户桌面上。
        env.update({"WAYLAND_DISPLAY": "ocs-guard-nodisplay", "XDG_RUNTIME_DIR": str(WORK / "xdg-runtime")})
        env.pop("DISPLAY", None)
    env.update({k: str(v) for k, v in extra.items()})
    return env


def run_tool(script, wrap_log, stdin=STDIN, timeout=180, ocs_bin=None, nodisplay=False, **extra):
    t0 = time.time()
    p = subprocess.run([sys.executable, str(script)], input=stdin, capture_output=True,
                       text=True, env=env_for(wrap_log, ocs_bin=ocs_bin, nodisplay=nodisplay, **extra),
                       timeout=timeout)
    check("本 case 全程未起真 GUI（pgrep -x OpenCADStudio 为空）", pgrep_ocs() == [], str(pgrep_ocs()))
    return p, time.time() - t0


def events(path):
    if not os.path.exists(path):
        return []
    out = []
    for line in open(path, encoding="utf-8"):
        if line.strip():
            try:
                out.append(json.loads(line))
            except ValueError:
                pass
    return out


def n_events(path, event):
    return sum(1 for e in events(path) if e.get("event") == event)


def phrase(p, key):
    """从 '#' 开头的诊断行里取 key=value（value 到空白/标点为止）。"""
    for line in p.stdout.splitlines():
        if line.startswith("#"):
            m = re.search(r"(?:^|[\s（(])" + key + r"=(\S+)", line)
            if m:
                return m.group(1).rstrip("；;）)")
    return None


def reset_shared():
    shutil.rmtree(SHARED, ignore_errors=True)
    AUTO.mkdir(parents=True, exist_ok=True)
    PLUGINS.mkdir(parents=True, exist_ok=True)
    os.chmod(AUTO, 0o700)


def start_forged(sid, log, pid_override=0):
    """在共享位置放一个「别人的」活实例（描述符格式照 transport.rs:44）。"""
    p = subprocess.Popen([sys.executable, str(FAKE), "--dir", str(AUTO), "--sid", sid,
                          "--log", str(log)] + (["--pid-override", str(pid_override)] if pid_override else []),
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         env=dict(os.environ, XDG_CONFIG_HOME=str(SHARED)))
    spawn_pids.append(p.pid)
    for _ in range(100):
        if (AUTO / (sid + ".json")).exists():
            return p
        time.sleep(0.05)
    raise SystemExit("伪造实例没起来")


def alive(pid):
    """还活着的进程（僵尸不算：已终止但未 reap 的子进程仍是 /proc 条目）。"""
    try:
        with open("/proc/%d/stat" % pid, "rb") as fh:
            if fh.read().rsplit(b")", 1)[1].split()[0] == b"Z":
                return False
    except OSError:
        return False
    return True


def reap(p):
    """终止并收尸，别把僵尸当残留。"""
    if p.poll() is None:
        p.terminate()
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            p.kill()
            p.wait(timeout=5)


def pgrep_ocs():
    return subprocess.run(["pgrep", "-x", "OpenCADStudio"], capture_output=True, text=True).stdout.split()


def real_auto_state():
    return (len(list(REAL_AUTO.glob("*.json"))) if REAL_AUTO.is_dir() else -1,
            max((f.stat().st_mtime for f in REAL_AUTO.glob("*.json")), default=0) if REAL_AUTO.is_dir() else 0)


# ── 准备 ─────────────────────────────────────────────────────────────────────
print("== 准备 ==")
print("  仓库根: %s" % ROOT)
print("  临时工作目录: %s（%s）" % (WORK, "OCS_GUARD_KEEP=1 ⇒ 跑完保留" if KEEP_WORK else "跑完删除"))
print("  老脚本夹具（隔离版之前，冻结在仓库里）: %s" % ORIG)
check("夹具是隔离版之前的快照（sha256 与记录一致 ⇒ 钉死，不读 git 历史）",
      ORIG.is_file() and sha256(ORIG) == FIXTURE_SHA256,
      "sha256=%s" % (sha256(ORIG)[:16] if ORIG.is_file() else "缺失"))
check("夹具正文确实不含隔离/逃生口（没有 OCS_ALLOW_EXISTING，就是改之前那份）",
      "OCS_ALLOW_EXISTING" not in fixture_body(ORIG), str(ORIG.parent))
check("红证夹具来自仓库文件而不是 git 历史", ORIG.is_file() and ORIG.parent.name == "fixtures", str(ORIG))
check("真二进制存在（--mcp 真链路 + 真宿主探测用）", REAL.is_file(), str(REAL))
shutil.copy2(FIXTURES / "wrap_ocs_bin.sh", WRAP)
os.chmod(WRAP, 0o755)
real_before = real_auto_state()
print("  真 ~/.config/OpenCADStudio/automation 快照（文件数, 最新 mtime）=", real_before)
check("起手 pgrep -x OpenCADStudio 为空", pgrep_ocs() == [], str(pgrep_ocs()))

# ── 正控制：无任何外部实例 → 从零起自己的实例并完成一次最小 RPC ──────────────
print("\n== 正控制：无外部实例，从零启动 + 最小 RPC（query） ==")
reset_shared()
r, dt = run_tool(TOOL, WORK / "wrap_ctrl.log", OCS_FAKE_LOG=WORK / "host_ctrl.log")
hl = WORK / "host_ctrl.log"
env_dump = [e for e in events(hl) if e.get("event") == "start"]
state_dir = phrase(r, "state_dir")
check("退出码 0", r.returncode == 0, "rc=%s dt=%.1fs" % (r.returncode, dt))
check("打印隔离横幅 + 走临时私有目录", "# 隔离模式" in r.stdout and (state_dir or "").startswith("/tmp/ocs-session-state-"),
      state_dir or "?")
check("最小 RPC 完成（query → 0 个实体）", "(0 个)" in r.stdout, r.stdout.strip().splitlines()[-1] if r.stdout else "")
check("宿主真的被脚本自己拉起（wrap 收到 --new-instance）",
      [l for l in open(WORK / "wrap_ctrl.log") if "--new-instance" in l].__len__() == 1)
check("宿主拿到的 XDG_CONFIG_HOME == 私有目录（隔离透传到宿主）",
      env_dump and env_dump[0]["xdg"] == state_dir, str(env_dump[0]["xdg"]) if env_dump else "no host")
check("宿主仍能拿到用户真实插件目录（OCS_PLUGINS_DIR，保证 OCSM 命令可用）",
      env_dump and env_dump[0]["plugins"] == str(SHARED / "OpenCADStudio" / "plugins"),
      str(env_dump[0]["plugins"]) if env_dump else "no host")
check("私有临时目录退出后已删除", state_dir and not os.path.exists(state_dir))
check("自己起的宿主退出后无残留", env_dump and not alive(env_dump[0]["real_pid"]),
      "host pid=%s" % (env_dump[0]["real_pid"] if env_dump else "-"))

# ── 反证 A：共享位置有伪造实例 → 默认隔离必须看不见它 ─────────────────────────
print("\n== 反证 A：共享位置放伪造实例（FAKE-SHARED），默认模式必须不连它 ==")
reset_shared()
shared_log = WORK / "shared_A.log"
forged = start_forged("FAKE-SHARED", shared_log)
print("  伪造实例 pid=%d 描述符=%s" % (forged.pid, AUTO / "FAKE-SHARED.json"))
r, dt = run_tool(TOOL, WORK / "wrap_A.log", OCS_FAKE_LOG=WORK / "host_A.log")
sid_a = phrase(r, "session")
check("退出码 0（自己起实例照常干活）", r.returncode == 0, "rc=%s dt=%.1fs" % (r.returncode, dt))
check("选中的 session 不是那个伪造实例", sid_a and sid_a != "FAKE-SHARED", "session=%s" % sid_a)
check("★ 伪造实例收到的 TCP 连接数 == 0（隔离生效，根本没看见它）",
      n_events(shared_log, "connect") == 0, "connects=%d" % n_events(shared_log, "connect"))
check("自己起的宿主被用了（收到 hello + query）",
      n_events(WORK / "host_A.log", "request") >= 1,
      "reqs=%d" % n_events(WORK / "host_A.log", "request"))
reap(forged)

# ── 红证：同一个布局，改之前的脚本会直接连进伪造实例 ─────────────────────────
print("\n== 红证：同一布局跑「隔离版之前」的冻结夹具（证明缺口真实存在、且本验证能红） ==")
reset_shared()
shared_log = WORK / "shared_red.log"
forged = start_forged("FAKE-SHARED", shared_log)
r, dt = run_tool(ORIG, WORK / "wrap_red.log")
sid_red = phrase(r, "session")
check("老脚本退出码 0 且选中 FAKE-SHARED（连进了别人的实例）",
      r.returncode == 0 and sid_red == "FAKE-SHARED", "rc=%s session=%s" % (r.returncode, sid_red))
check("★ 伪造实例被连上（connects>=1，正是新脚本为 0 的那条连接）",
      n_events(shared_log, "connect") >= 1, "connects=%d" % n_events(shared_log, "connect"))
check("老脚本没有自己起宿主（wrap 无 --new-instance）",
      not [l for l in open(WORK / "wrap_red.log") if "--new-instance" in l])
reap(forged)

# ── 反证 B：逃生口 OCS_ALLOW_EXISTING=1 才允许连现有实例 ─────────────────────
print("\n== 反证 B：OCS_ALLOW_EXISTING=1 才允许看见共享位置那个实例 ==")
reset_shared()
shared_log = WORK / "shared_B.log"
forged = start_forged("FAKE-SHARED", shared_log)
r, dt = run_tool(TOOL, WORK / "wrap_B.log", OCS_ALLOW_EXISTING="1")
sid_b = phrase(r, "session")
check("退出码 0", r.returncode == 0, "rc=%s" % r.returncode)
check("★ 这次选中了共享位置那个实例 FAKE-SHARED", sid_b == "FAKE-SHARED", "session=%s" % sid_b)
check("★ 它确实连了共享实例（connects>=1，与反证 A 的 0 形成可判定差异）",
      n_events(shared_log, "connect") >= 1, "connects=%d" % n_events(shared_log, "connect"))
check("逃生口下没再自己起宿主（wrap 无 --new-instance）",
      not [l for l in open(WORK / "wrap_B.log") if "--new-instance" in l])
check("stderr 有风险提示", "逃生态" in r.stderr, r.stderr.strip()[:80])
reap(forged)

# ── 反证 C：描述符 pid 对不上 → 必须报错退出，不得静默改用 ───────────────────
print("\n== 反证 C1：宿主写的描述符 pid 对不上（伪造 pid）→ 必须立刻报错 ==")
reset_shared()
victim = subprocess.Popen(["sleep", "120"])
spawn_pids.append(victim.pid)
r, dt = run_tool(TOOL, WORK / "wrap_C.log", OCS_FAKE_PID_OVERRIDE=victim.pid, OCS_FAKE_LOG=WORK / "host_C.log")
check("非 0 退出", r.returncode != 0, "rc=%s dt=%.1fs" % (r.returncode, dt))
check("报「实例归属校验失败」并点名 pid", "实例归属校验失败" in r.stderr and str(victim.pid) in r.stderr,
      r.stderr.strip().splitlines()[0][:110] if r.stderr else "")
check("没有输出 # session=（绝不降级去用别人的 session）", "# session=" not in r.stdout, r.stdout.strip()[:60])
check("失败是「归属不符」而非 45s 超时", dt < 20, "dt=%.1fs" % dt)
reap(victim)

print("\n== 反证 C2：私有目录里混进一个不属于自己的活会话（发现之后校验） ==")
reset_shared()
victim = subprocess.Popen(["sleep", "120"])
spawn_pids.append(victim.pid)
r, dt = run_tool(TOOL, WORK / "wrap_C2.log", OCS_FAKE_LOG=WORK / "host_C2.log",
                 OCS_FAKE_EXTRA_SID="IMPOSTOR", OCS_FAKE_EXTRA_PID=victim.pid)
check("非 0 退出", r.returncode != 0, "rc=%s dt=%.1fs" % (r.returncode, dt))
check("报「实例归属校验失败」且点名假会话+pid",
      "实例归属校验失败" in r.stderr and "IMPOSTOR" in r.stderr and str(victim.pid) in r.stderr,
      (r.stderr.strip().splitlines() or [""])[0][:120])
check("发现阶段确实成功过（宿主收到过 hello，不是没人应答）",
      n_events(WORK / "host_C2.log", "request") >= 1,
      "reqs=%d" % n_events(WORK / "host_C2.log", "request"))
check("仍然没有输出 # session=", "# session=" not in r.stdout)
reap(victim)

print("\n== 反证 C3（对照）：同一枚「pid 对不上」的描述符放共享位置 + 逃生口 → 才被允许连 ==")
reset_shared()
victim = subprocess.Popen(["sleep", "120"])
spawn_pids.append(victim.pid)
shared_log = WORK / "shared_C3.log"
forged = start_forged("IMPOSTOR-SHARED", shared_log, pid_override=victim.pid)
r, dt = run_tool(TOOL, WORK / "wrap_C3.log", OCS_ALLOW_EXISTING="1")
check("逃生口下退出码 0 且用它（说明默认模式的拒绝确实来自归属校验）",
      r.returncode == 0 and phrase(r, "session") == "IMPOSTOR-SHARED",
      "rc=%s session=%s" % (r.returncode, phrase(r, "session")))
check("它仍然没自己起宿主（用了现有实例，wrap 无 --new-instance）",
      not [l for l in open(WORK / "wrap_C3.log") if "--new-instance" in l])
reap(forged)
reap(victim)

# ── 正控制（真宿主）：尽量用真宿主；起不来就明确跳过，绝不算通过 ─────────────
print("\n== 正控制（真宿主探测）：OCS_BIN 直接指向真二进制 ==")
print("  %s（真宿主是 GUI：--new-instance，src/mcp.rs:315；无显示 ⇒ 允许「跳过」，但不许当通过）" % REAL)
reset_shared()
r, dt = run_tool(TOOL, WORK / "wrap_real.log", ocs_bin=REAL, nodisplay=True, timeout=240,
                 OCS_FAKE_LOG=WORK / "host_real.log", OCS_SEED_FROM=str(REAL_OCS_CONFIG))
first_err = next((l.strip() for l in (r.stderr or "").splitlines() if l.strip()), "")
if r.returncode == 0 and "(0 个)" in r.stdout:
    check("真宿主路线：真二进制被脚本自己拉起 + 最小 RPC（query → 0 个实体）完成", True,
          "rc=0 dt=%.1fs" % dt)
elif r.returncode != 0 and ("宿主启动失败" in r.stderr or "宿主启动超时" in r.stderr):
    skip("真宿主路线（真 OCS 宿主 + 最小 RPC）",
         "跳过（%s；rc=%s dt=%.1fs）—— 不计入通过；替身宿主路径已在上面的 case 实跑"
         % (first_err[:130], r.returncode, dt))
else:
    check("真宿主探测：要么真跑通、要么明确「起不来」（★ 不许被当成通过）", False,
          "rc=%s stdout=%r stderr=%r" % (r.returncode, (r.stdout or "")[-160:], first_err[:160]))

# ── 收尾自证 ─────────────────────────────────────────────────────────────────
print("\n== 收尾自证 ==")
time.sleep(1.0)
check("pgrep -x OpenCADStudio 仍为空（全程零 GUI）", pgrep_ocs() == [], str(pgrep_ocs()))
leftover = [p for p in spawn_pids if alive(p)]
check("本验证拉起的所有假进程都已退出", leftover == [], "leftover=%s" % leftover)
real_after = real_auto_state()
check("真 ~/.config/OpenCADStudio/automation 未被触碰",
      real_after == real_before, "before=%s after=%s" % (real_before, real_after))

bad = [n for n, ok, _ in results if not ok]
print("\n===== 汇总: %d 通过 / %d 失败 / %d 跳过（★ 跳过不计入通过）====="
      % (len(results) - len(bad), len(bad), len(skips)))
for n in bad:
    print("  FAIL:", n)
for n in skips:
    print("  SKIP:", n)
sys.exit(1 if bad else 0)
