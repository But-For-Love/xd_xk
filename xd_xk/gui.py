"""
西安电子科技大学 自动选课工具 — Fluent Design GUI.

Usage:  python -m xd_xk.gui
"""

from __future__ import annotations

import json
import queue
import threading
import time
import tkinter as tk
from dataclasses import dataclass
from pathlib import Path
from tkinter import messagebox
from typing import Any

import ttkbootstrap as ttk
from ttkbootstrap.constants import *

from xd_xk.core import CourseSession, add, dele, get_class, login

CONF_PATH = Path("conf.json")

# ═══════════════════════════════════════════════════════════════════
#  消息协议（值对象 — 替代裸 tuple）
# ═══════════════════════════════════════════════════════════════════


@dataclass(frozen=True)
class Msg:
    """线程间消息协议，替代易出错的 ("kind", payload) 裸元组."""

    kind: str  # "log" | "err" | "done" | "st" | "ok"
    payload: object = ""


# ═══════════════════════════════════════════════════════════════════
#  配色 · Fluent 色板
# ═══════════════════════════════════════════════════════════════════


class C:
    """Fluent Design 色板（深色日志区 + 浅色控件区）."""

    BG = "#f3f3f3"
    CARD = "#ffffff"
    CARD_BORDER = "#e5e5e5"
    TEXT = "#1a1a1a"
    TEXT_SEC = "#616161"
    TEXT_DIS = "#a0a0a0"
    ACCENT = "#0078d4"
    ACCENT_HOVER = "#106ebe"
    SUCCESS = "#0f7b0f"
    DANGER = "#c42b1c"
    WARNING = "#9d5d00"
    LOG_BG = "#1b1b1b"
    LOG_FG = "#cccccc"
    LOG_SEL = "#264f78"
    LOG_SUCCESS = "#4ec9b0"
    LOG_ERROR = "#f44747"
    LOG_WARN = "#e5c07b"


# ═══════════════════════════════════════════════════════════════════
#  数据层 · JSON 配置读写
# ═══════════════════════════════════════════════════════════════════

_DEFAULT_CONF: dict[str, Any] = {
    "ocr_captcha": "1",
    "debug": "0",
    "batch_name": "第二轮补选（国际创新周）",
    "bx_or_xx": 0,
    "bx": [],
    "xx": [],
    "data": {"loginname": "", "password": "", "captcha": "xxxx", "uuid": "xxxx"},
}


def load_conf() -> dict[str, Any]:
    """从 conf.json 加载配置."""
    try:
        return json.loads(CONF_PATH.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError):
        return dict(_DEFAULT_CONF)


def save_conf(
    loginname: str,
    password: str,
    ocr: bool,
    debug: bool,
    courses: list[dict[str, Any]],
    batch_name: str = "第二轮补选（国际创新周）",
) -> None:
    """保存配置到 conf.json."""
    bx = [
        {"KCH": c["KCH"], "KXH": c.get("KXH", ""), "KCM": c.get("KCM", "")}
        for c in courses
        if c["category"] == 0
    ]
    xx = [
        {"KCH": c["KCH"], "KXH": c.get("KXH", ""), "KCM": c.get("KCM", "")}
        for c in courses
        if c["category"] == 1
    ]
    conf = {
        "ocr_captcha": "1" if ocr else "0",
        "debug": "1" if debug else "0",
        "batch_name": batch_name,
        "bx_or_xx": 0,
        "bx": bx,
        "xx": xx,
        "data": {
            "loginname": loginname,
            "password": password,
            "captcha": "xxxx",
            "uuid": "xxxx",
        },
    }
    CONF_PATH.write_text(json.dumps(conf, ensure_ascii=False, indent=2), encoding="utf-8")


# ═══════════════════════════════════════════════════════════════════
#  工具函数
# ═══════════════════════════════════════════════════════════════════


def _center_win(child: tk.Toplevel, parent: tk.Widget) -> None:
    """让 child 窗口居中于 parent."""
    child.update_idletasks()
    w, h = child.winfo_width(), child.winfo_height()
    x = parent.winfo_x() + (parent.winfo_width() - w) // 2
    y = parent.winfo_y() + (parent.winfo_height() - h) // 2
    child.geometry(f"+{x}+{y}")


# ═══════════════════════════════════════════════════════════════════
#  工作流辅助（消除 5 处重复的登录→匹配→拉课程）
# ═══════════════════════════════════════════════════════════════════


def _prepare_session(
    conf: dict[str, Any],
    msg_q: queue.Queue,
    log_cb: Any,
) -> CourseSession | None:
    """登录→匹配批次，通过 msg_q 报告进度.

    此前这一流程在 _w_sel / _w_drop / _w_chk / _w_snipe / _fetch
    中重复了 5 次，现在统一为一行调用.
    """
    msg_q.put(Msg("log", "正在登录…"))
    try:
        return CourseSession.create(conf, log_func=log_cb)
    except RuntimeError:
        raise  # 由调用方统一 try/except
    except Exception as e:
        raise RuntimeError(f"准备会话出错：{type(e).__name__}: {e}")


def _fetch_courses(
    session: CourseSession,
    conf: dict[str, Any],
    categories: set[int],
    msg_q: queue.Queue,
) -> dict[int, list[dict]]:
    """拉取指定类别的课程列表."""
    rows_by_cat: dict[int, list[dict]] = {}
    for cat in categories:
        cat_name = "必修" if cat == 0 else "选修"
        msg_q.put(Msg("log", f"正在获取{cat_name}课程列表…"))
        resp = get_class(session.data, conf, batch=session.batch_code, category=cat)
        rows = resp.get("data", {}).get("rows", [])
        rows_by_cat[cat] = rows
        msg_q.put(Msg("log", f"  {cat_name}：{len(rows)} 门"))
        api_code = resp.get("code", "?")
        api_msg = resp.get("msg", "")
        if api_code != 200:
            msg_q.put(Msg("log", f"  [WARN] API code={api_code}, msg={api_msg}"))
        sample = [r.get("KCH", "?") for r in rows[:5]]
        msg_q.put(Msg("log", f"  API 返回课程号示例：{sample}"))
    return rows_by_cat


# ═══════════════════════════════════════════════════════════════════
#  对话框 · 添加课程
# ═══════════════════════════════════════════════════════════════════


class AddCourseDialog(tk.Toplevel):
    """手动输入课程号 / 课序号."""

    def __init__(self, parent: tk.Widget) -> None:
        super().__init__(parent)
        self.title("添加课程")
        self.resizable(False, False)
        self.grab_set()
        self.result: dict[str, Any] | None = None
        self.configure(bg=C.BG)

        self._build()
        _center_win(self, parent)

        self.protocol("WM_DELETE_WINDOW", self._cancel)
        self.bind("<Return>", lambda e: self._ok())
        self.bind("<Escape>", lambda e: self._cancel())

    # ── UI ──

    def _build(self) -> None:
        pad = {"padx": 16, "pady": 8}

        # 类别
        lf = ttk.Labelframe(self, text="  课程类别  ", bootstyle="info")
        lf.pack(fill=X, **pad)

        self.cat_var = tk.IntVar(value=1)
        ttk.Radiobutton(
            lf,
            text="必修（方案内课程）",
            variable=self.cat_var,
            value=0,
            bootstyle="info",
        ).pack(side=LEFT, padx=12, pady=8)
        ttk.Radiobutton(
            lf,
            text="选修",
            variable=self.cat_var,
            value=1,
            bootstyle="info",
        ).pack(side=LEFT, padx=12, pady=8)

        # 课程信息
        lf2 = ttk.Labelframe(self, text="  课程信息  ", bootstyle="info")
        lf2.pack(fill=X, **pad)

        r0 = ttk.Frame(lf2)
        r0.pack(fill=X, **pad)
        ttk.Label(r0, text="课程号", width=8).pack(side=LEFT)
        self.kch = ttk.Entry(r0, width=24)
        self.kch.pack(side=LEFT)
        self.kch.focus()

        r1 = ttk.Frame(lf2)
        r1.pack(fill=X, **pad)
        ttk.Label(r1, text="课序号", width=8).pack(side=LEFT)
        self.kxh = ttk.Entry(r1, width=24)
        self.kxh.pack(side=LEFT)
        ttk.Label(r1, text="必修必填，选修可留空", foreground=C.TEXT_SEC).pack(side=LEFT, padx=8)

        r2 = ttk.Frame(lf2)
        r2.pack(fill=X, **pad)
        ttk.Label(r2, text="课程名", width=8).pack(side=LEFT)
        self.kcm = ttk.Entry(r2, width=24)
        self.kcm.pack(side=LEFT)
        ttk.Label(r2, text="选填，仅用于显示", foreground=C.TEXT_SEC).pack(side=LEFT, padx=8)

        ttk.Label(
            self,
            text="课程号示例：TE204003　课序号示例：02（小卡片左上角 [01]）",
            foreground=C.TEXT_DIS,
            font=("", 8),
        ).pack(padx=16, pady=(0, 4))

        # 按钮
        bf = ttk.Frame(self)
        bf.pack(fill=X, padx=16, pady=(4, 16))
        ttk.Button(bf, text="取消", bootstyle="secondary", command=self._cancel).pack(
            side=RIGHT, padx=(6, 0)
        )
        ttk.Button(bf, text="确定", bootstyle="info", command=self._ok).pack(side=RIGHT)

    # ── 事件 ──

    def _ok(self) -> None:
        kch = self.kch.get().strip()
        kxh = self.kxh.get().strip()
        kcm = self.kcm.get().strip()
        if not kch:
            messagebox.showwarning("提示", "请填写课程号", parent=self)
            return
        if self.cat_var.get() == 0 and not kxh:
            messagebox.showwarning("提示", "必修课必须填写课序号", parent=self)
            return
        self.result = {
            "category": self.cat_var.get(),
            "KCH": kch,
            "KXH": kxh,
            "KCM": kcm,
        }
        self.destroy()

    def _cancel(self) -> None:
        self.result = None
        self.destroy()


# ═══════════════════════════════════════════════════════════════════
#  对话框 · 浏览课程
# ═══════════════════════════════════════════════════════════════════

_COLS = ("sel", "cat", "KCH", "KXH", "KCM", "teacher", "cap")
_HEAD = ("✔", "类别", "课程号", "课序号", "课程名", "教师", "已选/容量")


class CourseBrowserDialog(tk.Toplevel):
    """从服务器拉取课程列表，勾选后添加到选课池."""

    def __init__(
        self,
        parent: tk.Widget,
        conf: dict[str, Any],
        unfilled_only: bool = False,
    ) -> None:
        super().__init__(parent)
        batch = conf.get("batch_name", "（未设置）")
        self.title(f"浏览课程 — 批次：{batch} — 正在加载…")
        self.geometry("1050x750")
        self.minsize(900, 600)
        self.grab_set()
        self.configure(bg=C.BG)
        self.selected_courses: list[dict[str, Any]] = []

        self._all: list[tuple] = []
        self._conf = conf
        self._q: queue.Queue = queue.Queue()
        self._unfilled_only = unfilled_only
        self._log_lines: list[str] = []

        self._build()
        _center_win(self, parent)

        self.protocol("WM_DELETE_WINDOW", self._close)
        self.bind("<Escape>", lambda e: self._close())

        threading.Thread(target=self._fetch, daemon=True).start()
        self._poll()

    # ── UI ──

    def _build(self) -> None:
        # 搜索栏
        top = ttk.Frame(self)
        top.pack(fill=X, padx=12, pady=(12, 4))

        ttk.Label(top, text="🔍").pack(side=LEFT)
        self.search_var = tk.StringVar()
        self.search_var.trace_add("write", lambda *_: self._filter())
        se = ttk.Entry(top, textvariable=self.search_var, width=32)
        se.pack(side=LEFT, padx=(4, 8))
        ttk.Label(
            top,
            text="课程号 / 课程名 / 教师",
            foreground=C.TEXT_DIS,
            font=("", 9),
        ).pack(side=LEFT)

        self.v_unfilled = tk.BooleanVar(value=self._unfilled_only)
        ttk.Checkbutton(
            top,
            text="只显示未满课程",
            variable=self.v_unfilled,
            bootstyle="info",
            command=self._apply_filter,
        ).pack(side=LEFT, padx=(16, 0))

        self.status = ttk.Label(top, text="正在加载…", foreground=C.TEXT_SEC, font=("", 9))
        self.status.pack(side=RIGHT)

        # 表格
        tf = ttk.Frame(self)
        tf.pack(fill=BOTH, expand=True, padx=12, pady=4)

        self.tree = ttk.Treeview(
            tf,
            columns=_COLS,
            show="headings",
            selectmode="extended",
        )
        for c, h in zip(_COLS, _HEAD):
            self.tree.heading(c, text=h)

        self.tree.column("sel", width=32, anchor=CENTER, stretch=False)
        self.tree.column("cat", width=52, anchor=CENTER, stretch=False)
        self.tree.column("KCH", width=96, anchor=CENTER)
        self.tree.column("KXH", width=56, anchor=CENTER, stretch=False)
        self.tree.column("KCM", width=220, anchor=W)
        self.tree.column("teacher", width=130, anchor=W)
        self.tree.column("cap", width=80, anchor=CENTER, stretch=False)

        vs = ttk.Scrollbar(tf, orient=VERTICAL, command=self.tree.yview)
        hs = ttk.Scrollbar(tf, orient=HORIZONTAL, command=self.tree.xview)
        self.tree.configure(yscrollcommand=vs.set, xscrollcommand=hs.set)
        self.tree.grid(row=0, column=0, sticky="nsew")
        vs.grid(row=0, column=1, sticky="ns")
        hs.grid(row=1, column=0, sticky="ew")
        tf.rowconfigure(0, weight=1)
        tf.columnconfigure(0, weight=1)

        self.tree.heading("sel", command=self._toggle_all)
        self.tree.bind("<ButtonRelease-1>", self._click)

        # 底栏
        bf = ttk.Frame(self)
        bf.pack(fill=X, padx=12, pady=(4, 12))
        self.sel_lbl = ttk.Label(bf, text="已选 0 门", foreground=C.TEXT_SEC, font=("", 9))
        self.sel_lbl.pack(side=LEFT)

        ttk.Button(bf, text="添加到选课池", bootstyle="info", command=self._confirm).pack(
            side=RIGHT, padx=(6, 0)
        )
        ttk.Button(bf, text="取消", bootstyle="secondary", command=self._close).pack(
            side=RIGHT, padx=(6, 0)
        )
        ttk.Button(
            bf,
            text="全不选",
            bootstyle="secondary-outline",
            command=self._mark_none,
        ).pack(side=RIGHT, padx=(6, 0))
        ttk.Button(
            bf,
            text="全选",
            bootstyle="secondary-outline",
            command=self._mark_all,
        ).pack(side=RIGHT)

    # ── 后台拉取（使用 _prepare_session 消除重复）──

    def _fetch(self) -> None:
        try:
            session = _prepare_session(
                self._conf,
                self._q,
                log_cb=lambda m: self._q.put(Msg("st", m)),
            )
            if session is None:
                return
            self._q.put(Msg("st", f"已匹配批次 code：{session.batch_code}，正在获取课程…"))

            rows: list[tuple[int, dict]] = []
            for cat in (0, 1):
                self._q.put(Msg("st", f"正在获取{'必修' if cat == 0 else '选修'}课程…"))
                for course in (
                    get_class(session.data, self._conf, batch=session.batch_code, category=cat)
                    .get("data", {})
                    .get("rows", [])
                ):
                    rows.append((cat, course))
            self._q.put(Msg("ok", rows))
        except RuntimeError as e:
            self._q.put(Msg("err", str(e)))
        except Exception as e:
            self._q.put(Msg("err", f"{type(e).__name__}: {e}"))

    def _poll(self) -> None:
        try:
            while True:
                m: Msg = self._q.get_nowait()
                if m.kind == "st":
                    self.status.config(text=str(m.payload))
                    self._log_lines.append(str(m.payload))
                    if len(self._log_lines) > 20:
                        self._log_lines.pop(0)
                elif m.kind == "err":
                    self.status.config(text="加载失败", foreground=C.DANGER)
                    detail = "\n".join(self._log_lines[-10:]) if self._log_lines else ""
                    msg = f"{m.payload}\n\n--- 近期日志 ---\n{detail}" if detail else str(m.payload)
                    messagebox.showerror("加载失败", msg, parent=self)
                elif m.kind == "ok":
                    self._fill(m.payload)  # type: ignore[arg-type]
                    batch = self._conf.get("batch_name", "")
                    self.title(f"浏览课程 — 批次：{batch} — 勾选后添加到选课池")
        except queue.Empty:
            pass
        if self.winfo_exists():
            self.after(80, self._poll)

    def _fill(self, rows: list[tuple[int, dict]]) -> None:
        self._all = []
        for cat, course in rows:
            ct = "必修" if cat == 0 else "选修"
            self._all.append(
                (
                    "",
                    ct,
                    course.get("KCH", ""),
                    course.get("KXH", ""),
                    course.get("KCM", ""),
                    course.get("SKJS", ""),
                    f"{course.get('numberOfSelected', '?')}/{course.get('classCapacity', '?')}",
                    cat,
                )
            )
        self._filter()

    def _refresh(self, rows: list[tuple]) -> None:
        self.tree.delete(*self.tree.get_children())
        for r in rows:
            self.tree.insert("", END, values=r[:7], tags=(str(r[7]),))

    # ── 搜索 / 过滤 ──

    def _apply_filter(self) -> None:
        self._filter()

    def _filter(self) -> None:
        rows = self._all
        if self.v_unfilled.get():
            rows = [r for r in rows if self._is_unfilled(r)]
        kw = self.search_var.get().strip().lower()
        if kw:
            rows = [
                r for r in rows if kw in r[2].lower() or kw in r[4].lower() or kw in r[5].lower()
            ]
        self._refresh(rows)
        if not self._all:
            return
        if len(rows) == len(self._all):
            self.status.config(text=f"共 {len(self._all)} 门课程", foreground=C.SUCCESS)
        else:
            self.status.config(
                text=f"显示 {len(rows)}/{len(self._all)} 门课程",
                foreground=C.SUCCESS,
            )

    @staticmethod
    def _is_unfilled(r: tuple) -> bool:
        try:
            parts = str(r[6]).split("/")
            return int(parts[0]) < int(parts[1])
        except (ValueError, IndexError):
            return True

    # ── 勾选 ──

    def _click(self, e: tk.Event) -> None:
        rid = self.tree.identify_row(e.y)
        col = self.tree.identify_column(e.x)
        if not rid or col != "#1":
            return
        v = list(self.tree.item(rid, "values"))
        v[0] = "" if v[0] == "✔" else "✔"
        self.tree.item(rid, values=v)
        self._count()

    def _toggle_all(self) -> None:
        items = self.tree.get_children()
        if not items:
            return
        mark = "" if all(self.tree.item(i, "values")[0] == "✔" for i in items) else "✔"
        for i in items:
            v = list(self.tree.item(i, "values"))
            v[0] = mark
            self.tree.item(i, values=v)
        self._count()

    def _mark_all(self) -> None:
        for i in self.tree.get_children():
            v = list(self.tree.item(i, "values"))
            v[0] = "✔"
            self.tree.item(i, values=v)
        self._count()

    def _mark_none(self) -> None:
        for i in self.tree.get_children():
            v = list(self.tree.item(i, "values"))
            v[0] = ""
            self.tree.item(i, values=v)
        self._count()

    def _count(self) -> None:
        n = sum(1 for i in self.tree.get_children() if self.tree.item(i, "values")[0] == "✔")
        self.sel_lbl.config(text=f"已选 {n} 门")

    # ── 确认 / 取消 ──

    def _confirm(self) -> None:
        self.selected_courses = []
        for i in self.tree.get_children():
            v = self.tree.item(i, "values")
            if v[0] == "✔":
                self.selected_courses.append(
                    {
                        "category": 0 if v[1] == "必修" else 1,
                        "KCH": v[2],
                        "KXH": v[3],
                        "KCM": v[4],
                    }
                )
        self.destroy()

    def _close(self) -> None:
        self.selected_courses = []
        self.destroy()


# ═══════════════════════════════════════════════════════════════════
#  对话框 · 选择选课批次
# ═══════════════════════════════════════════════════════════════════


class BatchSelectDialog(tk.Toplevel):
    """显示可选批次列表，用户点击选择."""

    def __init__(self, parent: tk.Widget, batches: list[dict[str, str]]) -> None:
        super().__init__(parent)
        self.title("选择选课批次")
        self.resizable(False, False)
        self.grab_set()
        self.configure(bg=C.BG)
        self.selected: str | None = None

        self._batches = batches
        self._build()
        _center_win(self, parent)

        self.protocol("WM_DELETE_WINDOW", self._cancel)
        self.bind("<Escape>", lambda e: self._cancel())

    def _build(self) -> None:
        ttk.Label(
            self,
            text="点击选择要使用的选课批次：",
            foreground=C.TEXT_SEC,
            font=("", 10),
        ).pack(padx=16, pady=(16, 8))

        frame = ttk.Frame(self)
        frame.pack(fill=BOTH, expand=True, padx=16, pady=(0, 8))

        for b in self._batches:
            name = b["name"]
            can = b["canSelect"] == "1"
            state_text = "已开放" if can else "未开放"
            btn_text = f"{name}  [{state_text}]"

            row = ttk.Frame(frame)
            row.pack(fill=X, pady=2)
            btn = ttk.Button(
                row,
                text=btn_text,
                bootstyle="info-outline" if can else "secondary-outline",
                command=lambda n=name: self._pick(n),
            )
            btn.pack(side=LEFT, fill=X, expand=True)
            if not can:
                btn.config(state=DISABLED)

        bf = ttk.Frame(self)
        bf.pack(fill=X, padx=16, pady=(4, 16))
        ttk.Button(bf, text="取消", bootstyle="secondary", command=self._cancel).pack(side=RIGHT)

    def _pick(self, name: str) -> None:
        self.selected = name
        self.destroy()

    def _cancel(self) -> None:
        self.selected = None
        self.destroy()


# ═══════════════════════════════════════════════════════════════════
#  主界面
# ═══════════════════════════════════════════════════════════════════


class Application:
    """主应用程序."""

    TREE_COLS = ("category", "KCH", "KXH", "KCM")
    TREE_HEADS = ("类别", "课程号", "课序号", "课程名")

    def __init__(self, root: ttk.Window) -> None:
        self.root = root
        self.root.title("西电自动选课工具")
        self.root.geometry("960x860")
        self.root.minsize(880, 760)

        self.msg_q: queue.Queue = queue.Queue()
        self.stop_ev = threading.Event()
        self.running = False
        self._pending_err: str | None = None

        self._apply_style()
        self._build_login()
        self._build_courses()
        self._build_actions()
        self._build_log()
        self._load_conf()
        self._poll()

        self.root.protocol("WM_DELETE_WINDOW", self._quit)

    # ────────────── 样式 ──────────────

    def _apply_style(self) -> None:
        s = self.root.style
        s.configure("Treeview", rowheight=30, font=("", 10))
        s.configure("Treeview.Heading", font=("", 10, "bold"))

    # ────────────── 登录区 ──────────────

    def _build_login(self) -> None:
        card = ttk.Labelframe(self.root, text="  登录信息  ", bootstyle="info")
        card.pack(fill=X, padx=16, pady=(16, 8))

        r1 = ttk.Frame(card)
        r1.pack(fill=X, padx=16, pady=(12, 4))
        ttk.Label(r1, text="学号").pack(side=LEFT)
        self.v_user = tk.StringVar()
        ttk.Entry(r1, textvariable=self.v_user, width=20).pack(side=LEFT, padx=(4, 20))

        ttk.Label(r1, text="密码").pack(side=LEFT)
        self.v_pass = tk.StringVar()
        ttk.Entry(r1, textvariable=self.v_pass, width=20, show="•").pack(
            side=LEFT,
            padx=(4, 20),
        )

        self.v_ocr = tk.BooleanVar(value=True)
        ttk.Checkbutton(
            r1,
            text="自动验证码",
            variable=self.v_ocr,
            bootstyle="info",
        ).pack(side=LEFT, padx=(0, 12))
        self.v_dbg = tk.BooleanVar(value=False)
        ttk.Checkbutton(
            r1,
            text="调试",
            variable=self.v_dbg,
            bootstyle="secondary",
        ).pack(side=LEFT)

        r2 = ttk.Frame(card)
        r2.pack(fill=X, padx=16, pady=(4, 12))
        ttk.Label(r2, text="选课批次").pack(side=LEFT)
        self.v_batch = tk.StringVar(value="第一轮正选（国际创新周）")
        ttk.Entry(r2, textvariable=self.v_batch, width=42).pack(side=LEFT, padx=(4, 8))
        ttk.Button(
            r2,
            text="获取批次",
            bootstyle="info-outline",
            command=self._fetch_batches,
        ).pack(side=LEFT, padx=(0, 8))
        ttk.Label(
            r2,
            text="匹配批次名称关键字",
            foreground=C.TEXT_DIS,
            font=("", 9),
        ).pack(side=LEFT)

    # ────────────── 课程列表 ──────────────

    def _build_courses(self) -> None:
        card = ttk.Labelframe(self.root, text="  选课池  ", bootstyle="info")
        card.pack(fill=BOTH, expand=True, padx=16, pady=8)

        tf = ttk.Frame(card)
        tf.pack(fill=BOTH, expand=True, padx=8, pady=(8, 0))
        self.tree = ttk.Treeview(
            tf,
            columns=self.TREE_COLS,
            show="headings",
            selectmode="extended",
        )
        for c, h in zip(self.TREE_COLS, self.TREE_HEADS):
            self.tree.heading(c, text=h)
        self.tree.column("category", width=60, anchor=CENTER, stretch=False)
        self.tree.column("KCH", width=130, anchor=CENTER)
        self.tree.column("KXH", width=70, anchor=CENTER, stretch=False)
        self.tree.column("KCM", width=240, anchor=W)

        sb = ttk.Scrollbar(tf, orient=VERTICAL, command=self.tree.yview)
        self.tree.configure(yscrollcommand=sb.set)
        self.tree.pack(side=LEFT, fill=BOTH, expand=True)
        sb.pack(side=RIGHT, fill=Y)

        self.tree.bind("<Double-1>", self._edit_course)

        # 按钮行
        bf = ttk.Frame(card)
        bf.pack(fill=X, padx=8, pady=8)
        ttk.Button(
            bf,
            text="＋ 添加课程",
            bootstyle="info-outline",
            command=self._add_course,
        ).pack(side=LEFT, padx=(0, 6))
        ttk.Button(
            bf,
            text="－ 删除选中",
            bootstyle="danger-outline",
            command=self._del_course,
        ).pack(side=LEFT, padx=(0, 6))
        ttk.Button(
            bf,
            text="全选",
            bootstyle="secondary-outline",
            command=self._sel_all,
        ).pack(side=LEFT, padx=(0, 6))
        ttk.Button(
            bf,
            text="全不选",
            bootstyle="secondary-outline",
            command=self._sel_none,
        ).pack(side=LEFT, padx=(0, 6))
        ttk.Button(
            bf,
            text="📋 浏览课程",
            bootstyle="info-outline",
            command=self._browse,
        ).pack(side=LEFT, padx=(0, 6))
        ttk.Button(
            bf,
            text="⚡ 添加未满课程",
            bootstyle="success-outline",
            command=self._browse_unfilled,
        ).pack(side=LEFT)
        self.lbl_cnt = ttk.Label(bf, text="共 0 门课", foreground=C.TEXT_SEC, font=("", 9))
        self.lbl_cnt.pack(side=RIGHT)

    # ────────────── 操作栏 ──────────────

    def _build_actions(self) -> None:
        f = ttk.Frame(self.root)
        f.pack(fill=X, padx=16, pady=4)

        self.btn_sel = ttk.Button(
            f,
            text="选　课",
            bootstyle="success",
            command=self._start_sel,
        )
        self.btn_sel.pack(side=LEFT, padx=(0, 6))
        self.btn_drop = ttk.Button(
            f,
            text="退　课",
            bootstyle="danger",
            command=self._start_drop,
        )
        self.btn_drop.pack(side=LEFT, padx=(0, 6))
        self.btn_chk = ttk.Button(
            f,
            text="容量检查",
            bootstyle="warning",
            command=self._start_chk,
        )
        self.btn_chk.pack(side=LEFT, padx=(0, 12))
        self.btn_stop = ttk.Button(
            f,
            text="■ 停止",
            bootstyle="secondary",
            command=self._stop,
            state=DISABLED,
        )
        self.btn_stop.pack(side=LEFT)

        self.v_always = tk.BooleanVar(value=True)
        ttk.Checkbutton(
            f,
            text="连续重试",
            variable=self.v_always,
            bootstyle="secondary-round-toggle",
        ).pack(side=LEFT, padx=(16, 0))

        # 捡漏区
        sep = ttk.Separator(f, orient=VERTICAL)
        sep.pack(side=LEFT, fill=Y, padx=(16, 12))
        self.v_snipe_cat = tk.StringVar(value="选修")
        self.cb_snipe_cat = ttk.Combobox(
            f,
            textvariable=self.v_snipe_cat,
            values=["必修", "选修"],
            state="readonly",
            width=6,
        )
        self.cb_snipe_cat.pack(side=LEFT, padx=(0, 6))
        self.btn_snipe = ttk.Button(
            f,
            text="🎯 捡漏",
            bootstyle="success",
            command=self._start_snipe,
        )
        self.btn_snipe.pack(side=LEFT)

    # ────────────── 日志区 ──────────────

    def _build_log(self) -> None:
        card = ttk.Labelframe(self.root, text="  日志  ", bootstyle="secondary")
        card.pack(fill=BOTH, padx=16, pady=(4, 8), expand=False)

        self.log = tk.Text(
            card,
            height=18,
            wrap="word",
            font=("Cascadia Code", 9),
            bg=C.LOG_BG,
            fg=C.LOG_FG,
            insertbackground=C.LOG_FG,
            selectbackground=C.LOG_SEL,
            relief="flat",
            bd=0,
            padx=10,
            pady=8,
        )
        sb = ttk.Scrollbar(card, orient=VERTICAL, command=self.log.yview)
        self.log.configure(yscrollcommand=sb.set)
        self.log.pack(side=LEFT, fill=BOTH, expand=True, padx=(4, 0), pady=4)
        sb.pack(side=RIGHT, fill=Y, padx=(0, 4), pady=4)

        self.log.tag_configure("info", foreground=C.LOG_FG)
        self.log.tag_configure("success", foreground=C.LOG_SUCCESS)
        self.log.tag_configure("error", foreground=C.LOG_ERROR)
        self.log.tag_configure("warn", foreground=C.LOG_WARN)

        bf = ttk.Frame(self.root)
        bf.pack(fill=X, padx=16, pady=(0, 12))
        ttk.Button(
            bf,
            text="清除日志",
            bootstyle="secondary-outline",
            command=self._clear_log,
        ).pack(side=RIGHT)

    # ════════════════════════════════════════════════════════════════
    #  课程管理
    # ════════════════════════════════════════════════════════════════

    def _add_course(self) -> None:
        d = AddCourseDialog(self.root)
        self.root.wait_window(d)
        if d.result:
            r = d.result
            ct = "必修" if r["category"] == 0 else "选修"
            self.tree.insert("", END, values=(ct, r["KCH"], r["KXH"], r["KCM"]))
            self._cnt()
            self._log(f"已添加：{r['KCH']} {r['KXH']}（{ct}）")

    def _browse(self, unfilled_only: bool = False) -> None:
        conf = self._mk_conf()
        if not conf["data"]["loginname"] or not conf["data"]["password"]:
            messagebox.showwarning("提示", "请先填写学号和密码")
            return
        self._save()
        self._log(f"浏览课程 — 批次：{conf.get('batch_name', '（未设置）')}")
        d = CourseBrowserDialog(self.root, conf, unfilled_only=unfilled_only)
        self.root.wait_window(d)
        seen = {
            (self.tree.item(i, "values")[1], self.tree.item(i, "values")[2])
            for i in self.tree.get_children()
        }
        n = 0
        for c in d.selected_courses:
            if (c["KCH"], c["KXH"]) in seen:
                continue
            ct = "必修" if c["category"] == 0 else "选修"
            self.tree.insert("", END, values=(ct, c["KCH"], c["KXH"], c["KCM"]))
            seen.add((c["KCH"], c["KXH"]))
            n += 1
        if n:
            self._cnt()
            self._log(f"从课程列表添加了 {n} 门课程")

    def _browse_unfilled(self) -> None:
        self._browse(unfilled_only=True)

    def _del_course(self) -> None:
        sel = self.tree.selection()
        if not sel:
            return
        for it in sel:
            v = self.tree.item(it, "values")
            self.tree.delete(it)
            self._log(f"已删除：{v[1]} {v[2]}")
        self._cnt()

    def _sel_all(self) -> None:
        self.tree.selection_set(self.tree.get_children())

    def _sel_none(self) -> None:
        self.tree.selection_remove(self.tree.get_children())

    def _edit_course(self, e: tk.Event) -> None:
        it = self.tree.identify_row(e.y)
        if not it:
            return
        v = self.tree.item(it, "values")
        d = AddCourseDialog(self.root)
        d.cat_var.set(0 if v[0] == "必修" else 1)
        d.kch.insert(0, v[1])
        d.kxh.insert(0, v[2])
        d.kcm.insert(0, v[3])
        self.root.wait_window(d)
        if d.result:
            r = d.result
            ct = "必修" if r["category"] == 0 else "选修"
            self.tree.item(it, values=(ct, r["KCH"], r["KXH"], r["KCM"]))
            self._log(f"已更新：{r['KCH']} {r['KXH']}")

    def _courses(self) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        for it in self.tree.get_children():
            v = self.tree.item(it, "values")
            out.append(
                {
                    "category": 0 if v[0] == "必修" else 1,
                    "KCH": v[1],
                    "KXH": v[2],
                    "KCM": v[3],
                }
            )
        return out

    def _cnt(self) -> None:
        self.lbl_cnt.config(text=f"共 {len(self.tree.get_children())} 门课")

    # ════════════════════════════════════════════════════════════════
    #  配置持久化
    # ════════════════════════════════════════════════════════════════

    def _load_conf(self) -> None:
        c = load_conf()
        self.v_user.set(c["data"].get("loginname", ""))
        self.v_pass.set(c["data"].get("password", ""))
        self.v_ocr.set(c.get("ocr_captcha", "1") == "1")
        self.v_dbg.set(c.get("debug", "0") == "1")
        self.v_batch.set(c.get("batch_name", "第一轮正选（国际创新周）"))
        for x in c.get("bx", []):
            self.tree.insert(
                "",
                END,
                values=("必修", x["KCH"], x.get("KXH", ""), x.get("KCM", "")),
            )
        for x in c.get("xx", []):
            self.tree.insert(
                "",
                END,
                values=("选修", x["KCH"], x.get("KXH", ""), x.get("KCM", "")),
            )
        self._cnt()

    def _save(self) -> None:
        save_conf(
            self.v_user.get().strip(),
            self.v_pass.get().strip(),
            self.v_ocr.get(),
            self.v_dbg.get(),
            self._courses(),
            self.v_batch.get().strip(),
        )

    def _fetch_batches(self) -> None:
        conf = self._mk_conf()
        if not conf["data"]["loginname"] or not conf["data"]["password"]:
            messagebox.showwarning("提示", "请先填写学号和密码")
            return
        self._log("正在获取选课批次…")
        threading.Thread(target=self._w_fetch_batches, args=(conf,), daemon=True).start()

    def _w_fetch_batches(self, conf: dict[str, Any]) -> None:
        try:
            jd, ck = login(conf, log_func=self._log_cb)
            student = jd["data"]["student"]
            lst = student.get("electiveBatchList", [])
            if not lst:
                self.msg_q.put(Msg("err", "没有可用的选课批次"))
                return
            self.root.after(0, self._show_batch_dialog, lst)
        except RuntimeError as e:
            self._log_err(e)
        except Exception as e:
            self._log_err(f"获取批次出错：{type(e).__name__}: {e}")

    def _show_batch_dialog(self, batches: list[dict[str, str]]) -> None:
        d = BatchSelectDialog(self.root, batches)
        self.root.wait_window(d)
        if d.selected:
            self.v_batch.set(d.selected)
            self._log(f"已选择批次：{d.selected}")

    def _mk_conf(self) -> dict[str, Any]:
        return {
            "ocr_captcha": "1" if self.v_ocr.get() else "0",
            "debug": "1" if self.v_dbg.get() else "0",
            "batch_name": self.v_batch.get().strip(),
            "bx_or_xx": 0,
            "bx": [],
            "xx": [],
            "data": {
                "loginname": self.v_user.get().strip(),
                "password": self.v_pass.get().strip(),
                "captcha": "xxxx",
                "uuid": "xxxx",
            },
        }

    # ════════════════════════════════════════════════════════════════
    #  日志
    # ════════════════════════════════════════════════════════════════

    def _log(self, msg: str, tag: str = "info") -> None:
        self.log.insert(END, msg + "\n", tag)
        self.log.see(END)

    def _log_cb(self, msg: str) -> None:
        self.msg_q.put(Msg("log", msg))

    def _log_err(self, msg: object) -> None:
        self.msg_q.put(Msg("err", str(msg)))

    def _clear_log(self) -> None:
        self.log.delete("1.0", END)

    def _poll(self) -> None:
        try:
            while True:
                m: Msg = self.msg_q.get_nowait()
                if m.kind == "log":
                    self._dispatch_log(str(m.payload))
                elif m.kind == "err":
                    self._log("[ERR] " + str(m.payload), "error")
                    self._pending_err = str(m.payload)
                elif m.kind == "done":
                    self.running = False
                    self.btn_stop.config(state=DISABLED)
                    self._enable()
                    self._log("─" * 40)
                    if self._pending_err:
                        messagebox.showerror("操作失败", self._pending_err)
                        self._pending_err = None
        except queue.Empty:
            pass
        self.root.after(80, self._poll)

    def _dispatch_log(self, s: str) -> None:
        """根据日志内容自动选择标签颜色."""
        if "操作成功" in s or "已在选课结果" in s or "[OK]" in s:
            t = "success"
        elif "失败" in s or "错误" in s or "不存在" in s:
            t = "error"
        elif "冲突" in s:
            t = "warn"
        else:
            t = "info"
        self._log(s, t)

    # ════════════════════════════════════════════════════════════════
    #  按钮状态
    # ════════════════════════════════════════════════════════════════

    def _disable(self) -> None:
        for b in (self.btn_sel, self.btn_drop, self.btn_chk, self.btn_snipe):
            b.config(state=DISABLED)
        self.cb_snipe_cat.config(state=DISABLED)

    def _enable(self) -> None:
        for b in (self.btn_sel, self.btn_drop, self.btn_chk, self.btn_snipe):
            b.config(state=NORMAL)
        self.cb_snipe_cat.config(state="readonly")

    # ════════════════════════════════════════════════════════════════
    #  选课（使用 _prepare_session 消除重复）
    # ════════════════════════════════════════════════════════════════

    def _start_sel(self) -> None:
        conf = self._mk_conf()
        if not conf["data"]["loginname"] or not conf["data"]["password"]:
            messagebox.showwarning("提示", "请先填写学号和密码")
            return
        cs = self._courses()
        if not cs:
            messagebox.showwarning("提示", "请先添加课程")
            return
        self._save()
        self.stop_ev.clear()
        self.running = True
        self._disable()
        self.btn_stop.config(state=NORMAL)
        self._log("── 开始选课 ──")
        a = 1 if self.v_always.get() else 0
        threading.Thread(target=self._w_sel, args=(conf, cs, a), daemon=True).start()

    def _w_sel(
        self,
        conf: dict[str, Any],
        cs: list[dict[str, Any]],
        always: int,
    ) -> None:
        try:
            session = _prepare_session(conf, self.msg_q, self._log_cb)
            if session is None:
                return
            self.msg_q.put(Msg("log", f"选课批次 code：{session.batch_code}"))

            need_cats = {c["category"] for c in cs}
            rows_by_cat = _fetch_courses(session, conf, need_cats, self.msg_q)

            for c in cs:
                if self.stop_ev.is_set():
                    self.msg_q.put(Msg("log", "用户停止操作"))
                    break
                rows = rows_by_cat.get(c["category"], [])
                found = False
                for course in rows:
                    if course["KCH"] == c["KCH"]:
                        if c["category"] == 0:
                            for j in course.get("tcList", []):
                                if j["KXH"] == c["KXH"]:
                                    add(
                                        session.data,
                                        j,
                                        cookie=session.cookie,
                                        batch=session.batch_code,
                                        always=always,
                                        category=0,
                                        log_func=self._log_cb,
                                        stop_event=self.stop_ev,
                                    )
                                    found = True
                                    break
                        else:
                            add(
                                session.data,
                                course,
                                cookie=session.cookie,
                                batch=session.batch_code,
                                always=always,
                                category=1,
                                log_func=self._log_cb,
                                stop_event=self.stop_ev,
                            )
                            found = True
                        break
                if not found:
                    self.msg_q.put(
                        Msg(
                            "log",
                            f"未找到课程 {c['KCH']} {c['KXH']} ｜"
                            f"该类别共 {len(rows)} 门，"
                            f"要找的 KCH={c['KCH']}，"
                            f"API 返回的 KCH 列表前5："
                            f"{[r.get('KCH', '?') for r in rows[:5]]}",
                        )
                    )
        except RuntimeError as e:
            self._log_err(e)
        except Exception as e:
            self._log_err(f"选课出错：{type(e).__name__}: {e}")
        finally:
            self.msg_q.put(Msg("done"))

    # ════════════════════════════════════════════════════════════════
    #  退课
    # ════════════════════════════════════════════════════════════════

    def _start_drop(self) -> None:
        conf = self._mk_conf()
        if not conf["data"]["loginname"] or not conf["data"]["password"]:
            messagebox.showwarning("提示", "请先填写学号和密码")
            return
        cs = self._courses()
        if not cs:
            messagebox.showwarning("提示", "请先添加课程")
            return
        self._save()
        self.stop_ev.clear()
        self.running = True
        self._disable()
        self.btn_stop.config(state=NORMAL)
        self._log("── 开始退课 ──")
        a = 1 if self.v_always.get() else 0
        threading.Thread(target=self._w_drop, args=(conf, cs, a), daemon=True).start()

    def _w_drop(
        self,
        conf: dict[str, Any],
        cs: list[dict[str, Any]],
        always: int,
    ) -> None:
        try:
            session = _prepare_session(conf, self.msg_q, self._log_cb)
            if session is None:
                return
            self.msg_q.put(Msg("log", f"选课批次 code：{session.batch_code}"))

            need_cats = {c["category"] for c in cs}
            rows_by_cat = _fetch_courses(session, conf, need_cats, self.msg_q)

            for c in cs:
                if self.stop_ev.is_set():
                    self.msg_q.put(Msg("log", "用户停止操作"))
                    break
                rows = rows_by_cat.get(c["category"], [])
                found = False
                for course in rows:
                    if course["KCH"] == c["KCH"]:
                        if c["category"] == 0:
                            for j in course.get("tcList", []):
                                if j["KXH"] == c["KXH"]:
                                    dele(
                                        session.data,
                                        j,
                                        cookie=session.cookie,
                                        batch=session.batch_code,
                                        always=always,
                                        category=0,
                                        log_func=self._log_cb,
                                        stop_event=self.stop_ev,
                                    )
                                    found = True
                                    break
                        else:
                            dele(
                                session.data,
                                course,
                                cookie=session.cookie,
                                batch=session.batch_code,
                                always=always,
                                category=1,
                                log_func=self._log_cb,
                                stop_event=self.stop_ev,
                            )
                            found = True
                        break
                if not found:
                    self.msg_q.put(
                        Msg(
                            "log",
                            f"未找到课程 {c['KCH']} {c['KXH']} ｜"
                            f"该类别共 {len(rows)} 门，"
                            f"要找的 KCH={c['KCH']}，"
                            f"API 返回的 KCH 列表前5："
                            f"{[r.get('KCH', '?') for r in rows[:5]]}",
                        )
                    )
        except RuntimeError as e:
            self._log_err(e)
        except Exception as e:
            self._log_err(f"退课出错：{type(e).__name__}: {e}")
        finally:
            self.msg_q.put(Msg("done"))

    # ════════════════════════════════════════════════════════════════
    #  容量检查
    # ════════════════════════════════════════════════════════════════

    def _start_chk(self) -> None:
        conf = self._mk_conf()
        if not conf["data"]["loginname"] or not conf["data"]["password"]:
            messagebox.showwarning("提示", "请先填写学号和密码")
            return
        cs = self._courses()
        if not cs:
            messagebox.showwarning("提示", "请先添加课程")
            return
        self._save()
        self.stop_ev.clear()
        self.running = True
        self._disable()
        self.btn_stop.config(state=NORMAL)
        self._log("── 开始容量检查（有空位自动选课）──")
        threading.Thread(target=self._w_chk, args=(conf, cs), daemon=True).start()

    def _w_chk(
        self,
        conf: dict[str, Any],
        cs: list[dict[str, Any]],
    ) -> None:
        try:
            session = _prepare_session(conf, self.msg_q, self._log_cb)
            if session is None:
                return
            self.msg_q.put(Msg("log", f"选课批次 code：{session.batch_code}"))
            kset = {c["KCH"] for c in cs}
            k = 0
            while not self.stop_ev.is_set():
                k += 1
                rows = (
                    get_class(session.data, conf, batch=session.batch_code, category=0)
                    .get("data", {})
                    .get("rows", [])
                )
                for course in rows:
                    if course["KCH"] in kset and course.get("SFYX") == "0":
                        sel = int(course.get("numberOfSelected", 0))
                        cap = int(course.get("classCapacity", 0))
                        self.msg_q.put(Msg("log", f"{course['KCM']}　已选/容量：{sel}/{cap}"))
                        if sel < cap:
                            self.msg_q.put(
                                Msg(
                                    "log",
                                    f"  ✦ 发现空位 → {course['KXH']} {course['KCM']}",
                                )
                            )
                            add(
                                session.data,
                                course,
                                session.cookie,
                                session.batch_code,
                                category=1,
                                always=0,
                                log_func=self._log_cb,
                                stop_event=self.stop_ev,
                            )
                self.msg_q.put(Msg("log", f"第 {k} 次检查{'━' * min(k, 20)}"))
                k = k % 10
                time.sleep(0.5)
        except RuntimeError as e:
            self._log_err(e)
        except Exception as e:
            self._log_err(f"容量检查出错：{type(e).__name__}: {e}")
        finally:
            self.msg_q.put(Msg("done"))

    # ════════════════════════════════════════════════════════════════
    #  捡漏
    # ════════════════════════════════════════════════════════════════

    def _start_snipe(self) -> None:
        conf = self._mk_conf()
        if not conf["data"]["loginname"] or not conf["data"]["password"]:
            messagebox.showwarning("提示", "请先填写学号和密码")
            return
        self._save()
        self.stop_ev.clear()
        self.running = True
        self._disable()
        self.btn_stop.config(state=NORMAL)
        cat = 0 if self.v_snipe_cat.get() == "必修" else 1
        self._log(f"── 开始捡漏（{self.v_snipe_cat.get()}，5~10s 一轮）──")
        threading.Thread(target=self._w_snipe, args=(conf, cat), daemon=True).start()

    def _w_snipe(self, conf: dict[str, Any], cat: int) -> None:
        try:
            session = _prepare_session(conf, self.msg_q, self._log_cb)
            if session is None:
                return
            self.msg_q.put(Msg("log", f"选课批次 code：{session.batch_code}"))

            added: set[str] = set()
            k = 0
            while not self.stop_ev.is_set():
                k += 1
                rows = (
                    get_class(session.data, conf, batch=session.batch_code, category=cat)
                    .get("data", {})
                    .get("rows", [])
                )
                found_any = False

                if cat == 1:
                    for course in rows:
                        if self.stop_ev.is_set():
                            break
                        sel = int(course.get("numberOfSelected", 0))
                        cap = int(course.get("classCapacity", 0))
                        if sel >= cap:
                            continue
                        key = course.get("KCH", "")
                        if key in added:
                            continue
                        found_any = True
                        self.msg_q.put(
                            Msg(
                                "log",
                                f"  ✦ 发现空位 → {course.get('KCM', '')} ({sel}/{cap})",
                            )
                        )
                        add(
                            session.data,
                            course,
                            session.cookie,
                            session.batch_code,
                            category=1,
                            always=0,
                            log_func=self._log_cb,
                            stop_event=self.stop_ev,
                        )
                        added.add(key)
                else:
                    for course in rows:
                        if self.stop_ev.is_set():
                            break
                        for j in course.get("tcList", []):
                            if self.stop_ev.is_set():
                                break
                            sel = int(j.get("numberOfSelected", 0))
                            cap = int(j.get("classCapacity", 0))
                            if sel >= cap:
                                continue
                            key = f"{j.get('KCH', '')}|{j.get('KXH', '')}"
                            if key in added:
                                continue
                            found_any = True
                            self.msg_q.put(
                                Msg(
                                    "log",
                                    f"  ✦ 发现空位 → {j.get('KCM', '')} "
                                    f"{j.get('KXH', '')} ({sel}/{cap})",
                                )
                            )
                            add(
                                session.data,
                                j,
                                session.cookie,
                                session.batch_code,
                                category=0,
                                always=0,
                                log_func=self._log_cb,
                                stop_event=self.stop_ev,
                            )
                            added.add(key)

                status = f"已抢 {len(added)} 门" if added else "暂无空位"
                self.msg_q.put(Msg("log", f"第 {k} 轮检查 ━ {status} ━{'━' * min(k, 20)}"))
                k = k % 10

                time.sleep(8 if not found_any and not added else 5)

        except RuntimeError as e:
            self._log_err(e)
        except Exception as e:
            self._log_err(f"捡漏出错：{type(e).__name__}: {e}")
        finally:
            self.msg_q.put(Msg("done"))

    # ════════════════════════════════════════════════════════════════
    #  停止 / 退出
    # ════════════════════════════════════════════════════════════════

    def _stop(self) -> None:
        if self.running:
            self.stop_ev.set()
            self._log("正在停止…")

    def _quit(self) -> None:
        if self.running:
            if not messagebox.askyesno("确认", "任务正在运行，确定退出？"):
                return
            self.stop_ev.set()
        self._save()
        self.root.destroy()


# ═══════════════════════════════════════════════════════════════════
#  入口
# ═══════════════════════════════════════════════════════════════════


def main() -> None:
    """启动 GUI."""
    root = ttk.Window(
        title="西电自动选课工具",
        themename="cosmo",
        size=(960, 860),
        minsize=(880, 760),
    )
    _app = Application(root)
    root.mainloop()


if __name__ == "__main__":
    main()
