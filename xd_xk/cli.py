"""命令行入口 — 选课 / 退课 / 容量检查."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

from xd_xk.core import add, get_class, login, show_msg

CONF_PATH = Path("conf.json")

# ── 默认扫描的选修课课程号 ─────────────────────────────────────────
_DEFAULT_KCH = [
    "EY226022",
    "EY226023",
    "EY226024",
    "EY226025",
    "EY226026",
    "EY226027",
    "EY226028",
    "EY226029",
    "EY226030",
    "EY226031",
    "EY226032",
    "EY226035",
    "EY226036",
    "EY226037",
    "EY226038",
    "EY226039",
    "EY226041",
    "EY226042",
    "EY226043",
    "EY226044",
    "EY226045",
    "EY226046",
    "EY226047",
]


def _load_conf() -> dict:
    return json.loads(CONF_PATH.read_text(encoding="utf-8"))


def _login_and_fetch(args: argparse.Namespace) -> None:
    """选课 / 退课共用：登录 → 匹配批次 → 获取课程列表."""
    conf = _load_conf()
    data, _cookie = login(conf)
    batch = show_msg(data, batch_name=conf.get("batch_name", "2025级"))
    get_class(data, conf, batch=batch, category=args.category)
    print("[OK] 登录成功，已获取课程列表")


def cmd_check(args: argparse.Namespace) -> None:
    """容量检查 — 循环扫描指定课程号，有余量自动选课."""
    conf = _load_conf()
    data, cookie = login(conf)
    batch = show_msg(data, batch_name=conf.get("batch_name", "2025级"))

    target_kch = set(args.kch) if args.kch else set(_DEFAULT_KCH)
    print(f"开始容量检查，目标课程号：{target_kch}")

    k = 0
    while True:
        k += 1
        rows = get_class(data, conf, batch=batch, category=1)["data"]["rows"]
        for course in rows:
            if course["KCH"] in target_kch and course.get("SFYX") == "0":
                kcm = course["KCM"]
                sel = course.get("numberOfSelected")
                cap = course.get("classCapacity")
                print(kcm, sel, cap)
                if (sel or 0) < (cap or 0):
                    print(course.get("KXH"), course.get("KCM"))
                    add(data, course, cookie, batch, category=1, always=0)
        print(f"第 {k} 次检查{'━' * min(k % 10 or 10, 20)}")
        time.sleep(0.5)


def main() -> None:
    parser = argparse.ArgumentParser(
        prog="xd-xk",
        description="西安电子科技大学自动选课工具",
    )
    sub = parser.add_subparsers(dest="command")

    p_sel = sub.add_parser("select", help="选课")
    p_sel.add_argument("-c", "--category", type=int, default=0, help="0=必修 1=选修")

    p_drop = sub.add_parser("drop", help="退课")
    p_drop.add_argument("-c", "--category", type=int, default=0, help="0=必修 1=选修")

    p_chk = sub.add_parser("check", help="容量检查（循环扫描）")
    p_chk.add_argument("kch", nargs="*", help="课程号列表，不指定则使用默认列表")

    args = parser.parse_args()

    match args.command:
        case "select" | "drop":
            _login_and_fetch(args)
        case "check":
            cmd_check(args)
        case _:
            parser.print_help()


if __name__ == "__main__":
    main()
