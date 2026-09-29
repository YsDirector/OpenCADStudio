#!/usr/bin/env python3
"""发布前卫生扫描：把本机私有标识词从**工作树**与**全历史**里扫一遍。

词表在仓库外（默认 `~/.config/ocsm/hygiene-words.txt`，每行一个词，`#` 起注释）——
**本脚本只写路径、不写任何私有词**。命中只打印 `私#N`（词表里的第 N 个词），
不打印词本身，免得扫描输出（终端记录 / CI 日志 / 报告）二次泄露。

用法（在待发布仓库根目录跑）：
    python3 tools/hygiene_scan.py                  # 工作树（git 已跟踪文件）
    python3 tools/hygiene_scan.py --history        # 再加全历史（对象级：每个对象的原始字节都比对）
    python3 tools/hygiene_scan.py --pattern <n>    # 只查第 n 个词（1 起）
    python3 tools/hygiene_scan.py -i <词表路径>    # 换词表（例如与 junk-names.txt 共用）

退出码：0 = 干净；1 = 有命中；2 = 词表缺失/为空（没词表就别当"已扫"）。

⚠ 发布纪律：任何对外推送（force-push 重写历史也算）之前都必须跑 `--history` 并拿到 0 命中。
"""
from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from hygiene_words import HYGIENE_WORDS_FILE, read_word_file  # noqa: E402


def _git(args: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *args], capture_output=True, text=True)


def _scan_file(path: Path, words: list[str]) -> list[int]:
    """返回该文件里命中的词序号（0 起，去重保序）；二进制/读不了的文件跳过。"""
    try:
        raw = path.read_bytes()
    except OSError:
        return []
    hits: list[int] = []
    for enc in ("utf-8", "gb2312", "utf-16-le"):
        try:
            text = raw.decode(enc, errors="ignore")
        except Exception:
            continue
        for i, word in enumerate(words):
            if word in text and i not in hits:
                hits.append(i)
    return sorted(hits)


def scan_worktree(words: list[str]) -> list[tuple[str, list[int]]]:
    tracked = [p for p in _git(["ls-files", "-z"]).stdout.split("\0") if p]
    out: list[tuple[str, list[int]]] = []
    for name in tracked:
        hits = _scan_file(Path(name), words)
        if hits:
            out.append((name, hits))
    return out


def _enc_variants(word: str) -> list[bytes]:
    """一个词在常见中文编码下的字节形态（按字节比对，绕开解码 errors=ignore 的假合并）。"""
    outs: list[bytes] = []
    for enc in ("utf-8", "gb2312", "utf-16-le"):
        try:
            b = word.encode(enc)
        except Exception:
            continue
        if b and b not in outs:
            outs.append(b)
    return outs


def _scan_bytes(raw: bytes, words: list[str]) -> list[int]:
    """按字节判命中：blob 内容 / tree 文件名 / commit 信息与作者 / tag 信息都适用。"""
    return sorted(i for i, w in enumerate(words)
                  if any(b in raw for b in _enc_variants(w)))


def scan_history(words: list[str]) -> list[tuple[str, str, str, list[int]]]:
    """对象级扫全历史：`cat-file --batch-all-objects` 取出**每个对象**的原始字节再比对。

    为什么不用「逐提交 git grep 文件 + 回读工作树」：旧提交里命中的文件，工作树里
    往往已是干净版本 ⇒ 回读判 0 ⇒ **假阴性**（实测过：旧提交命中却报「全历史 0」）。
    对象级覆盖 blob(文件内容)/tree(文件名)/commit(信息+作者)/tag，并且**连不可达对象一起扫**。
    返回 (对象 sha, 类型, 归属说明, 命中词序号)；只报 sha、不打印提交标题，免得二次泄露。
    """
    listing = _git(["cat-file", "--batch-all-objects",
                    "--batch-check=%(objectname) %(objecttype) %(objectsize)"]).stdout
    objs: list[tuple[str, str, int]] = []
    for line in listing.splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[1] in ("blob", "tree", "commit", "tag"):
            objs.append((parts[0], parts[1], int(parts[2])))

    proc = subprocess.Popen(["git", "cat-file", "--batch"],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    assert proc.stdin is not None and proc.stdout is not None

    def _feed() -> None:
        for sha, _t, _s in objs:
            proc.stdin.write((sha + "\n").encode())
        proc.stdin.close()

    threading.Thread(target=_feed, daemon=True).start()

    out: list[tuple[str, str, str, list[int]]] = []
    for n, (sha, otype, size) in enumerate(objs, 1):
        if not proc.stdout.readline():
            break
        raw = proc.stdout.read(size)
        proc.stdout.read(1)  # 对象之间的换行
        hits = _scan_bytes(raw, words)
        if hits:
            where = ""
            if otype == "blob":
                own = _git(["log", "--all", "--format=%h",
                            "--find-object=" + sha]).stdout.split()
                where = "提交 " + ", ".join(own[:3]) if own else "不可达/无归属"
            out.append((sha, otype, where, hits))
        if n % 5000 == 0:
            print(f"  …已扫 {n}/{len(objs)} 个对象", file=sys.stderr)
    proc.wait()
    return out


def _fmt(hits: list[int], offset: int) -> str:
    return ",".join(f"私#{i + 1 + offset}" for i in hits)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="发布前卫生扫描（词表在仓库外）")
    ap.add_argument("-i", "--words", default=str(HYGIENE_WORDS_FILE),
                    help=f"词表路径（默认 {HYGIENE_WORDS_FILE}）")
    ap.add_argument("--history", action="store_true", help="连全历史一起扫（逐提交）")
    ap.add_argument("--pattern", type=int, default=None, help="只查第 n 个词（1 起）")
    args = ap.parse_args(argv)

    path = Path(args.words)
    words = read_word_file(path)
    if not words:
        print(f"✗ 词表缺失或为空：{path}", file=sys.stderr)
        print("  建法：每行一个词，'#' 起注释，权限 600，放仓库外（如 ~/.config/ocsm/）", file=sys.stderr)
        return 2
    if args.pattern is not None:
        if not 1 <= args.pattern <= len(words):
            print(f"✗ --pattern 越界（词表 {len(words)} 个词）", file=sys.stderr)
            return 2
        words = [words[args.pattern - 1]]

    total = 0
    wt = scan_worktree(words)
    print(f"词表 : {path}（{len(words)} 个词）")
    print(f"工作树: {len(wt)} 个文件命中")
    for name, hits in wt:
        print(f"  {name}  {_fmt(hits, 0)}")
    total += len(wt)

    if args.history:
        hist = scan_history(words)
        print(f"全历史: {len(hist)} 个对象命中")
        for sha, otype, where, hits in hist:
            tail = f"  ← {where}" if where else ""
            print(f"  {sha[:10]}  {otype}  {_fmt(hits, 0)}{tail}")
        total += len(hist)

    print("结果 : " + ("✓ 干净（0 命中）" if total == 0 else f"✗ {total} 处命中，禁止发布/推送"))
    return 0 if total == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
