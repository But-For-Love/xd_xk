# 西电自动选课工具

西安电子科技大学自动选课工具，通过模拟 HTTP 请求与 [xk.xidian.edu.cn](https://xk.xidian.edu.cn) 交互，实现自动登录、验证码识别、选课与退课。

## 功能

- **自动登录** — AES-ECB 加密密码，自动获取验证码
- **验证码识别** — 基于 [ddddocr](https://github.com/sml2h3/ddddocr) 的 OCR，也支持手动输入
- **自动选课/退课** — 支持必修课（按课程号 + 课序号）和选修课（按课程号）批量操作
- **课容量监控** — 持续轮询指定课程余量，有余量时自动抢课
- **GUI 界面** — Fluent Design 风格图形界面（tkinter + ttkbootstrap）
- **命令行模式** — 无 GUI 环境也可运行

## 安装

```bash
pip install -i https://pypi.tuna.tsinghua.edu.cn/simple -r requirements.txt
```

| 依赖 | 用途 |
|---|---|
| `ddddocr` | 验证码 OCR 识别 |
| `matplotlib` | 手动模式下显示验证码图片 |
| `pycryptodome` | AES 加密密码 |
| `requests` | HTTP 请求 |
| `ttkbootstrap` | GUI Fluent Design 主题 |

## 使用方式

### GUI（推荐）

```bash
python gui.py
```

启动时自动加载 `conf.json`，在界面中配置课程、查看操作日志；关闭窗口时自动保存配置。

### 命令行

```bash
python xk_main.py
```

按 `conf.json` 配置自动执行一轮选课。在 `if __name__` 块中可切换不同功能：

| 函数 | 用途 |
|---|---|
| `main()` | 按 conf.json 执行选课 |
| `add_1(cat, kc, always)` | 编程式选课，可传自定义课程列表 |
| `del_1(cat, kc, always)` | 编程式退课 |
| `check()` | 课容量轮询监控，有余量时自动抢课 |

## 配置

编辑 `conf.json`：

```json
{
  "ocr_captcha": "1",
  "debug": "0",
  "batch_name": "第一轮正选（国际创新周）",
  "bx_or_xx": 0,
  "bx": [],
  "xx": [
    { "KCH": "24TS1218" },
    { "KCH": "24TS1256" }
  ],
  "data": {
    "loginname": "",
    "password": "",
    "captcha": "xxxx",
    "uuid": "xxxx"
  }
}
```

| 字段 | 说明 |
|---|---|
| `ocr_captcha` | `"1"` 自动识别，`"0"` 手动输入 |
| `debug` | `"1"` 将 API 响应保存为 JSON 调试文件 |
| `batch_name` | 选课批次关键字（如 `"2025级"`），用于自动匹配当前开放批次 |
| `bx_or_xx` | `0` = 必修课，`1` = 选修课 |
| `bx` | 必修课列表，每项需 `KCH`（课程号）和 `KXH`（课序号） |
| `xx` | 选修课列表，每项只需 `KCH`（课程号） |
| `data` | 登录凭据，留空则运行时提示输入 |

> **注意**：`batch_name` 不匹配时会报错并列出所有可用批次，方便排查。

## 项目结构

```
.
├── gui.py          # GUI 入口
├── xk_main.py      # 命令行入口
├── func.py         # 核心：登录、验证码、选课、退课
├── encrypt.py      # AES-ECB 加密模块
├── conf.json       # 配置文件
├── requirements.txt
└── README.md
```

## API 端点

所有接口基于 `https://xk.xidian.edu.cn/xsxk/`：

| 端点 | 方法 | 说明 |
|---|---|---|
| `auth/captcha` | POST | 获取验证码图片及 UUID |
| `auth/login` | POST | 登录，获取 token |
| `elective/clazz/list` | POST | 获取可选课程列表 |
| `elective/clazz/add` | POST | 选课 |
| `elective/clazz/del` | POST | 退课 |

## 注意事项

- **必修课** 必须填写课序号 (`KXH`)，**选修课** 只需课程号 (`KCH`)
- `add()` / `dele()` 默认 `always=1` 时会每秒重试，直到成功或遇到冲突
- `check()` 中轮询的课程号是硬编码的，使用前需在代码中自行修改
- `batch_name` 需与当前选课轮次匹配，不同年级/学期需更新
- 请勿将含真实密码的 `conf.json` 提交到公开仓库

## 许可

仅供学习交流使用。
