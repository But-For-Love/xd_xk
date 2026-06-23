# -*- coding: utf-8 -*-
"""
打包脚本：将 GUI 版本打包为单个便携式 exe
用法：python build.py
"""
import subprocess
import sys
import os
import glob
import shutil

def main():
    pkg_site = os.path.join(os.path.dirname(sys.executable), 'Lib', 'site-packages')
    ddddocr_dir = os.path.join(pkg_site, 'ddddocr')
    ttk_dir     = os.path.join(pkg_site, 'ttkbootstrap')

    # 收集 ddddocr 数据文件
    ddddocr_args = []
    for pat in ('*.onnx', '*.png'):
        for f in glob.glob(os.path.join(ddddocr_dir, pat)):
            ddddocr_args += ['--add-data', f'{f};ddddocr']

    # 收集 ttkbootstrap 主题
    ttk_args = []
    themes = os.path.join(ttk_dir, 'themes')
    if os.path.isdir(themes):
        for f in glob.glob(os.path.join(themes, '*.json')):
            ttk_args += ['--add-data', f'{f};ttkbootstrap/themes']

    # 清理旧构建
    for d in ('build', 'dist'):
        if os.path.isdir(d):
            shutil.rmtree(d)

    cmd = [
        sys.executable, '-m', 'PyInstaller',
        '--onefile',
        '--noconsole',
        '--name', '西电自动选课工具',
        '--add-data', 'conf.example.json;.',
        *ddddocr_args,
        *ttk_args,
        '--hidden-import', 'ddddocr',
        '--hidden-import', 'onnxruntime',
        '--hidden-import', 'PIL',
        '--hidden-import', 'Crypto.Cipher.AES',
        '--hidden-import', 'requests',
        '--hidden-import', 'ttkbootstrap',
        '--hidden-import', 'func',
        '--hidden-import', 'encrypt',
        'gui.py',
    ]

    print(' '.join(cmd))
    subprocess.run(cmd, check=True)
    print('\n[OK] dist/' + '西电自动选课工具.exe')

if __name__ == '__main__':
    main()
