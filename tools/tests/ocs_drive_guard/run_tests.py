#!/usr/bin/env python3
"""`tools/ocs_drive.py` 的「默认不猜目标实例」守卫回归测试（仓库内版）。

★ 这套断言是「ocs_drive 不显式指定就绝不驱动现有实例」这条安全属性的**唯一**回归测试：
  默认拒绝（rc=2）、拒绝先于任何 TCP 连接、只有 --session / --allow-existing /
  OCS_ALLOW_EXISTING=1 三种显式许可才连；无实例时 rc=1 且明确报错。

验什么（同一布局 = 自建无头实例 + 共享临时 XDG）：
  正控制   tools/ocs_session.py（OCS_BIN 替身 + OCS_KEEP_HOST=1）起自家无头实例 A；
           run_tests.py 直接起第二个自家实例 B。
  list     不需要许可：rc=0、列两个实例、一次连接都不发。
  红证     ★ 同一布局跑「加固前」的冻结夹具（98b472f9 版）：旧脚本 rc=0 且真的连上 A/B
           （connects >= 1，打印出 mode/doc 实例信息）。新版默认这条 connects == 0
           —— 同一布局下 connects>=1 / ==0 的对比，就是整套测试的有效性来源；这条掉了测试即失去意义。
  反证     新版不带许可 ⇒ rc=2、stderr「拒绝执行」、★ 拒绝先于连接（A/B connects == 0）。
  反证     --allow-existing 与 OCS_ALLOW_EXISTING=1 两种显式许可 ⇒ 才连上（connects >= 1）。
  反证     --session DRIVE-B ⇒ 只连/只驱动 B，A 的 connects == 0。
  反证     空布局无实例 ⇒ rc=1 + 明确报错（绝不静默成功）。
  收尾     零 GUI（pgrep -x OpenCADStudio 为空）、自己拉起的实例全部退出、真
           ~/.config/OpenCADStudio/automation 未被触碰（文件数 + 最新 mtime 快照）。

怎么跑：
    tools/tests/ocs_drive_guard/run.sh                    # 一键（推荐）
    python3 tools/tests/ocs_drive_guard/run_tests.py
  OCS_GUARD_KEEP=1 保留临时工作目录（默认跑完删干净）。零 GUI、零 cargo、全程临时目录。

为什么这样造：
  ★ 全程隔离：自带 mktemp -d，HOME / XDG_* / OCS_* 全部指到那个临时目录（描述符也在里面），
    跑完删除；不依赖 /tmp/b_drive_check.sh 之类外部残留，搬进仓库后就能独立跑。
  ★ 夹具钉死：`fixtures/ocs_drive_pre_guard.py` 是 commit 98b472f9 的 `tools/ocs_drive.py`
    的**逐字节快照**（sha256 记在 FIXTURE_SHA256，行数/体积见 README.md），红证只许用它。
    测试绝不读 git 历史；夹具里不加任何注释头（加了 sha256 就对不上，说明文字一律放 README.md）。
  ★ 无头实例：真宿主是 GUI（--new-instance，src/mcp.rs:315），本机无显示且测试禁止上屏，
    所以 OCS_BIN 走 `fixtures/wrap_ocs_bin.sh`：`--new-instance` 用自家无头实例夹具
    （`fixtures/fake_ocs_instance.py`，描述符字段照 src/app/control/transport.rs:44、协议照
    transport.rs 的 token+session_id 校验），`--mcp` 用真二进制（真 MCP 客户端 / 真描述符发现）。
"""
import hashlib, json, os, shutil, signal, subprocess, sys, tempfile, time
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / "fixtures"
ROOT = HERE.parents[2]                       # tools/tests/ocs_drive_guard → 仓库根
TOOL = ROOT / "tools" / "ocs_drive.py"
SESSION_TOOL = ROOT / "tools" / "ocs_session.py"
REAL = ROOT / "target" / "release" / "OpenCADStudio"
OLD = FIXTURES / "ocs_drive_pre_guard.py"    # 加固前那版（冻结在仓库里，只用于证明能红）
FAKE = FIXTURES / "fake_ocs_instance.py"
WRAP_SRC = FIXTURES / "wrap_ocs_bin.sh"
WRAP = None                                  # 拷进临时目录再 chmod（仓库夹具保持 644）

# 冻结夹具的 sha256（钉死；改夹具就必须同步改这里 + README.md 里的行数/体积，并重跑红证）
FIXTURE_SHA256 = "4f095f8c64969d86cce40fc423e0110e0aa3020f090b7bd8f77f40218cce93cb"

WORK = Path(os.environ.get("OCS_GUARD_WORK") or tempfile.mkdtemp(prefix="ocs-drive-guard-"))
KEEP_WORK = os.environ.get("OCS_GUARD_KEEP", "").strip().lower() in ("1", "true", "yes", "on")
HOME = WORK / "home"
XDG = WORK / "xdg"                            # 共享临时 XDG：所有靶子描述符都在这
AUTO = XDG / "OpenCADStudio" / "automation"
EMPTY_XDG = WORK / "empty-xdg"                # 「无实例」反证用的空布局
REAL_OCS_CONFIG = Path.home() / ".config" / "OpenCADStudio"   # ★ 只读：真 automation 快照
REAL_AUTO = REAL_OCS_CONFIG / "automation"

results, skips, spawned = [], [], []          # spawned: 本验证拉起的实例 pid（收尾全杀）


def _cleanup_work():
    for pid in spawned:
        try:
            os.kill(pid, signal.SIGTERM)
        except OSError:
            pass
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


def env_for(xdg, **extra):
    """子进程环境：私有 HOME/XDG，剥掉父进程的 OCS_*（★ 别把用户真实配置交出去）。"""
    env = {k: v for k, v in os.environ.items()
           if not k.startswith("OCS_") and k not in ("XDG_CONFIG_HOME", "DISPLAY", "WAYLAND_DISPLAY")}
    env.update({"HOME": str(HOME), "XDG_CONFIG_HOME": str(xdg),
                "XDG_DATA_HOME": str(WORK / "xdg-data"),
                "XDG_CACHE_HOME": str(WORK / "xdg-cache"),
                "XDG_STATE_HOME": str(WORK / "xdg-state")})
    env.update({k: str(v) for k, v in extra.items()})
    return env


def run_drive(script, *args, xdg=None, extra_env=None, timeout=90):
    """跑被测脚本（新版 tools/ocs_drive.py 或旧夹具），返回 (CompletedProcess, t0)。

    t0 用来只数「这一次运行期间」的 TCP 连接（假实例日志里带时间戳）。
    """
    t0 = time.time()   # ★ 不许回拨：上一轮的 connect 就在 t0 前几毫秒，回拨会把它们算进本轮
    p = subprocess.run([sys.executable, str(script)] + [str(a) for a in args],
                       capture_output=True, text=True,
                       env=env_for(xdg or XDG, **(extra_env or {})), timeout=timeout)
    return p, t0


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


def connects_since(path, t0):
    """t0 之后该实例收到的 TCP 连接数（证明「到底连没连」的硬指标）。"""
    return sum(1 for e in events(path) if e.get("event") == "connect" and float(e.get("at", 0)) >= t0)


def alive(pid):
    """还活着的进程（僵尸不算：已终止但未 reap 的子进程仍是 /proc 条目）。"""
    try:
        with open("/proc/%d/stat" % pid, "rb") as fh:
            if fh.read().rsplit(b")", 1)[1].split()[0] == b"Z":
                return False
    except OSError:
        return False
    return True


def wait_dead(pid, timeout=5.0):
    deadline = time.time() + timeout
    while time.time() < deadline and alive(pid):
        time.sleep(0.05)
    return not alive(pid)


def reap(p):
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


def wait_descriptor(sid, timeout=60.0):
    path = AUTO / (sid + ".json")
    deadline = time.time() + timeout
    while time.time() < deadline:
        if path.exists():
            try:
                return json.loads(path.read_text(encoding="utf-8"))
            except ValueError:
                pass
        time.sleep(0.05)
    return None


# ── 准备 ─────────────────────────────────────────────────────────────────────
print("== 准备 ==")
print("  仓库根: %s" % ROOT)
print("  临时工作目录: %s（%s）" % (WORK, "OCS_GUARD_KEEP=1 ⇒ 跑完保留" if KEEP_WORK else "跑完删除"))
print("  加固前夹具（98b472f9 的逐字节快照，冻结在仓库里）: %s" % OLD)
check("夹具是加固前的逐字节快照（sha256 与记录一致 ⇒ 钉死，不读 git 历史）",
      OLD.is_file() and sha256(OLD) == FIXTURE_SHA256,
      "sha256=%s" % (sha256(OLD)[:16] if OLD.is_file() else "缺失"))
check("红证夹具正文确实不含显式许可（没有 --allow-existing / OCS_ALLOW_EXISTING，就是加固前那份）",
      OLD.is_file() and "--allow-existing" not in OLD.read_text(encoding="utf-8")
      and "OCS_ALLOW_EXISTING" not in OLD.read_text(encoding="utf-8"), str(OLD))
check("真二进制存在（会话工具的 --mcp 客户端用）", REAL.is_file(), str(REAL))
shutil.copy2(WRAP_SRC, WORK / "wrap_ocs_bin.sh")
WRAP = WORK / "wrap_ocs_bin.sh"
os.chmod(WRAP, 0o755)
real_before = real_auto_state()
print("  真 ~/.config/OpenCADStudio/automation 快照（文件数, 最新 mtime）=", real_before)
check("起手 pgrep -x OpenCADStudio 为空", pgrep_ocs() == [], str(pgrep_ocs()))

# ── 起靶子 A：tools/ocs_session.py 自己拉起的自家无头实例（kept alive）────────
print("\n== 起靶子 A：tools/ocs_session.py（OCS_BIN 替身 + OCS_KEEP_HOST=1）==")
log_a = WORK / "instance_a.log"
t0 = time.time()
p = subprocess.run([sys.executable, str(SESSION_TOOL)], input="", capture_output=True, text=True,
                   env=env_for(XDG, OCS_BIN=str(WRAP), OCS_REAL=str(REAL), OCS_FAKE_INSTANCE=str(FAKE),
                               OCS_FAKE_SID="DRIVE-A", OCS_FAKE_LOG=str(log_a),
                               OCS_SESSION_STATE_DIR=str(XDG), OCS_WRAP_LOG=str(WORK / "wrap_a.log"),
                               OCS_KEEP_HOST="1"),
                   timeout=180)
dt = time.time() - t0
check("会话工具退出码 0（自家无头实例已就绪）", p.returncode == 0, "rc=%s dt=%.1fs" % (p.returncode, dt))
check("会话工具打印 # session=DRIVE-A（它认成自己那份实例）",
      "# session=DRIVE-A" in p.stdout, next((l for l in p.stdout.splitlines() if l.startswith("# session=")), ""))
check("会话工具真的自己起了宿主（wrap 收到 --new-instance）",
      any("--new-instance" in l for l in open(WORK / "wrap_a.log")))
adesc = wait_descriptor("DRIVE-A")
pid_a = adesc["pid"] if adesc else -1
if pid_a > 0:                                # ★ 绝不把 -1 放进收尾杀进程列表（os.kill(-1) 会杀一片）
    spawned.append(pid_a)
check("靶子 A 描述符落在共享临时 XDG 且进程活着", bool(adesc) and alive(pid_a),
      "descriptor=%s pid=%s" % (AUTO / "DRIVE-A.json", pid_a))
check("A 已被真 --mcp 客户端握手过（日志有 request）", len([e for e in events(log_a) if e.get("event") == "request"]) >= 1)

# ── 起靶子 B：同一共享 XDG 里的第二个自家无头实例（验证 --session 的精度）─────
print("\n== 起靶子 B：第二个自家无头实例（直接拉起同一夹具，同一共享 XDG）==")
log_b = WORK / "instance_b.log"
p_b = subprocess.Popen([sys.executable, str(FAKE), "--dir", str(AUTO), "--sid", "DRIVE-B",
                        "--log", str(log_b)],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env_for(XDG))
spawned.append(p_b.pid)
bdesc = wait_descriptor("DRIVE-B")
pid_b = bdesc["pid"] if bdesc else -1
check("靶子 B 已就绪（同一共享 XDG，pid 活着）", bool(bdesc) and alive(pid_b),
      "descriptor=%s pid=%s" % (AUTO / "DRIVE-B.json", pid_b))

# ── list：唯一不需要显式许可的 op，且一次连接都不发 ─────────────────────────
print("\n== list：不需要许可、不连接 ==")
r, t0 = run_drive(TOOL, "list")
check("rc=0 且列出两个实例（无需任何许可）",
      r.returncode == 0 and r.stdout.count("session=") == 2, "rc=%s lines=%d" % (r.returncode, r.stdout.count("session=")))
check("list 一次 TCP 连接都不发（A/B connects == 0）",
      connects_since(log_a, t0) == 0 and connects_since(log_b, t0) == 0,
      "A=%d B=%d" % (connects_since(log_a, t0), connects_since(log_b, t0)))

# ── 红证：同一布局，加固前的旧夹具会直接连上去 ──────────────────────────────
print("\n== 红证：同一布局跑加固前夹具（证明缺口真实存在、且本验证能红） ==")
r, t0 = run_drive(OLD, "state")
red_line = next((l for l in r.stdout.splitlines() if l.startswith("pid=")), "")
check("旧夹具 rc=0 且真的打印出实例信息（mode / doc）",
      r.returncode == 0 and " mode=" in red_line and "doc=" in red_line, red_line)
check("★ 旧夹具连上了自家实例 A（connects>=1，正是新版为 0 的那条连接）",
      connects_since(log_a, t0) >= 1, "connects=%d" % connects_since(log_a, t0))
check("旧夹具把 B 也探测了（state 会连全部活实例）",
      connects_since(log_b, t0) >= 1, "connects=%d" % connects_since(log_b, t0))

# ── 反证：新版默认拒绝，且拒绝先于任何连接 ──────────────────────────────────
print("\n== 反证：新版默认拒绝 + 拒绝先于任何连接 ==")
r, t0 = run_drive(TOOL, "state")
check("rc=2（被守卫拒绝，不是连上去之后才报错）", r.returncode == 2, "rc=%s" % r.returncode)
check("stderr 明确「拒绝执行」并给出显式许可办法",
      "拒绝执行" in r.stderr and "--session" in r.stderr and "--allow-existing" in r.stderr,
      next((l.strip() for l in r.stderr.splitlines() if l.strip()), "")[:100])
check("★ 拒绝先于任何连接：A/B 的 TCP 连接数都 == 0",
      connects_since(log_a, t0) == 0 and connects_since(log_b, t0) == 0,
      "A=%d B=%d" % (connects_since(log_a, t0), connects_since(log_b, t0)))

# ── 反证：--allow-existing（flag 形式）才连上 ───────────────────────────────
print("\n== 反证：--allow-existing 才连上 ==")
r, t0 = run_drive(TOOL, "--allow-existing", "state")
check("rc=0 且打印实例信息（mode=gui）", r.returncode == 0 and "mode=gui" in r.stdout,
      next((l for l in r.stdout.splitlines() if l.startswith("pid=")), ""))
check("A 被连上（connects>=1）", connects_since(log_a, t0) >= 1,
      "connects=%d" % connects_since(log_a, t0))
check("stderr 有逃生态风险提示", "逃生态" in r.stderr,
      next((l for l in r.stderr.splitlines() if "逃生态" in l), "")[:90])

# ── 反证：OCS_ALLOW_EXISTING=1（环境变量形式）才连上 ────────────────────────
print("\n== 反证：OCS_ALLOW_EXISTING=1 才连上 ==")
r, t0 = run_drive(TOOL, "state", extra_env={"OCS_ALLOW_EXISTING": "1"})
check("rc=0 且打印实例信息（mode=gui）", r.returncode == 0 and "mode=gui" in r.stdout,
      next((l for l in r.stdout.splitlines() if l.startswith("pid=")), ""))
check("A 被连上（connects>=1）", connects_since(log_a, t0) >= 1,
      "connects=%d" % connects_since(log_a, t0))
check("stderr 有逃生态风险提示", "逃生态" in r.stderr,
      next((l for l in r.stderr.splitlines() if "逃生态" in l), "")[:90])

# ── 反证：--session 只驱动指定的那一个 ──────────────────────────────────────
print("\n== 反证：--session DRIVE-B 只驱动 B ==")
r, t0 = run_drive(TOOL, "--session", "DRIVE-B", "state")
check("rc=0 且只打印 B 的 pid 行（没有 A）",
      r.returncode == 0 and ("pid=%s" % pid_b) in r.stdout and ("pid=%s" % pid_a) not in r.stdout,
      "out=%r" % r.stdout.strip()[:110])
check("只连了 B：B connects>=1 且 A connects==0",
      connects_since(log_b, t0) >= 1 and connects_since(log_a, t0) == 0,
      "A=%d B=%d" % (connects_since(log_a, t0), connects_since(log_b, t0)))

# ── 反证：空布局无实例 ⇒ rc=1 + 明确报错 ────────────────────────────────────
print("\n== 反证：空布局无实例 ⇒ rc=1 + 明确报错 ==")
(EMPTY_XDG / "OpenCADStudio" / "automation").mkdir(parents=True, exist_ok=True)
r, _ = run_drive(TOOL, "--session", "NOPE", "state", xdg=EMPTY_XDG)
check("rc=1（发现不到就明确失败，绝不静默成功）", r.returncode == 1, "rc=%s" % r.returncode)
check("stderr 点名「没有匹配」的实例",
      "没有匹配" in r.stderr, next((l.strip() for l in r.stderr.splitlines() if l.strip()), "")[:110])

# ── 收尾自证 ─────────────────────────────────────────────────────────────────
print("\n== 收尾自证 ==")
if pid_a > 0:
    try:
        os.kill(pid_a, signal.SIGTERM)
    except OSError:
        pass
reap(p_b)
check("靶子 A 已退出", wait_dead(pid_a), "pid=%s" % pid_a)
check("靶子 B 已退出", wait_dead(pid_b), "pid=%s" % pid_b)
time.sleep(0.5)
check("pgrep -x OpenCADStudio 仍为空（全程零 GUI）", pgrep_ocs() == [], str(pgrep_ocs()))
leftover = [pid for pid in spawned if alive(pid)]
check("本验证拉起的所有实例都已退出", leftover == [], "leftover=%s" % leftover)
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
