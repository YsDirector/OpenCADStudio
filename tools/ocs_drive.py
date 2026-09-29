#!/usr/bin/env python3
"""用 OCS 自带的自动化端口驱动「正在运行的」OpenCADStudio GUI（本机 OCSM 验证用）。

★ 风险边界（默认不猜目标实例）：本脚本驱动的是「发现到的现有实例」——而现有实例通常就是
  用户正在用的那个 GUI，run / input / save / capture 会**直接打进那张正在编辑的图**。
  所以默认**拒绝执行**，必须显式指定目标之一：
      --session <session_id>   只驱动这一个实例（最精确，推荐）
      --allow-existing         允许驱动「发现到的现有实例」（与 tools/ocs_session.py 的
                               OCS_ALLOW_EXISTING=1 同名同义）；多个实例时取第一个并打印提醒
  没有任何显式指定 ⇒ 大声警告 + 拒绝执行（退出码 2），绝不静默连。
  只列出实例（本地读描述符、一次连接都不发）用：`tools/ocs_drive.py list`。

OpenCADStudio 每个实例会在 `~/.config/OpenCADStudio/automation/<session>.json`
写一份描述文件（session_id / pid / port / token），并在 127.0.0.1:<port> 上
一条连接一条 JSON 请求（协议 1）。本脚本读描述文件 → 发请求 → 打印应答。

用法:
    python3 tools/ocs_drive.py list                      # 只列实例（不连接，无需许可）
    python3 tools/ocs_drive.py --session <id> state      # 显式指定：列该实例（含 pid/文档）
    python3 tools/ocs_drive.py --allow-existing run "OCSM"   # 显式许可才驱动现有实例
    python3 tools/ocs_drive.py --session <id> run "ZOOM ALL"
    python3 tools/ocs_drive.py --session <id> capture /tmp/x.png   # 抓应用窗口 PNG
    python3 tools/ocs_drive.py --allow-existing save               # 保存当前文档

退出码：0 正常 / 1 请求被拒或失败 / 2 被守卫拒绝（缺显式许可，见上）。
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import socket
import sys
import uuid
from pathlib import Path

DESC_DIR = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "OpenCADStudio" / "automation"

REFUSE = """★ 拒绝执行：ocs_drive.py 默认不猜目标实例 —— 它会驱动「发现到的现有实例」，
  而 run/input/save/capture 会直接打进那张正在编辑的图（通常就是用户正在画的那张）。
  请显式指定其一：
    tools/ocs_drive.py --session <session_id> <op> ...   # 只驱动这一个实例（推荐）
    tools/ocs_drive.py --allow-existing <op> ...         # 或 export OCS_ALLOW_EXISTING=1
  只列实例（本地读描述符、不连接）用：tools/ocs_drive.py list
  现有实例描述符：%s/*.json""" % DESC_DIR


def _flag(name: str) -> bool:
    """与 tools/ocs_session.py 同款开关解析（OCS_ALLOW_EXISTING=1/true/yes/on）。"""
    return os.environ.get(name, "").strip().lower() in ("1", "true", "yes", "on")


ALLOW_EXISTING_ENV = _flag("OCS_ALLOW_EXISTING")


def descriptors() -> list[dict]:
    out = []
    for path in sorted(glob.glob(str(DESC_DIR / "*.json"))):
        try:
            d = json.loads(Path(path).read_text())
        except Exception:
            continue
        d["_file"] = path
        out.append(d)
    return out


def alive(pid: int) -> bool:
    return Path(f"/proc/{pid}").exists()


def exchange(desc: dict, request: dict, timeout: float = 20.0) -> dict:
    req = dict(request)
    req.setdefault("protocol", 1)
    req.setdefault("request_id", uuid.uuid4().hex)   # 变更类 op 必须带唯一 id
    req["session_id"] = desc["session_id"]
    req["token"] = desc["token"]
    with socket.create_connection(("127.0.0.1", desc["port"]), timeout=timeout) as s:
        s.sendall((json.dumps(req, ensure_ascii=False) + "\n").encode("utf-8"))
        s.shutdown(socket.SHUT_WR)
        buf = b""
        while not buf.endswith(b"\n"):
            chunk = s.recv(1 << 20)
            if not chunk:
                break
            buf += chunk
    return json.loads(buf.decode("utf-8", errors="replace")) if buf else {}


def probe(desc: dict) -> dict | None:
    """只认「描述文件 + 端口 + session_id 三者对得上」的实例。

    描述文件在进程退出后不会自动删（实测残留几十份），pid 还可能被别的进程
    复用，所以必须真连一次 `state` 验活，不能只看 /proc。
    """
    try:
        st = exchange(desc, {"op": "state"}, timeout=2.0)
    except Exception:
        return None
    if st.get("ok") and st.get("session_id") == desc["session_id"]:
        return st
    return None


def candidates(pid: int | None, session: str | None = None) -> list[tuple[dict, dict]]:
    """匹配 --pid / --session 的**活**实例（描述符 + 真实 state 应答）。

    ★ 只做「发现 + 过滤 + 验活」，不替调用者决定连谁；发现不到就 SystemExit（明确报错，
      绝不静默新建实例、也不乱连）。
    """
    found = descriptors()
    matched = found
    if session:
        exact = [d for d in found if d["session_id"] == session]
        if exact:
            matched = exact
        else:
            matched = [d for d in found if d["session_id"].startswith(session)]
            if len(matched) > 1:
                raise SystemExit("--session %s 前缀匹配多个实例：%s（请给完整 session_id）"
                                 % (session, "、".join(d["session_id"] for d in matched)))
    if pid is not None:
        matched = [d for d in matched if d["pid"] == pid]
    target = " --session %s" % session if session else (" --pid %s" % pid if pid is not None else "")
    suffix = target + " " if target else ""
    if not matched:
        raise SystemExit("没有匹配%s的 OCS 自动化实例（%s 里现有 %d 份描述符；用 tools/ocs_drive.py "
                         "list 列出现有实例）" % (suffix, DESC_DIR, len(found)))
    live: list[tuple[dict, dict]] = []
    for d in matched:
        if not alive(d["pid"]):
            continue
        st = probe(d)
        if st:
            live.append((d, st))
    if not live:
        raise SystemExit("匹配%s的 %d 个实例都不活/不应答（描述符残留？；本脚本不会自己新建实例、"
                         "也不会乱连；先启动 OpenCADStudio GUI，或用 tools/ocs_session.py 起隔离实例）"
                         % (suffix, len(matched)))
    return live


def pick(pid: int | None, session: str | None = None, quiet: bool = False) -> tuple[dict, dict]:
    live = candidates(pid, session)
    if len(live) > 1 and not quiet:
        print(f"（{len(live)} 个实例，取 pid={live[0][0]['pid']}；可用 --session / --pid 精确指定）",
              file=sys.stderr)
    return live[0]


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        epilog="默认不猜目标：不给 --session / --allow-existing（或 OCS_ALLOW_EXISTING=1）时拒绝执行"
               "（rc=2），只有 list 不需要许可。风险：run/input/save/capture 会直接打进那张正在编辑的图。")
    ap.add_argument("op", choices=["state", "list", "run", "capture", "save", "open", "input", "raw"])
    ap.add_argument("arg", nargs="?")
    ap.add_argument("--pid", type=int, default=None)
    ap.add_argument("--allow-existing", action="store_true",
                    help="★ 允许驱动「发现到的现有实例」（可能是用户正在编辑的 GUI）："
                         "run/input/save/capture 会直接打进那张正在编辑的图；等价于 OCS_ALLOW_EXISTING=1")
    ap.add_argument("--session", default=None, metavar="ID",
                    help="★ 显式指定 session_id（或其唯一前缀）：本次只驱动这一个实例")
    ap.add_argument("--json", action="store_true", help="原样打印应答 JSON")
    ap.add_argument("--doc", type=int, default=None, help="document_id（变更类 op 必填）")
    args = ap.parse_args(argv)

    if args.op == "list":
        # ★ 唯一不需要显式许可的 op：只读本地描述符，一次连接都不发。
        for d in descriptors():
            print(f"pid={d['pid']:<8} alive={alive(d['pid'])} port={d['port']} session={d['session_id'][:12]}…")
        return 0

    # ── 守卫：默认不猜目标 —— 除了上面的 list，其它 op 都会连接实例，
    #    所以都要求显式许可（--allow-existing / OCS_ALLOW_EXISTING=1）或显式 --session。
    if not (ALLOW_EXISTING_ENV or args.allow_existing or args.session):
        print(REFUSE, file=sys.stderr)
        return 2
    if args.allow_existing or ALLOW_EXISTING_ENV:
        print("# ★ 逃生态（--allow-existing / OCS_ALLOW_EXISTING=1）：允许连现有实例，"
              "run/input/save/capture 会直接打进那张正在编辑的图", file=sys.stderr)
    if args.session:
        print("# 显式指定 --session %s：本次只驱动这一个实例" % args.session, file=sys.stderr)

    if args.op == "state":
        # state 也要求「发现不到就明确报错」——不许静默返回空、给人「好像没事」的假成功。
        for d, st in candidates(args.pid, args.session):
            docs = st.get("documents", [])
            print(f"pid={d['pid']} mode={st.get('mode')} enabled={st.get('enabled')} "
                  f"doc={[x['title'] for x in docs]}")
        return 0


    desc, st = pick(args.pid, args.session)
    doc_id = args.doc
    if doc_id is None and args.op not in ("state", "list", "raw"):
        docs = [d for d in st.get("documents", []) if not d.get("start")]
        if len(docs) == 1:
            doc_id = docs[0]["id"]
        else:
            print("多个文档，请用 --doc 指定：", [(d["id"], d["path"]) for d in st.get("documents", [])],
                  file=sys.stderr)
    if args.op == "run":
        req = {"op": "run", "cmd": args.arg}
    elif args.op == "capture":
        req = {"op": "capture", "path": args.arg}
    elif args.op == "save":
        req = {"op": "save"} if not args.arg else {"op": "save", "path": args.arg}
    elif args.op == "open":
        req = {"op": "open", "path": args.arg}
    elif args.op == "input":
        # 交互命令喂字符串（如 INSERT 的块名/插入点、属性的 8 个值）；空串 = 空格
        req = {"op": "input", "kind": "token", "text": " " if args.arg == "" else args.arg}
    else:
        req = json.loads(args.arg)
    if doc_id is not None:
        req["document_id"] = doc_id
    reply = exchange(desc, req)
    print(json.dumps(reply, ensure_ascii=False, indent=2) if args.json else
          json.dumps(reply, ensure_ascii=False))
    return 0 if reply.get("ok", True) else 1


if __name__ == "__main__":
    sys.exit(main())
