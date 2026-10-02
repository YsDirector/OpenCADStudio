#!/usr/bin/env python3
"""无头 open E2E 夹具：占住一份 OCS 编辑锁（真 flock），模拟「另一个实例正打开这份图」。

sidecar 路径/内容与 `src/io/edit_lock.rs` 同构（`.{文件名}.ocs.lock`，文本含 `pid=…`）；
pid 写自己的（活着的进程），一直持有到被杀。用法：hold_lock.py <目标 dxf> [秒数]。
"""
import fcntl, os, sys, time

target = os.path.abspath(sys.argv[1])
lock = os.path.join(os.path.dirname(target), "." + os.path.basename(target) + ".ocs.lock")
with open(lock, "w", encoding="utf-8") as fh:
    fh.write("Open CAD Studio pid=%d path=%s\n" % (os.getpid(), target))
    fh.flush()
    fcntl.flock(fh.fileno(), fcntl.LOCK_EX)
    print(os.getpid(), flush=True)
    time.sleep(int(sys.argv[2]) if len(sys.argv) > 2 else 600)
