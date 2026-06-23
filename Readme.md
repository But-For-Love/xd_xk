# 西电自动选课工具

西安电子科技大学自动选课工具，通过模拟 HTTP 请求与 [xk.xidian.edu.cn](https://xk.xidian.edu.cn) 交互，实现自动登录、验证码识别、选课与退课。

## 快速开始

```bash   ”“bash   “bash”;“bash
# 1. 安装依赖

# 2. 启动 GUI

```


## 配置文件

GUI 模式下无需手动编辑 `conf.json`，关闭窗口时自动保存。手动编辑时的格式：

```json   ' ' ' json
{
  "ocr_captcha": "1",   "ocr_captcha": "1",   "ocr_captcha": "1",
  "debug": "0",   "debug": "0",
  "batch_name": "第一轮正选（国际创新周）",
  "bx_or_xx": 0,   "bx_or_xx": 0,
  "bx": [   "bx": [
    { "KCH": "TE204003", "KXH": "02" }{ "KCH": "TE204003", "KXH": "02" }
  ],
  "xx": [   "xx": [
    { "KCH": "FL006066" }   { "KCH": "FL006066" }
  ],
  "data": {   "data": {
    "loginname": "你的学号",   "loginname": "你的学号",
    "password": "你的密码",   "password": "你的密码",
    "captcha": "xxxx",   "captcha": "xxxx",
    "uuid": "xxxx"   "uuid": "xxxx"
  }
}
```

| 字段 | 说明 |
|---|---|
| `ocr_captcha` | `"1"` 自动识别验证码，`"0"` 手动输入 |
| `debug` | `"1"` 将 API 响应保存为 JSON 调试文件 |
| `batch_name` | 选课批次关键字，用于自动匹配当前开放批次 |
| `bx_or_xx` | `0` = 必修，`1` = 选修（GUI 自动管理，无需手动改） |
| `bx` | 必修课列表，每项需 `KCH` + `KXH` |
| `xx` | 选修课列表，每项只需 `KCH` |
| `data` | 登录凭据，留空则运行时提示输入（密码明文存储，勿上传公开仓库） |

> **选课批次**：`batch_name` 不匹配时会自动列出所有可用批次，方便排查。不同年级修改为对应关键字即可（如 `"2025级"`）。

---

## 依赖

| 依赖 | 用途 |
|---|---|
| `ddddocr` | 验证码 OCR 识别 |
| `matplotlib` | 手动模式下显示验证码图片 |
| `pycryptodome` | AES-ECB 密码加密 |
| `requests` | HTTP 请求 |
| `ttkbootstrap` | GUI Fluent Design 主题 |

---

## 注意事项

- **必修课必须填课序号**（小卡片上 `[01]` 这类编号），选修课不需要
- 选课/退课失败时会每秒自动重试，直到成功或冲突
- `check()` 里的课程号是硬编码的，用之前需要自行改代码
- **不要**把含真实密码的 `conf.json` 上传到 GitHub
- 访问选课系统可能需要校园网或 VPN

---

## 许可

仅供学习交流使用。
