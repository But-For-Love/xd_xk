# -*- coding: utf-8 -*-
"""
西安电子科技大学 自动选课工具 — Fluent Design GUI
"""
import tkinter as tk
from tkinter import messagebox
import ttkbootstrap as ttk
from ttkbootstrap.constants import *
import json
import threading
import queue
import time

from func import login, show_msg, get_class, add, dele


# ═══════════════════════════════════════════════════════════════
#  配色 · Fluent 色板
# ═══════════════════════════════════════════════════════════════

class C:
    """Fluent Design 色板（深色日志区 + 浅色控件区）"""
    BG           = "#f3f3f3"
    CARD         = "#ffffff"
    CARD_BORDER  = "#e5e5e5"
    TEXT         = "#1a1a1a"
    TEXT_SEC     = "#616161"
    TEXT_DIS     = "#a0a0a0"
    ACCENT       = "#0078d4"
    ACCENT_HOVER = "#106ebe"
    SUCCESS      = "#0f7b0f"
    DANGER       = "#c42b1c"
    WARNING      = "#9d5d00"
    LOG_BG       = "#1b1b1b"
    LOG_FG       = "#cccccc"
    LOG_SEL      = "#264f78"
    LOG_SUCCESS  = "#4ec9b0"
    LOG_ERROR    = "#f44747"
    LOG_WARN     = "#e5c07b"


# ═══════════════════════════════════════════════════════════════
#  数据层 · JSON 配置读写
# ═══════════════════════════════════════════════════════════════

_DEFAULT_CONF = {
    "ocr_captcha": "1", "debug": "0",
    "batch_name": "第一轮正选（国际创新周）",
    "bx_or_xx": 0, "bx": [], "xx": [],
    "data": {"loginname": "", "password": "", "captcha": "xxxx", "uuid": "xxxx"},
}


def load_conf():
    try:
        with open("conf.json", "r", encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return dict(_DEFAULT_CONF)


def save_conf(loginname, password, ocr, debug, courses,
              batch_name="第一轮正选（国际创新周）"):
    bx = [{"KCH": c["KCH"], "KXH": c.get("KXH", ""), "KCM": c.get("KCM", "")}
          for c in courses if c["category"] == 0]
    xx = [{"KCH": c["KCH"], "KCM": c.get("KCM", "")}
          for c in courses if c["category"] == 1]
    conf = {
        "ocr_captcha": "1" if ocr else "0",
        "debug": "1" if debug else "0",
        "batch_name": batch_name,
        "bx_or_xx": 0, "bx": bx, "xx": xx,
        "data": {"loginname": loginname, "password": password,
                 "captcha": "xxxx", "uuid": "xxxx"},
    }
    with open("conf.json", "w", encoding="utf-8") as f:
        json.dump(conf, f, ensure_ascii=False, indent=2)


# ═══════════════════════════════════════════════════════════════
#  工具函数
# ═══════════════════════════════════════════════════════════════

def _center_win(child, parent):
    """让 child 窗口居中于 parent"""
    child.update_idletasks()
    w, h = child.winfo_width(), child.winfo_height()
    x = parent.winfo_x() + (parent.winfo_width()  - w) // 2
    y = parent.winfo_y() + (parent.winfo_height() - h) // 2
    child.geometry(f"+{x}+{y}")


# ═══════════════════════════════════════════════════════════════
#  对话框 · 添加课程
# ═══════════════════════════════════════════════════════════════

class AddCourseDialog(tk.Toplevel):
    """手动输入课程号 / 课序号"""

    def __init__(self, parent):
        super().__init__(parent)
        self.title("添加课程")
        self.resizable(False, False)
        self.grab_set()
        self.result = None
        self.configure(bg=C.BG)

        self._build()
        _center_win(self, parent)

        self.protocol("WM_DELETE_WINDOW", self._cancel)
        self.bind("<Return>", lambda e: self._ok())
        self.bind("<Escape>", lambda e: self._cancel())

    # ── UI ──

    def _build(self):
        pad = {"padx": 16, "pady": 8}

        # 类别
        lf = ttk.Labelframe(self, text="  课程类别  ", bootstyle="info")
        lf.pack(fill=X, **pad)

        self.cat_var = tk.IntVar(value=1)
        ttk.Radiobutton(lf, text="必修（方案内课程）",
                        variable=self.cat_var, value=0,
                        bootstyle="info").pack(side=LEFT, padx=12, pady=8)
        ttk.Radiobutton(lf, text="选修",
                        variable=self.cat_var, value=1,
                        bootstyle="info").pack(side=LEFT, padx=12, pady=8)

        # 课程信息
        lf2 = ttk.Labelframe(self, text="  课程信息  ", bootstyle="info")
        lf2.pack(fill=X, **pad)

        r0 = ttk.Frame(lf2); r0.pack(fill=X, **pad)
        ttk.Label(r0, text="课程号", width=8).pack(side=LEFT)
        self.kch = ttk.Entry(r0, width=24); self.kch.pack(side=LEFT); self.kch.focus()

        r1 = ttk.Frame(lf2); r1.pack(fill=X, **pad)
        ttk.Label(r1, text="课序号", width=8).pack(side=LEFT)
        self.kxh = ttk.Entry(r1, width=24); self.kxh.pack(side=LEFT)
        ttk.Label(r1, text="必修必填，选修可留空",
                  foreground=C.TEXT_SEC).pack(side=LEFT, padx=8)

        r2 = ttk.Frame(lf2); r2.pack(fill=X, **pad)
        ttk.Label(r2, text="课程名", width=8).pack(side=LEFT)
        self.kcm = ttk.Entry(r2, width=24); self.kcm.pack(side=LEFT)
        ttk.Label(r2, text="选填，仅用于显示",
                  foreground=C.TEXT_SEC).pack(side=LEFT, padx=8)

        ttk.Label(self, text="课程号示例：TE204003　课序号示例：02（小卡片左上角 [01]）",
                  foreground=C.TEXT_DIS, font=("", 8)).pack(padx=16, pady=(0, 4))

        # 按钮
        bf = ttk.Frame(self); bf.pack(fill=X, padx=16, pady=(4, 16))
        ttk.Button(bf, text="取消", bootstyle="secondary",
                   command=self._cancel).pack(side=RIGHT, padx=(6, 0))
        ttk.Button(bf, text="确定", bootstyle="info",
                   command=self._ok).pack(side=RIGHT)

    # ── 事件 ──

    def _ok(self):
        kch = self.kch.get().strip()
        kxh = self.kxh.get().strip()
        kcm = self.kcm.get().strip()
        if not kch:
            messagebox.showwarning("提示", "请填写课程号", parent=self); return
        if self.cat_var.get() == 0 and not kxh:
            messagebox.showwarning("提示", "必修课必须填写课序号", parent=self); return
        self.result = {"category": self.cat_var.get(),
                       "KCH": kch, "KXH": kxh, "KCM": kcm}
        self.destroy()

    def _cancel(self):
        self.result = None; self.destroy()


# ═══════════════════════════════════════════════════════════════
#  对话框 · 浏览课程
# ═══════════════════════════════════════════════════════════════

_COLS = ("sel", "cat", "KCH", "KXH", "KCM", "teacher", "cap")
_HEAD = ("✔",   "类别","课程号","课序号","课程名","教师","已选/容量")

class CourseBrowserDialog(tk.Toplevel):
    """从服务器拉取课程列表，勾选后添加到选课池"""

    def __init__(self, parent, conf):
        super().__init__(parent)
        self.title("浏览课程 — 正在加载…")
        self.geometry("900x540")
        self.minsize(720, 400)
        self.grab_set()
        self.configure(bg=C.BG)
        self.selected_courses = []

        self._all: list[tuple] = []
        self._conf = conf
        self._q: queue.Queue = queue.Queue()

        self._build()
        _center_win(self, parent)

        self.protocol("WM_DELETE_WINDOW", self._close)
        self.bind("<Escape>", lambda e: self._close())

        threading.Thread(target=self._fetch, daemon=True).start()
        self._poll()

    # ── UI ──

    def _build(self):
        # 搜索栏
        top = ttk.Frame(self); top.pack(fill=X, padx=12, pady=(12, 4))

        ttk.Label(top, text="🔍").pack(side=LEFT)
        self.search_var = tk.StringVar()
        self.search_var.trace_add("write", lambda *_: self._filter())
        se = ttk.Entry(top, textvariable=self.search_var, width=32)
        se.pack(side=LEFT, padx=(4, 8))
        ttk.Label(top, text="课程号 / 课程名 / 教师",
                  foreground=C.TEXT_DIS, font=("", 9)).pack(side=LEFT)

        self.status = ttk.Label(top, text="正在加载…",
                                foreground=C.TEXT_SEC, font=("", 9))
        self.status.pack(side=RIGHT)

        # 表格
        tf = ttk.Frame(self); tf.pack(fill=BOTH, expand=True, padx=12, pady=4)

        self.tree = ttk.Treeview(tf, columns=_COLS, show="headings",
                                 selectmode="extended", height=18)
        for c, h in zip(_COLS, _HEAD):
            self.tree.heading(c, text=h)

        self.tree.column("sel",     width=32,  anchor=CENTER, stretch=False)
        self.tree.column("cat",     width=52,  anchor=CENTER, stretch=False)
        self.tree.column("KCH",     width=96,  anchor=CENTER)
        self.tree.column("KXH",     width=56,  anchor=CENTER, stretch=False)
        self.tree.column("KCM",     width=220, anchor=W)
        self.tree.column("teacher", width=130, anchor=W)
        self.tree.column("cap",     width=80,  anchor=CENTER, stretch=False)

        vs = ttk.Scrollbar(tf, orient=VERTICAL,   command=self.tree.yview)
        hs = ttk.Scrollbar(tf, orient=HORIZONTAL, command=self.tree.xview)
        self.tree.configure(yscrollcommand=vs.set, xscrollcommand=hs.set)
        self.tree.grid(row=0, column=0, sticky="nsew")
        vs.grid(row=0, column=1, sticky="ns")
        hs.grid(row=1, column=0, sticky="ew")
        tf.rowconfigure(0, weight=1); tf.columnconfigure(0, weight=1)

        self.tree.heading("sel", command=self._toggle_all)
        self.tree.bind("<ButtonRelease-1>", self._click)

        # 底栏
        bf = ttk.Frame(self); bf.pack(fill=X, padx=12, pady=(4, 12))
        self.sel_lbl = ttk.Label(bf, text="已选 0 门",
                                 foreground=C.TEXT_SEC, font=("", 9))
        self.sel_lbl.pack(side=LEFT)

        ttk.Button(bf, text="添加到选课池", bootstyle="info",
                   command=self._confirm).pack(side=RIGHT, padx=(6, 0))
        ttk.Button(bf, text="取消", bootstyle="secondary",
                   command=self._close).pack(side=RIGHT, padx=(6, 0))
        ttk.Button(bf, text="全不选", bootstyle="secondary-outline",
                   command=self._none).pack(side=RIGHT, padx=(6, 0))
        ttk.Button(bf, text="全选", bootstyle="secondary-outline",
                   command=self._all).pack(side=RIGHT)

    # ── 后台拉取 ──

    def _fetch(self):
        try:
            self._q.put(("st", "正在登录…"))
            jd, ck = login(self._conf, log_func=lambda _: None)
            ba = show_msg(jd, log_func=lambda _: None,
                          batch_name=self._conf.get("batch_name", "第一轮正选（国际创新周）"))

            rows = []
            for cat in (0, 1):
                self._q.put(("st", f"正在获取{'必修' if cat == 0 else '选修'}课程…"))
                for i in get_class(jd, self._conf, batch=ba, category=cat) \
                        .get("data", {}).get("rows", []):
                    rows.append((cat, i))
            self._q.put(("ok", rows))
        except RuntimeError as e:
            self._q.put(("err", str(e)))
        except Exception as e:
            self._q.put(("err", f"{type(e).__name__}: {e}"))

    def _poll(self):
        try:
            while True:
                k, d = self._q.get_nowait()
                if k == "st":
                    self.status.config(text=d)
                elif k == "err":
                    self.status.config(text="加载失败", foreground=C.DANGER)
                    messagebox.showerror("加载失败", str(d), parent=self)
                elif k == "ok":
                    self._fill(d)
                    self.title("浏览课程 — 勾选后添加到选课池")
        except queue.Empty:
            pass
        if self.winfo_exists():
            self.after(80, self._poll)

    def _fill(self, rows):
        self._all = []
        for cat, i in rows:
            ct = "必修" if cat == 0 else "选修"
            self._all.append((
                "", ct,
                i.get("KCH", ""), i.get("KXH", ""), i.get("KCM", ""),
                i.get("SKJS", ""),
                f"{i.get('numberOfSelected', '?')}/{i.get('classCapacity', '?')}",
                cat,
            ))
        self._refresh(self._all)
        self.status.config(text=f"共 {len(self._all)} 门课程",
                          foreground=C.SUCCESS)

    def _refresh(self, rows):
        self.tree.delete(*self.tree.get_children())
        for r in rows:
            self.tree.insert("", END, values=r[:7], tags=(str(r[7]),))

    # ── 搜索 ──

    def _filter(self):
        kw = self.search_var.get().strip().lower()
        if not kw:
            self._refresh(self._all); return
        self._refresh([r for r in self._all
                       if kw in r[2].lower() or kw in r[4].lower()
                       or kw in r[5].lower()])

    # ── 勾选 ──

    def _click(self, e):
        rid = self.tree.identify_row(e.y)
        col = self.tree.identify_column(e.x)
        if not rid or col != "#1":
            return
        v = list(self.tree.item(rid, "values"))
        v[0] = "" if v[0] == "✔" else "✔"
        self.tree.item(rid, values=v); self._count()

    def _toggle_all(self):
        items = self.tree.get_children()
        if not items: return
        mark = "" if all(self.tree.item(i, "values")[0] == "✔" for i in items) else "✔"
        for i in items:
            v = list(self.tree.item(i, "values")); v[0] = mark
            self.tree.item(i, values=v)
        self._count()

    def _all(self):
        for i in self.tree.get_children():
            v = list(self.tree.item(i, "values")); v[0] = "✔"
            self.tree.item(i, values=v)
        self._count()

    def _none(self):
        for i in self.tree.get_children():
            v = list(self.tree.item(i, "values")); v[0] = ""
            self.tree.item(i, values=v)
        self._count()

    def _count(self):
        n = sum(1 for i in self.tree.get_children()
                if self.tree.item(i, "values")[0] == "✔")
        self.sel_lbl.config(text=f"已选 {n} 门")

    # ── 确认 / 取消 ──

    def _confirm(self):
        self.selected_courses = []
        for i in self.tree.get_children():
            v = self.tree.item(i, "values")
            if v[0] == "✔":
                self.selected_courses.append({
                    "category": 0 if v[1] == "必修" else 1,
                    "KCH": v[2], "KXH": v[3], "KCM": v[4],
                })
        self.destroy()

    def _close(self):
        self.selected_courses = []; self.destroy()


# ═══════════════════════════════════════════════════════════════
#  主界面
# ═══════════════════════════════════════════════════════════════

class Application:
    TREE_COLS  = ("category", "KCH", "KXH", "KCM")
    TREE_HEADS = ("类别",      "课程号","课序号","课程名")

    def __init__(self, root: ttk.Window):
        self.root = root
        self.root.title("西电自动选课工具")
        self.root.geometry("900x780")
        self.root.minsize(800, 680)

        self.msg_q: queue.Queue = queue.Queue()
        self.stop_ev = threading.Event()
        self.running = False

        self._apply_style()
        self._build_login()
        self._build_courses()
        self._build_actions()
        self._build_log()
        self._load_conf()
        self._poll()

        self.root.protocol("WM_DELETE_WINDOW", self._quit)

    # ────────────── 样式 ──────────────

    def _apply_style(self):
        s = self.root.style
        s.configure("Treeview", rowheight=30, font=("", 10))
        s.configure("Treeview.Heading", font=("", 10, "bold"))
        # 日志区用 tkinter.Text，不走 ttk

    # ────────────── 登录区 ──────────────

    def _build_login(self):
        card = ttk.Labelframe(self.root, text="  登录信息  ", bootstyle="info")
        card.pack(fill=X, padx=16, pady=(16, 8))

        r1 = ttk.Frame(card); r1.pack(fill=X, padx=16, pady=(12, 4))
        ttk.Label(r1, text="学号").pack(side=LEFT)
        self.v_user = tk.StringVar()
        ttk.Entry(r1, textvariable=self.v_user, width=20).pack(side=LEFT, padx=(4, 20))

        ttk.Label(r1, text="密码").pack(side=LEFT)
        self.v_pass = tk.StringVar()
        ttk.Entry(r1, textvariable=self.v_pass, width=20, show="•").pack(side=LEFT, padx=(4, 20))

        self.v_ocr = tk.BooleanVar(value=True)
        ttk.Checkbutton(r1, text="自动验证码", variable=self.v_ocr,
                        bootstyle="info").pack(side=LEFT, padx=(0, 12))
        self.v_dbg = tk.BooleanVar(value=False)
        ttk.Checkbutton(r1, text="调试", variable=self.v_dbg,
                        bootstyle="secondary").pack(side=LEFT)

        r2 = ttk.Frame(card); r2.pack(fill=X, padx=16, pady=(4, 12))
        ttk.Label(r2, text="选课批次").pack(side=LEFT)
        self.v_batch = tk.StringVar(value="第一轮正选（国际创新周）")
        ttk.Entry(r2, textvariable=self.v_batch, width=42).pack(side=LEFT, padx=(4, 8))
        ttk.Label(r2, text="匹配批次名称关键字",
                  foreground=C.TEXT_DIS, font=("", 9)).pack(side=LEFT)

    # ────────────── 课程列表 ──────────────

    def _build_courses(self):
        card = ttk.Labelframe(self.root, text="  选课池  ", bootstyle="info")
        card.pack(fill=BOTH, expand=True, padx=16, pady=8)

        tf = ttk.Frame(card); tf.pack(fill=BOTH, expand=True, padx=8, pady=(8, 0))
        self.tree = ttk.Treeview(tf, columns=self.TREE_COLS, show="headings",
                                 selectmode="extended")
        for c, h in zip(self.TREE_COLS, self.TREE_HEADS):
            self.tree.heading(c, text=h)
        self.tree.column("category", width=60,  anchor=CENTER, stretch=False)
        self.tree.column("KCH",      width=130, anchor=CENTER)
        self.tree.column("KXH",      width=70,  anchor=CENTER, stretch=False)
        self.tree.column("KCM",      width=240, anchor=W)

        sb = ttk.Scrollbar(tf, orient=VERTICAL, command=self.tree.yview)
        self.tree.configure(yscrollcommand=sb.set)
        self.tree.pack(side=LEFT, fill=BOTH, expand=True)
        sb.pack(side=RIGHT, fill=Y)

        self.tree.bind("<Double-1>", self._edit_course)

        # 按钮行
        bf = ttk.Frame(card); bf.pack(fill=X, padx=8, pady=8)
        ttk.Button(bf, text="＋ 添加课程", bootstyle="info-outline",
                   command=self._add_course).pack(side=LEFT, padx=(0, 6))
        ttk.Button(bf, text="－ 删除选中", bootstyle="danger-outline",
                   command=self._del_course).pack(side=LEFT, padx=(0, 6))
        ttk.Button(bf, text="📋 浏览课程", bootstyle="info-outline",
                   command=self._browse).pack(side=LEFT)
        self.lbl_cnt = ttk.Label(bf, text="共 0 门课",
                                 foreground=C.TEXT_SEC, font=("", 9))
        self.lbl_cnt.pack(side=RIGHT)

    # ────────────── 操作栏 ──────────────

    def _build_actions(self):
        f = ttk.Frame(self.root); f.pack(fill=X, padx=16, pady=4)

        self.btn_sel = ttk.Button(f, text="选　课", bootstyle="success",
                                  command=self._start_sel)
        self.btn_sel.pack(side=LEFT, padx=(0, 6))
        self.btn_drop = ttk.Button(f, text="退　课", bootstyle="danger",
                                   command=self._start_drop)
        self.btn_drop.pack(side=LEFT, padx=(0, 6))
        self.btn_chk = ttk.Button(f, text="容量检查", bootstyle="warning",
                                  command=self._start_chk)
        self.btn_chk.pack(side=LEFT, padx=(0, 12))
        self.btn_stop = ttk.Button(f, text="■ 停止", bootstyle="secondary",
                                   command=self._stop, state=DISABLED)
        self.btn_stop.pack(side=LEFT)

        self.v_always = tk.BooleanVar(value=True)
        ttk.Checkbutton(f, text="连续重试", variable=self.v_always,
                        bootstyle="secondary-round-toggle").pack(side=LEFT, padx=(16, 0))

    # ────────────── 日志区 ──────────────

    def _build_log(self):
        card = ttk.Labelframe(self.root, text="  日志  ", bootstyle="secondary")
        card.pack(fill=BOTH, padx=16, pady=(4, 8), expand=False)

        self.log = tk.Text(
            card, height=14, wrap="word",
            font=("Cascadia Code", 9),
            bg=C.LOG_BG, fg=C.LOG_FG,
            insertbackground=C.LOG_FG,
            selectbackground=C.LOG_SEL,
            relief="flat", bd=0, padx=10, pady=8,
        )
        sb = ttk.Scrollbar(card, orient=VERTICAL, command=self.log.yview)
        self.log.configure(yscrollcommand=sb.set)
        self.log.pack(side=LEFT, fill=BOTH, expand=True, padx=(4, 0), pady=4)
        sb.pack(side=RIGHT, fill=Y, padx=(0, 4), pady=4)

        self.log.tag_configure("info",    foreground=C.LOG_FG)
        self.log.tag_configure("success", foreground=C.LOG_SUCCESS)
        self.log.tag_configure("error",   foreground=C.LOG_ERROR)
        self.log.tag_configure("warn",    foreground=C.LOG_WARN)

        bf = ttk.Frame(self.root); bf.pack(fill=X, padx=16, pady=(0, 12))
        ttk.Button(bf, text="清除日志", bootstyle="secondary-outline",
                   command=self._clear_log).pack(side=RIGHT)

    # ════════════════════════════════════════════════════════════
    #  课程管理
    # ════════════════════════════════════════════════════════════

    def _add_course(self):
        d = AddCourseDialog(self.root); self.root.wait_window(d)
        if d.result:
            r = d.result
            ct = "必修" if r["category"] == 0 else "选修"
            self.tree.insert("", END, values=(ct, r["KCH"], r["KXH"], r["KCM"]))
            self._cnt(); self._log(f"已添加：{r['KCH']} {r['KXH']}（{ct}）")

    def _browse(self):
        conf = self._mk_conf()
        if not conf["data"]["loginname"] or not conf["data"]["password"]:
            messagebox.showwarning("提示", "请先填写学号和密码"); return
        self._save()
        d = CourseBrowserDialog(self.root, conf); self.root.wait_window(d)
        seen = {(self.tree.item(i, "values")[1], self.tree.item(i, "values")[2])
                for i in self.tree.get_children()}
        n = 0
        for c in d.selected_courses:
            if (c["KCH"], c["KXH"]) in seen:
                continue
            ct = "必修" if c["category"] == 0 else "选修"
            self.tree.insert("", END, values=(ct, c["KCH"], c["KXH"], c["KCM"]))
            seen.add((c["KCH"], c["KXH"])); n += 1
        if n:
            self._cnt(); self._log(f"从课程列表添加了 {n} 门课程")

    def _del_course(self):
        sel = self.tree.selection()
        if not sel: return
        for it in sel:
            v = self.tree.item(it, "values")
            self.tree.delete(it)
            self._log(f"已删除：{v[1]} {v[2]}")
        self._cnt()

    def _edit_course(self, e):
        it = self.tree.identify_row(e.y)
        if not it: return
        v = self.tree.item(it, "values")
        d = AddCourseDialog(self.root)
        d.cat_var.set(0 if v[0] == "必修" else 1)
        d.kch.insert(0, v[1]); d.kxh.insert(0, v[2]); d.kcm.insert(0, v[3])
        self.root.wait_window(d)
        if d.result:
            r = d.result
            ct = "必修" if r["category"] == 0 else "选修"
            self.tree.item(it, values=(ct, r["KCH"], r["KXH"], r["KCM"]))
            self._log(f"已更新：{r['KCH']} {r['KXH']}")

    def _courses(self):
        out = []
        for it in self.tree.get_children():
            v = self.tree.item(it, "values")
            out.append({"category": 0 if v[0] == "必修" else 1,
                        "KCH": v[1], "KXH": v[2], "KCM": v[3]})
        return out

    def _cnt(self):
        self.lbl_cnt.config(text=f"共 {len(self.tree.get_children())} 门课")

    # ════════════════════════════════════════════════════════════
    #  配置持久化
    # ════════════════════════════════════════════════════════════

    def _load_conf(self):
        c = load_conf()
        self.v_user.set(c["data"].get("loginname", ""))
        self.v_pass.set(c["data"].get("password", ""))
        self.v_ocr.set(c.get("ocr_captcha", "1") == "1")
        self.v_dbg.set(c.get("debug", "0") == "1")
        self.v_batch.set(c.get("batch_name", "第一轮正选（国际创新周）"))
        for x in c.get("bx", []):
            self.tree.insert("", END, values=("必修", x["KCH"], x.get("KXH", ""), x.get("KCM", "")))
        for x in c.get("xx", []):
            self.tree.insert("", END, values=("选修", x["KCH"], "", x.get("KCM", "")))
        self._cnt()

    def _save(self):
        save_conf(self.v_user.get().strip(), self.v_pass.get().strip(),
                  self.v_ocr.get(), self.v_dbg.get(), self._courses(),
                  self.v_batch.get().strip())

    def _mk_conf(self):
        return {
            "ocr_captcha": "1" if self.v_ocr.get() else "0",
            "debug": "1" if self.v_dbg.get() else "0",
            "batch_name": self.v_batch.get().strip(),
            "bx_or_xx": 0, "bx": [], "xx": [],
            "data": {"loginname": self.v_user.get().strip(),
                     "password": self.v_pass.get().strip(),
                     "captcha": "xxxx", "uuid": "xxxx"},
        }

    # ════════════════════════════════════════════════════════════
    #  日志
    # ════════════════════════════════════════════════════════════

    def _log(self, msg, tag="info"):
        self.log.insert(END, msg + "\n", tag)
        self.log.see(END)

    def _log_cb(self, msg):
        self.msg_q.put(("log", msg))

    def _log_err(self, msg):
        """记录错误日志（红色高亮 + 弹窗提示）"""
        self.msg_q.put(("err", str(msg)))

    def _clear_log(self):
        self.log.delete("1.0", END)

    def _poll(self):
        try:
            while True:
                k, d = self.msg_q.get_nowait()
                if k == "log":
                    t = "info"
                    s = str(d)
                    if "操作成功" in s or "已在选课结果" in s or "[OK]" in s:
                        t = "success"
                    elif "失败" in s or "错误" in s or "不存在" in s:
                        t = "error"
                    elif "冲突" in s:
                        t = "warn"
                    self._log(s, t)
                elif k == "err":
                    self._log("[ERR] " + str(d), "error")
                    self._pending_err = str(d)
                elif k == "done":
                    self.running = False
                    self.btn_stop.config(state=DISABLED)
                    self._enable()
                    self._log("─" * 40)
                    # 弹出上一次错误（如果有）
                    if hasattr(self, '_pending_err') and self._pending_err:
                        messagebox.showerror("操作失败", self._pending_err)
                        self._pending_err = None
        except queue.Empty:
            pass
        self.root.after(80, self._poll)

    # ════════════════════════════════════════════════════════════
    #  按钮状态
    # ════════════════════════════════════════════════════════════

    def _disable(self):
        for b in (self.btn_sel, self.btn_drop, self.btn_chk):
            b.config(state=DISABLED)

    def _enable(self):
        for b in (self.btn_sel, self.btn_drop, self.btn_chk):
            b.config(state=NORMAL)

    # ════════════════════════════════════════════════════════════
    #  选课
    # ════════════════════════════════════════════════════════════

    def _start_sel(self):
        conf = self._mk_conf()
        if not conf["data"]["loginname"] or not conf["data"]["password"]:
            messagebox.showwarning("提示", "请先填写学号和密码"); return
        cs = self._courses()
        if not cs:
            messagebox.showwarning("提示", "请先添加课程"); return
        self._save()
        self.stop_ev.clear(); self.running = True
        self._disable(); self.btn_stop.config(state=NORMAL)
        self._log("── 开始选课 ──")
        a = 1 if self.v_always.get() else 0
        threading.Thread(target=self._w_sel, args=(conf, cs, a), daemon=True).start()

    def _w_sel(self, conf, cs, always):
        try:
            self.msg_q.put(("log", "正在登录…"))
            jd, ck = login(conf, log_func=self._log_cb)
            ba = show_msg(jd, log_func=self._log_cb,
                          batch_name=conf.get("batch_name", "第一轮正选（国际创新周）"))
            self.msg_q.put(("log", f"选课批次 code：{ba}"))

            # 按需获取必修 + 选修课程列表
            need_cats = {c["category"] for c in cs}
            rows_by_cat = {}
            for cat in need_cats:
                cat_name = "必修" if cat == 0 else "选修"
                self.msg_q.put(("log", f"正在获取{cat_name}课程列表…"))
                rows = get_class(jd, conf, batch=ba, category=cat) \
                    .get("data", {}).get("rows", [])
                rows_by_cat[cat] = rows
                self.msg_q.put(("log", f"  {cat_name}：{len(rows)} 门"))

            for c in cs:
                if self.stop_ev.is_set():
                    self.msg_q.put(("log", "用户停止操作")); break
                rows = rows_by_cat.get(c["category"], [])
                found = False
                for i in rows:
                    if i["KCH"] == c["KCH"]:
                        if c["category"] == 0:
                            for j in i.get("tcList", []):
                                if j["KXH"] == c["KXH"]:
                                    add(jd, j, cookie=ck, batch=ba, always=always,
                                        category=0, log_func=self._log_cb,
                                        stop_event=self.stop_ev)
                                    found = True; break
                        else:
                            add(jd, i, cookie=ck, batch=ba, always=always,
                                category=1, log_func=self._log_cb,
                                stop_event=self.stop_ev)
                            found = True
                        break
                if not found:
                    self.msg_q.put(("log", f"未找到课程 {c['KCH']} {c['KXH']}"))
        except RuntimeError as e:
            self._log_err(e)
        except Exception as e:
            self._log_err(f"选课出错：{type(e).__name__}: {e}")
        finally:
            self.msg_q.put(("done", ""))

    # ════════════════════════════════════════════════════════════
    #  退课
    # ════════════════════════════════════════════════════════════

    def _start_drop(self):
        conf = self._mk_conf()
        if not conf["data"]["loginname"] or not conf["data"]["password"]:
            messagebox.showwarning("提示", "请先填写学号和密码"); return
        cs = self._courses()
        if not cs:
            messagebox.showwarning("提示", "请先添加课程"); return
        self._save()
        self.stop_ev.clear(); self.running = True
        self._disable(); self.btn_stop.config(state=NORMAL)
        self._log("── 开始退课 ──")
        a = 1 if self.v_always.get() else 0
        threading.Thread(target=self._w_drop, args=(conf, cs, a), daemon=True).start()

    def _w_drop(self, conf, cs, always):
        try:
            self.msg_q.put(("log", "正在登录…"))
            jd, ck = login(conf, log_func=self._log_cb)
            ba = show_msg(jd, log_func=self._log_cb,
                          batch_name=conf.get("batch_name", "第一轮正选（国际创新周）"))
            self.msg_q.put(("log", f"选课批次 code：{ba}"))

            need_cats = {c["category"] for c in cs}
            rows_by_cat = {}
            for cat in need_cats:
                cat_name = "必修" if cat == 0 else "选修"
                self.msg_q.put(("log", f"正在获取{cat_name}课程列表…"))
                rows = get_class(jd, conf, batch=ba, category=cat) \
                    .get("data", {}).get("rows", [])
                rows_by_cat[cat] = rows
                self.msg_q.put(("log", f"  {cat_name}：{len(rows)} 门"))

            for c in cs:
                if self.stop_ev.is_set():
                    self.msg_q.put(("log", "用户停止操作")); break
                rows = rows_by_cat.get(c["category"], [])
                found = False
                for i in rows:
                    if i["KCH"] == c["KCH"]:
                        if c["category"] == 0:
                            for j in i.get("tcList", []):
                                if j["KXH"] == c["KXH"]:
                                    dele(jd, j, cookie=ck, batch=ba, always=always,
                                         category=0, log_func=self._log_cb,
                                         stop_event=self.stop_ev)
                                    found = True; break
                        else:
                            dele(jd, i, cookie=ck, batch=ba, always=always,
                                 category=1, log_func=self._log_cb,
                                 stop_event=self.stop_ev)
                            found = True
                        break
                if not found:
                    self.msg_q.put(("log", f"未找到课程 {c['KCH']} {c['KXH']}"))
        except RuntimeError as e:
            self._log_err(e)
        except Exception as e:
            self._log_err(f"退课出错：{type(e).__name__}: {e}")
        finally:
            self.msg_q.put(("done", ""))

    # ════════════════════════════════════════════════════════════
    #  容量检查
    # ════════════════════════════════════════════════════════════

    def _start_chk(self):
        conf = self._mk_conf()
        if not conf["data"]["loginname"] or not conf["data"]["password"]:
            messagebox.showwarning("提示", "请先填写学号和密码"); return
        cs = self._courses()
        if not cs:
            messagebox.showwarning("提示", "请先添加课程"); return
        self._save()
        self.stop_ev.clear(); self.running = True
        self._disable(); self.btn_stop.config(state=NORMAL)
        self._log("── 开始容量检查（有空位自动选课）──")
        threading.Thread(target=self._w_chk, args=(conf, cs), daemon=True).start()

    def _w_chk(self, conf, cs):
        try:
            self.msg_q.put(("log", "正在登录…"))
            jd, ck = login(conf, log_func=self._log_cb)
            ba = show_msg(jd, log_func=self._log_cb,
                          batch_name=conf.get("batch_name", "第一轮正选（国际创新周）"))
            self.msg_q.put(("log", f"选课批次 code：{ba}"))
            kset = {c["KCH"] for c in cs}
            k = 0
            while not self.stop_ev.is_set():
                k += 1
                rows = get_class(jd, conf, batch=ba, category=0) \
                    .get("data", {}).get("rows", [])
                for i in rows:
                    if i["KCH"] in kset and i.get("SFYX") == "0":
                        sel = int(i.get("numberOfSelected", 0))
                        cap = int(i.get("classCapacity", 0))
                        self.msg_q.put(("log",
                            f"{i['KCM']}　已选/容量：{sel}/{cap}"))
                        if sel < cap:
                            self.msg_q.put(("log",
                                f"  ✦ 发现空位 → {i['KXH']} {i['KCM']}"))
                            add(jd, i, ck, ba, category=1, always=0,
                                log_func=self._log_cb,
                                stop_event=self.stop_ev)
                self.msg_q.put(("log", f"第 {k} 次检查{'━' * min(k, 20)}"))
                k = k % 10; time.sleep(0.5)
        except RuntimeError as e:
            self._log_err(e)
        except Exception as e:
            self._log_err(f"容量检查出错：{type(e).__name__}: {e}")
        finally:
            self.msg_q.put(("done", ""))

    # ════════════════════════════════════════════════════════════
    #  停止 / 退出
    # ════════════════════════════════════════════════════════════

    def _stop(self):
        if self.running:
            self.stop_ev.set(); self._log("正在停止…")

    def _quit(self):
        if self.running:
            if not messagebox.askyesno("确认", "任务正在运行，确定退出？"):
                return
            self.stop_ev.set()
        self._save(); self.root.destroy()


# ═══════════════════════════════════════════════════════════════
#  入口
# ═══════════════════════════════════════════════════════════════

if __name__ == "__main__":
    root = ttk.Window(
        title="西电自动选课工具",
        themename="cosmo",       # Fluent 浅色主题
        size=(900, 780),
        minsize=(800, 680),
    )
    app = Application(root)
    root.mainloop()
