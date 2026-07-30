# 西电自动选课工具

西安电子科技大学教务系统自动选课 / 退课 / 捡漏工具。通过 HTTP 模拟与 [xk.xidian.edu.cn](https://xk.xidian.edu.cn) 交互，提供 GUI 和 CLI 两种模式。

> **仅供学习交流使用。**

## 功能

- **自动登录** — 模拟登录，内置 OCR 自动识别验证码（ddddocr），也支持手动输入
- **选课 / 退课** — 按课程号 + 课序号精确操作，支持已满时连续重试
- **浏览课程** — 在线拉取课程列表，按课程号 / 名称 / 教师搜索，过滤未满课程
- **容量检查** — 定时扫描目标课程，有空位自动抢
- **捡漏模式** — 遍历全量课程列表，见空位就抢（5~10 秒一轮）
- **打包为 exe** — PyInstaller 一键打包单文件，无需 Python 环境

## 安装

```bash
git clone https://github.com/But-For-Love/xd_xk.git
cd xd_xk

# 安装运行依赖
uv sync

# （可选）安装开发依赖（打包、代码检查）
uv sync --dev
```

## 使用

### GUI

```bash
uv run xd-xk-gui
# 或
python -m xd_xk.gui
```

### CLI

```bash
# 选课（0=必修, 1=选修）
uv run xd-xk select -c 0

# 退课
uv run xd-xk drop -c 1

# 容量检查（循环扫描，指定课程号）
uv run xd-xk check TE204003 TE204004

# 查看帮助
uv run xd-xk --help
```

## 配置

GUI 模式下无需手动编辑配置文件，填写后关闭窗口时自动保存。手动配置格式：

```json
{
  "ocr_captcha": "1",
  "debug": "0",
  "batch_name": "第一轮正选（国际创新周）",
  "bx_or_xx": 0,
  "bx": [{ "KCH": "TE204003", "KXH": "02" }],
  "xx": [{ "KCH": "FL006066" }],
  "data": {
    "loginname": "你的学号",
    "password": "你的密码",
    "captcha": "xxxx",
    "uuid": "xxxx"
  }
}
```

| 字段 | 说明 |
|------|------|
| `ocr_captcha` | `"1"` 自动识别，`"0"` 手动输入 |
| `debug` | `"1"` 将 API 响应保存到本地文件 |
| `batch_name` | 选课批次关键字，用于匹配可用批次 |
| `bx` | 必修课列表，需同时指定 `KCH` + `KXH` |
| `xx` | 选修课列表，只需 `KCH` |

> 必修课必须填课序号（小卡片上 `[01]` 这类编号），选修课不需要。

## 打包 exe

```bash
python build.py
```

产物：`dist/西电自动选课工具.exe`，单文件约 120 MB（含 onnxruntime）。

## 项目结构

```
xd_xk/
├── pyproject.toml          # uv 项目配置
├── build.py                # PyInstaller 打包脚本
├── conf.example.json       # 配置模板
└── xd_xk/
    ├── __init__.py
    ├── core.py             # 核心逻辑：登录、选课、退课
    ├── encrypt.py          # AES 密码加密
    ├── gui.py              # GUI 界面（ttkbootstrap）
    └── cli.py              # 命令行入口（argparse）
```

## 依赖

| 库 | 用途 |
|----|------|
| `ddddocr` | 验证码 OCR（onnxruntime） |
| `Pillow` | 手动模式下显示验证码 |
| `pycryptodomex` | AES-ECB 密码加密 |
| `requests` | HTTP 请求 |
| `ttkbootstrap` | GUI 主题 |

## 注意事项

- **勿将含真实密码的 `conf.json` 提交到 Git**（已加入 `.gitignore`）
- 使用选课系统需要校园网或 VPN
- 首次运行 exe 可能被 Windows Defender 拦截，点击「仍要运行」即可
