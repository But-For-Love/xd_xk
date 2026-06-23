# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

西安电子科技大学 (Xidian University) 自动选课工具。通过模拟 HTTP 请求与选课系统 (xk.xidian.edu.cn) 交互，实现自动登录、验证码识别、选课/退课功能。

## Setup

```bash
pip install -i https://pypi.tuna.tsinghua.edu.cn/simple -r requirements.txt
```

Dependencies: `ddddocr` (captcha OCR), `matplotlib` (manual captcha display), `pycryptodome` (AES encryption), `requests`, `ttkbootstrap` (Fluent Design GUI 主题).

## Running

GUI 版本（推荐）：
```bash
python gui.py
```

命令行版本：
```bash
python xk_main.py
```

Course details and credentials are configured in `conf.json`. Credentials can also be entered at runtime if left empty in config. GUI 版本启动时自动加载 conf.json，关闭时自动保存。

## Architecture

**Source files:**

- `gui.py` — GUI 入口（tkinter）。包含 `Application` 主界面类和 `AddCourseDialog` 添加课程对话框。通过 `threading` + `queue.Queue` 实现后台网络操作与界面日志的线程安全通信。
- `xk_main.py` — 命令行入口。Contains `main()` (reads `conf.json`), `add_1()`/`del_1()` (programmatic add/drop with custom course lists), and `check()` (capacity polling loop). The `if __name__` block toggles between `main()` and `add_1()`/`del_1()` by commenting/uncommenting.
- `func.py` — All HTTP interactions: `login()`, `get_captcha()`, `show_msg()`, `get_class()`, `add()`, `dele()`. 支持可选的 `log_func` 回调和 `stop_event` 停止信号，GUI 和命令行共用。
- `encrypt.py` — AES-ECB encryption with PKCS7 padding for password field. Key is hardcoded as `MWMqg2tPcDkxcm11`. `aes_decrypt` is broken (uses CBC mode incorrectly).

**Config (`conf.json`):**
- `ocr_captcha`: `"1"` for auto OCR, `"0"` for manual captcha input
- `debug`: `"1"` to dump raw API responses to JSON files
- `batch_name`: 选课批次名称关键字，用于自动匹配当前开放批次（如 `"第一轮正选（国际创新周）"`）
- `bx_or_xx`: `0` = 必修 (required courses, `FANKC`), `1` = 选修 (elective courses, `XGKC`)
- `bx[]`: required course list — needs both `KCH` (course code) and `KXH` (section number)
- `xx[]`: elective course list — only needs `KCH`
- `data`: login credentials (password stored in plaintext)

**API base:** `https://xk.xidian.edu.cn/xsxk/`

**Key API endpoints:**
- `auth/captcha` → GET captcha image + uuid
- `auth/login` → POST login with encrypted password
- `elective/clazz/list` → POST fetch available courses (requires `batchId` + `Authorization` in headers)
- `elective/clazz/add` → POST enroll in a course
- `elective/clazz/del` → POST drop a course

## Important Details

- `show_msg()` uses `batch_name` from config to match the 选课批次 — defaults to `"2025级"` if not provided. Update `batch_name` in `conf.json` for different grade years or selection rounds.
- `add()` and `dele()` retry in a 1-second loop when `always=1`, printing status each iteration. Both accept an optional `stop_event` (`threading.Event`) to interrupt the retry loop.
- Required courses (`bx`) need `KXH` (section number); electives (`xx`) do not.
- `check()` is hardcoded for specific elective course codes — edit the list before use.
- `encrypt.py` 的 `aes_decrypt` 方法不可用（错误使用了 CBC 模式），只有 `AES_encrypt` 是正确的。
- `conf.json` is gitignored. Use `conf.example.json` as a template. Never commit real credentials.
