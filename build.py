"""
打包脚本：将 GUI 版本打包为单个便携式 exe
用法：python build.py
"""

from __future__ import annotations

import glob
import shutil
import subprocess
import sys
from pathlib import Path


def main() -> None:
    pkg_site = Path(sys.exec_prefix) / "Lib" / "site-packages"
    ddddocr_dir = pkg_site / "ddddocr"
    ttk_dir = pkg_site / "ttkbootstrap"

    # 收集 ddddocr 数据文件
    ddddocr_args: list[str] = []
    for pat in ("*.onnx", "*.png"):
        for f in glob.glob(str(ddddocr_dir / pat)):
            ddddocr_args += ["--add-data", f"{f};ddddocr"]

    # 收集 ttkbootstrap 主题
    ttk_args: list[str] = []
    themes = ttk_dir / "themes"
    if themes.is_dir():
        for f in glob.glob(str(themes / "*.json")):
            ttk_args += ["--add-data", f"{f};ttkbootstrap/themes"]

    # 收集 xd_xk 包数据
    xd_xk_args: list[str] = []
    xd_xk_dir = Path("xd_xk")
    for py_file in xd_xk_dir.glob("*.py"):
        xd_xk_args += ["--add-data", f"{py_file};xd_xk"]

    # 清理旧构建
    for d in ("build", "dist"):
        p = Path(d)
        if p.is_dir():
            shutil.rmtree(p)

    # 清理 spec 文件
    for spec in Path().glob("*.spec"):
        spec.unlink()

    cmd = [
        sys.executable,
        "-m",
        "PyInstaller",
        "--onefile",
        "--noconsole",
        "--name",
        "西电自动选课工具",
        "--add-data",
        "conf.example.json;.",
        *ddddocr_args,
        *ttk_args,
        *xd_xk_args,
        "--hidden-import",
        "ddddocr",
        "--hidden-import",
        "onnxruntime",
        "--hidden-import",
        "PIL",
        "--hidden-import",
        "Cryptodome.Cipher.AES",
        "--hidden-import",
        "requests",
        "--hidden-import",
        "ttkbootstrap",
        "--hidden-import",
        "xd_xk.core",
        "--hidden-import",
        "xd_xk.encrypt",
        "xd_xk/gui.py",
    ]

    print(" ".join(cmd))
    subprocess.run(cmd, check=True)
    print("\n[OK] dist/" + "西电自动选课工具.exe")


if __name__ == "__main__":
    main()
