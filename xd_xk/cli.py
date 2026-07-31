"""命令行入口 — 选课 / 退课 / 容量检查."""

from __future__ import annotations

import argparse
import json
import tempfile
import time
from importlib.metadata import version as pkg_version
from pathlib import Path

from PIL import Image as PILImage

from xd_xk.core import CourseSession, add, get_class

CONF_PATH = Path("conf.json")

_EPILOG = """\
示例用法:
  xd-xk select                    选课（必修，FANKC）
  xd-xk select -c 1               选课（选修，XGKC）
  xd-xk drop                      退课（必修）
  xd-xk drop -c 1                 退课（选修）
  xd-xk check                     容量检查（使用 conf.json 中的选修课列表）
  xd-xk check EY226022 EY226023   容量检查（指定课程号）

配置文件:
  当前目录下需要有 conf.json，包含学号、密码、批次名称、课程列表等信息。
  首次使用请先手动创建或从模板复制。"""


def _load_conf() -> dict:
    return json.loads(CONF_PATH.read_text(encoding="utf-8"))


def _manual_captcha(img_bytes: bytes) -> str:
    """用系统图片查看器显示验证码，等待用户手动输入（仅 CLI 使用）."""
    with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
        f.write(img_bytes)
        tmp_path = f.name
    img = PILImage.open(tmp_path)
    img.show()
    code = input("请输入验证码: ")
    Path(tmp_path).unlink(missing_ok=True)
    return code


def _login_and_fetch(args: argparse.Namespace) -> None:
    """选课 / 退课共用：登录 → 匹配批次 → 获取课程列表."""
    conf = _load_conf()
    session = CourseSession.create(conf, captcha_cb=_manual_captcha)
    get_class(session.data, conf, batch=session.batch_code, category=args.category)
    print("[OK] 登录成功，已获取课程列表")


def _courses_from_conf(conf: dict) -> dict[int, set[str]]:
    """从 conf.json 中提取所有课程号，按分类分组."""
    by_cat: dict[int, set[str]] = {}
    for c in conf.get("bx", []):
        if c.get("KCH"):
            by_cat.setdefault(0, set()).add(c["KCH"])
    for c in conf.get("xx", []):
        if c.get("KCH"):
            by_cat.setdefault(1, set()).add(c["KCH"])
    return by_cat


def cmd_check(args: argparse.Namespace) -> None:
    """容量检查 — 循环扫描指定课程号，有余量自动选课."""
    conf = _load_conf()
    session = CourseSession.create(conf, captcha_cb=_manual_captcha)

    if args.kch:
        by_cat = {1: set(args.kch)}  # 命令行指定的当作选修处理
    else:
        by_cat = _courses_from_conf(conf)
        if not by_cat:
            print("错误：未指定课程号，且 conf.json 的 bx / xx 列表均为空。")
            print("请在 conf.json 中添加课程信息，或通过命令行参数指定课程号。")
            return

    all_kch = {kch for kset in by_cat.values() for kch in kset}
    print(f"开始容量检查，目标课程号：{all_kch}")

    k = 0
    while True:
        k += 1
        for cat, target_kch in by_cat.items():
            rows = get_class(
                session.data, conf, batch=session.batch_code, category=cat,
            )["data"]["rows"]
            for course in rows:
                if course["KCH"] in target_kch and course.get("SFYX") == "0":
                    kcm = course["KCM"]
                    sel = course.get("numberOfSelected")
                    cap = course.get("classCapacity")
                    print(kcm, sel, cap)
                    if (sel or 0) < (cap or 0):
                        print(course.get("KXH"), course.get("KCM"))
                        add(
                            session.data, course, session.cookie,
                            session.batch_code, category=cat, always=0,
                        )
        print(f"第 {k} 次检查{'━' * min(k % 10 or 10, 20)}")
        time.sleep(0.5)


def main() -> None:
    parser = argparse.ArgumentParser(
        prog="xd-xk",
        description="西安电子科技大学 (XDU) 自动选课工具 —— 登录教务系统，自动完成选课、退课与容量监控。",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=_EPILOG,
    )
    parser.add_argument(
        "-V", "--version",
        action="version",
        version=f"xd-xk {pkg_version('xd-xk')}",
        help="显示版本号并退出",
    )
    sub = parser.add_subparsers(
        dest="command",
        title="可用命令",
        description="选择要执行的操作：",
    )

    p_sel = sub.add_parser(
        "select",
        help="自动选课 — 登录后获取课程列表并提交选课请求",
        description="登录教务系统，匹配选课批次，自动获取课程列表并提交选课请求。",
        epilog="示例: xd-xk select           # 必修课选课\n      xd-xk select -c 1      # 选修课选课",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p_sel.add_argument(
        "-c", "--category",
        type=int,
        default=0,
        choices=[0, 1],
        metavar="CATEGORY",
        help="课程类别：0=必修课（FANKC，默认），1=选修课（XGKC）",
    )

    p_drop = sub.add_parser(
        "drop",
        help="自动退课 — 登录后获取已选课程并提交退课请求",
        description="登录教务系统，匹配选课批次，获取已选课程列表并提交退课请求。",
        epilog="示例: xd-xk drop              # 必修课退课\n      xd-xk drop -c 1         # 选修课退课",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p_drop.add_argument(
        "-c", "--category",
        type=int,
        default=0,
        choices=[0, 1],
        metavar="CATEGORY",
        help="课程类别：0=必修课（TJKC，默认），1=选修课（XGKC）",
    )

    p_chk = sub.add_parser(
        "check",
        help="容量检查 — 持续监控课程余量，有空位时自动抢课",
        description=(
            "循环扫描选修课列表，监控每门课已选人数与课容量。"
            "当发现某门课已选人数 < 课容量（即有空位），自动提交选课请求。"
            "按 Ctrl+C 停止。"
        ),
        epilog=(
            "示例: xd-xk check                              # 使用 conf.json 中的选修课列表\n"
            "      xd-xk check EY226022 EY226023             # 只监控指定课程号"
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p_chk.add_argument(
        "kch",
        nargs="*",
        metavar="KCH",
        help="要监控的课程号（KCH）列表，多个用空格分隔。不指定则使用 conf.json 中 xx 列表的课程号",
    )

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
