# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

西电自动选课工具 — a Python desktop app (Tkinter GUI + CLI) that automates course registration for Xidian University (xidian.edu.cn). It handles login (with OCR or manual captcha), add/drop courses, capacity checking (poll-and-grab), and a "snipe" mode that scans all courses for openings.

## Commands

```bash
# Install dependencies
uv sync

# Run GUI
uv run xd-xk-gui

# Run CLI
uv run xd-xk --help
uv run xd-xk select          # add courses (必修, requires KXH per line in tcList)
uv run xd-xk select -c 1     # add courses (选修, no tcList nesting)
uv run xd-xk drop -c 1       # drop courses (选修)
uv run xd-xk check           # capacity check using conf.json course list
uv run xd-xk check KCH1 KCH2 # capacity check for specific course IDs

# Lint and format
uv run ruff check .
uv run ruff format .

# Build distributable .exe
uv run python build.py
```

No test suite exists yet.

## Architecture

Four source files in `xd_xk/`, each with a distinct responsibility:

### `core.py` — All business logic (no UI)

- **`CourseSession`** — facade dataclass: `login()` → `show_msg()` → batch matching, packaged into a single object holding `token`, `cookie`, `batch_code`, and raw `data`. GUI and CLI both use `CourseSession.create()`.
- **`login()`** — calls `get_captcha()` (OCR via ddddocr or manual callback), AES-encrypts password, POSTs to `xk.xidian.edu.cn/xsxk/auth/login`.
- **`show_msg()`** / **`match_batch_name()`** / **`get_batch_list()`** — parse student info and the elective batch list from the login response; match a user-provided keyword against batch names.
- **`get_class()`** / **`fetch_courses()`** — fetch course lists from the API. `get_class()` takes a category (0=FANKC 必修, 1=XGKC 选修); `fetch_courses()` batches multiple categories.
- **`add()`** / **`dele()`** — add/drop courses. Both delegate to **`_poll_operation()`** (template method pattern) which loops with a 1-second sleep until a stop-message is received or `stop_event` is set. Stop messages: "操作成功", "课程已在选课结果中", "所选课程与已选课程冲突", "所选课程人数已满", "选课门数或学分超过".
- **`captcha_cb`** pattern: core never shows UI; callers pass a callback `(bytes) -> str` for manual captcha input. CLI implements it with PIL image display; GUI relies on ddddocr auto-OCR and does not provide manual input (the GUI's "自动验证码" checkbox toggles `ocr_captcha` in conf).

### `gui.py` — Tkinter GUI (ttkbootstrap, Fluent Design)

- **`Application`** — main window class. Uses a `queue.Queue` (`msg_q`) + daemon **`threading.Thread`** for background work. Threads send `Msg` dataclass objects to the queue; `_poll()` (called every 80ms via `root.after`) drains the queue on the main thread.
- **`Msg`** — frozen dataclass with `kind` ("log"|"err"|"done"|"st"|"ok") and `payload`, replacing the old bare-tuple protocol.
- **`AddCourseDialog`** / **`CourseBrowserDialog`** — modal `Toplevel` dialogs. Browser fetches courses via background thread and displays them in a Treeview with search/filter.
- **`C`** class holds the Fluent Design color palette.
- **`load_conf()`** / **`save_conf()`** — JSON config persistence. Config auto-saves on window close and auto-loads on startup.
- Operations (选课/退课/容量检查/捡漏) follow the same pattern: `_save()` → `stop_ev.clear()` → `_disable()` buttons → spawn daemon thread → thread calls `CourseSession.create()` then the relevant core functions → sends `Msg("done")` on completion.

### `cli.py` — CLI (argparse subcommands)

- `select`, `drop`, `check` subcommands. Uses `CourseSession.create()` same as GUI.
- `_manual_captcha()` — writes captcha bytes to a temp PNG, opens with PIL, prompts stdin. This is the CLI's `captcha_cb` implementation.
- `cmd_check()` — loops `get_class()` + `add()` with 0.5s sleep, similar to GUI capacity check but single-threaded.

### `encrypt.py` — AES-ECB encryption

- `AESCipher` class with PKCS7 padding, used for password encryption before login. `AES_encrypt()` is the convenience function used throughout.

### `build.py` — PyInstaller packaging

Collects ddddocr ONNX models, ttkbootstrap themes, and xd_xk source files; produces a single `西电自动选课工具.exe` with `--noconsole`.

## Configuration

`conf.json` (gitignored, template at `conf.example.json`):

```json
{
  "ocr_captcha": "1",        // "1" = auto OCR, "0" = manual
  "debug": "0",              // "1" = dump API responses to files
  "batch_name": "",          // keyword to match elective batch
  "bx": [{ "KCH": "", "KXH": "", "KCM": "" }],  // 必修 courses
  "xx": [{ "KCH": "", "KXH": "", "KCM": "" }],  // 选修 courses
  "data": { "loginname": "", "password": "", "captcha": "xxxx", "uuid": "xxxx" }
}
```

Boolean values in conf are strings `"0"`/`"1"` (not JSON booleans). Use `_conf_bool()` in core.py or `== "1"` comparisons.

## Key API Details

- Base URL: `https://xk.xidian.edu.cn/xsxk`
- Requires campus network or VPN to access
- Login returns a token used as `Authorization` header; batch code as `batchId` header
- 必修 (category=0, clazzType=FANKC) courses are nested under `tcList` per course; 选修 (category=1, clazzType=XGKC) are flat rows
- Course objects use field names: `KCH` (课程号), `KXH` (课序号), `KCM` (课程名), `JXBID`, `secretVal`, `SKJS`, `numberOfSelected`, `classCapacity`, `SFYX`
- `SFYX == "0"` means the course has available capacity

## Dependencies

| Package | Purpose |
|---------|---------|
| `ddddocr` | OCR captcha recognition (bundles ONNX models) |
| `pycryptodomex` | AES encryption for passwords |
| `requests` | HTTP client |
| `ttkbootstrap` | Themed Tkinter widgets (cosmo theme) |
| `Pillow` | Image display for manual captcha in CLI |

Dev: `ruff` (linter/formatter), `pyinstaller` (exe bundling). Package manager: `uv`. Build backend: `hatchling`.

