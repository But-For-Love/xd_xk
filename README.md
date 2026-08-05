# XDXK Rust 版

用 Rust 重构的西电选课工具，功能与 Python 原版保持一致：验证码识别、登录、选课、退课、只读兼容性检测。原项目文件保持不动，本目录是独立的新 Cargo 项目。

## 技术栈

- `tokio`：异步运行时与 `async/await` 架构
- `clap`：CLI 子命令与参数解析
- `ddddocr`（ddddocr-rs）+ `ort`：ONNX Runtime 离线验证码识别，替代 Python `ddddocr`
- `reqwest`：异步 HTTP 请求与 Cookie 管理
- `aes` / `base64`：与 Python `pycryptodome` 一致的 AES-128-ECB + PKCS#7 + Base64 密码加密
- `thiserror` / `anyhow`：分层错误处理
- `dotenvy`：`.env` 配置

> 注意：为了简单直观，所有交互输入（包括密码）都是明文显示，不会做隐藏输入。

## 准备

1. 准备 ONNX 模型（默认读取项目根目录 `ddddocr.onnx`）。如果本机已经安装了 Python `ddddocr`，可直接运行：

   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts/setup-model.ps1
   ```

   或 Linux/macOS：

   ```bash
   bash scripts/setup-model.sh
   ```

   也可以自行把 `common_old.onnx` 放到项目根目录并命名为 `ddddocr.onnx`，或通过 `XK_OCR_MODEL` / `--model` 指定路径。

2. 创建配置：

   ```powershell
   Copy-Item .env.example .env
   ```

3. 编译：

   ```powershell
   cargo build --release
   ```

## 使用

不带任何参数直接运行会进入与原版一致的主菜单：

```text
西电选课工具
1. 正常选课
2. 只读兼容性检测（推荐先运行）
3. 编辑配置
4. 退课
0. 退出
```

推荐顺序：

```powershell
# 1. 编辑配置，填写学号、密码、课程
cargo run --release
# 菜单里选 3

# 2. 只读兼容性检测
cargo run --release
# 菜单里选 2

# 3. 正常选课
cargo run --release
# 菜单里选 1
```

也可以直接使用子命令（适合脚本化）：

```text
xd-xk-rust [OPTIONS] <COMMAND>

Commands:
  menu    交互式主菜单（不带参数时默认进入）
  select  正常选课
  drop    退课
  compat  只读兼容性检测（推荐先运行）
  config  编辑配置；加 --show 只显示当前配置
```

选课/退课还支持临时指定课程类别与课程：

```powershell
# 选修课
cargo run --release -- select --category 1 --courses "FL006066,FL006121"

# 必修课只尝试一次
cargo run --release -- select --category 0 --courses "TE204004:06,TE204004:07" --once

# 退课
cargo run --release -- drop
```

## 容错行为

- 网络超时、非 JSON 响应、字段缺失和登录失败会显示原因并返回主菜单；
- 连续选课或退课发生 5 次网络/协议错误后，会停止当前课程操作；
- 按 `Ctrl+C` 会取消当前操作并返回主菜单，而不是退出整个程序；
- 缺少验证码模型或 ONNX Runtime 不可用时会给出 `scripts/setup-model.ps1` 提示，启动菜单本身不会闪退。

## 配置项

全部配置统一保存在被 Git 忽略的 `.env`：

- `XK_LOGINNAME` / `XK_PASSWORD`：学号与密码，留空时每次启动交互输入
- `XK_OCR_CAPTCHA`：`1` 自动识别验证码，`0` 保存图片后手动输入
- `XK_DEBUG`：`1` 保存接口原始响应（`login_pac.json` 等）
- `XK_BATCH_KEYWORD`：批次名称关键字，留空自动选择第一个可选批次
- `XK_CAMPUS`：课程列表接口使用的校区代码
- `XK_REQUEST_TIMEOUT` / `XK_REQUEST_INTERVAL`：请求超时与重试间隔
- `XK_MAX_ATTEMPTS`：单课最大尝试次数，`0` 表示不限制
- `XK_CATEGORY`：`0` 必修 / `1` 选修
- `XK_REQUIRED_COURSES`：必修课，格式 `课程号:课序号`，逗号分隔
- `XK_ELECTIVE_COURSES`：选修课，只填课程号，逗号分隔
- `XK_OCR_MODEL`：ONNX 模型路径

## 关于 ddddocr-rs 的本地补丁

`ddddocr` crate 0.1.0 对标准 `common_old.onnx` 存在一个输出类型问题：模型输出是 `f32` logits（`[seq_len, 1, 8210]`），原库却按 `i64` 索引读取。本项目将该 crate 以 MIT 许可 vendored 到 `vendor/ddddocr`，补上了“f32 按时间步 argmax + CTC 解码”的逻辑，推理后端仍是 `ort`。改动只涉及输出解码，没有改变模型与字符集。
