# 西电自动选课工具 Rust 重构可行性分析报告

> 分析日期：2026-08-01
> 分析对象：`xd_xk` v2.0.0（Python：Tkinter GUI + argparse CLI）
> 目标：全功能移植到 Rust，clap 重做 CLI，gpui/gpui-component 作为未来 GUI 方案，代码为 GUI 预留扩展点。

---

## 1. 结论摘要

**可行，且是值得做的重构。** 这个项目的核心业务面其实很窄：4 个 HTTP 接口 + 一个 AES 加密 + 一个验证码 OCR + 若干轮询循环。没有数据库、没有持久化服务端状态、没有复杂的并发模型，非常适合用 Rust 的 `reqwest + serde + tokio` 完整复刻。

| 维度 | 结论 | 说明 |
|---|---|---|
| 核心功能移植 | ✅ 完全可以全量移植 | 全部业务集中在 `xd_xk/core.py`（约 560 行），HTTP 面窄 |
| CLI 重构 | ✅ 提升空间很大 | 现有 argparse 有 8 处明显的人体工学缺陷（见 §6） |
| 验证码 OCR | ⚠️ 可行，需选型 | Rust 生态有 4 条路线，d≥有效但成熟度分层（见 §5） |
| GUI（gpui） | 🔒 暂缓，但代码须预留 | gpui pre-1.0、git 依赖、接口不稳；先做核心库与 CLI，GUI 后置（见 §7） |
| 打包分发 | ✅ 优于现状 | PyInstaller 单 exe → `cargo build --release`，体积与启动速度显著改善 |

**核心判断：** 把"核心逻辑"与"UI/CLI 壳"彻底分离后（对应 Python 里 `core.py` 与 `cli.py`/`gui.py` 的边界），Rust 版可以在**不损失任何功能**的前提下，把 CLI 的人体工学、可脚本化程度、分发体验都提升一档。GUI 用 gpui-component 是合理的中长期目标，但现阶段（2026-08）gpui 生态仍以 git 依赖为主，**不建议在迁移第一版就把 GUI 作为阻塞项**。

---

## 2. 功能清单盘点（重构后必须一个不落）

以下是对照 Python 源码逐函数整理的功能清单，作为 Rust 移植的验收依据（对应"需求 1：核心功能必须一个不落"）。

### 2.1 核心逻辑（`xd_xk/core.py`）

| 功能 | Python 函数 | 行为要点（移植时必须 1:1 复刻） |
|---|---|---|
| 验证码获取 | `get_captcha()` | POST `/xsxk/auth/captcha`；解析 `data.captcha`（`data:image/png;base64,` 前缀剥离 → base64 解码）；`ocr_captcha=1` 走 OCR，否则走 `captcha_cb` 回调；返回 `(code, uuid)` |
| 验证码识别 | `ocr_captcha()` | ddddocr → Rust 换成 Ocr trait 实现（见 §5） |
| 登录 | `login()` | AES-ECB 加密密码；**loginname/password/captcha/uuid 以 URL query 提交**；从 `data.token` 取令牌；返回 `(json, cookie_jar)` |
| 学生信息与批次 | `show_msg()` / `get_batch_list()` / `match_batch_name()` | 读 `data.data.student.{XM,ZYMC,schoolClass}`；`electiveBatchList` 逐条列出 `name/code/canSelect`；按关键字子串匹配批次，优先返回**可选**（`canSelect=="1"`）批次 |
| 批次匹配异常路径 | `_match_batch()` | 三种错误文案必须区分：①没找到含关键字的批次 ②匹配到但未开放 ③匹配到但拿不到 code |
| 切换批次 | `choose_batch()` | POST `/xsxk/elective/user`（`batchId` 参数）。当前 CLI/GUI 未调用，属保留 API，仍应移植 |
| 课程列表 | `get_class()` | POST `/xsxk/elective/clazz/list`，**JSON body**：`teachingClassType`（`FANKC`=必修/`XGKC`=选修）、`pageNumber:1`、`pageSize:300`、`orderBy:""`、`campus:"S"`；需 `Content-Type: application/json` 头 |
| 批量取课 | `fetch_courses()` | 按 category 集合循环 `get_class`，返回 `{category: rows}` |
| 选课 | `add()` | POST `/xsxk/elective/clazz/add`，**URL query**：`clazzType`（必修 `FANKC`/选修 `XGKC`）、`clazzId=JXBID`、`secretVal`、`chooseVolunteer:1`；cookie 中注入 `Authorization`；`always=1` 时轮询，命中终止消息或 `stop_event` 停止 |
| 退课 | `dele()` | POST `/xsxk/elective/clazz/del`，**URL query**：`clazzType`（必修 `TJKC`/选修 `XGKC`）；**只有选修**才带 `chooseVolunteer:1`；终止消息集合与 add 不同（见下） |
| 轮询模板方法 | `_poll_operation()` | 循环：POST → 读 `json()["msg"]` → sleep 1s → 用 `-` 符号计数进度；`add` 终止条件为 5 条消息的**子串匹配**，`dele` 为 2 条消息的**精确匹配** |
| 会话外观 | `CourseSession` | 打包 `token`/`cookie`/`batch_code`/原始 `data`，`create()` = 登录→展示→匹配批次 |

**关键终止消息（必须逐字保留）：**
- `_STOP_MSGS`（add）：`该课程已在选课结果中`、`所选课程与已选课程冲突`、`所选课程人数已满`、`操作成功`、`选课门数或学分超过` —— **子串匹配**。
- dele：`所选课程与已选课程冲突`、`操作成功` —— **精确匹配**。

### 2.2 加密（`xd_xk/encrypt.py`）

| 功能 | 要点 |
|---|---|
| AES-ECB 加密 | 密钥 `MWMqg2tPcDkxcm11`（16 字节 = AES-128），PKCS7 填充，输出 **Base64**。Rust 用 `aes` crate 的 `Aes128` + `BlockEncrypt` 手工 ECB + `pkcs7` 填充，或直接 `cipher` + `cbc` 不行（是 ECB） |

### 2.3 CLI（`xd_xk/cli.py`）

| 功能 | 要点 |
|---|---|
| `select` | 登录 → 匹配批次 → 按 category 拉课程 → 对 conf 中每门课按 `KCH`（必修再按 `tcList` 内 `KXH`）匹配 → `add()`；`--once` 只发一次，否则持续轮询 |
| `drop` | 同上，调 `dele()` |
| `check` | 登录一次；循环 0.5s 间隔，对每个 category 的 `get_class()` 扫描目标 `KCH`，`SFYX=="0"` 且 `已选<容量` 时 `add(always=0)`；Ctrl+C 停止 |
| 人工验证码 | `_manual_captcha()`：写临时 PNG → PIL 显示 → stdin 输入 |

### 2.4 GUI（`xd_xk/gui.py`，未来 gpui 版须保留的能力）

| 区块 | 功能 |
|---|---|
| 登录表单 | 学号/密码输入、"自动验证码"开关、"调试"开关、批次下拉 + "获取批次"按钮（登录后列出批次及可选状态，自动选中第一个可选批次） |
| 选课池 | 表格（类别/课程号/课序号/课程名）、增删改（双击编辑）、全选/全不选、"浏览课程"、"添加未满课程" |
| 课程浏览对话框 | 后台线程拉两类课程，搜索（KCH/KCM/教师）、"只显示未满"过滤、✔ 列多选、全选/全不选、添加到选课池 |
| 操作区 | 选课/退课/容量检查/■停止、"连续重试"开关、🎯捡漏（必修/选修下拉） |
| 捡漏模式 | 5~8s 一轮，扫 `sel<cap` 的空位，**必修在 `tcList` 内按子项判断**，已抢去重（`KCH` 或 `KCH|KXH`），5s/8s 动态间隔 |
| 容量检查模式 | 与 CLI check 相同逻辑但支持停止按钮 |
| 消息协议 | `Msg(kind, payload)` + `queue.Queue` + 主线程轮询；kind ∈ {log, err, done, st, ok}；`stop_event` 取消；配置自动保存/加载 |

### 2.5 打包（`build.py`）

PyInstaller onefile → Rust 用 `cargo build --release`（CLI）；GUI 期用 `winres`（图标/版本资源）+ NSIS/WiX 或简单 `cargo-bundle`。附带 `conf.example.json`。

---

## 3. 网络接口契约（Rust 移植最容易踩坑的地方）

逐条核对 Python 源码后，**以下细节必须原样保留**，否则登录/选课会静默失败：

1. **登录是 query 不是 body**：`requests.post(url, headers, params=form, ...)` → `params` 走 URL query。Rust 侧是 `client.post(url).query(&form)`，**不是** `.json(&form)`。
2. **add/del 也是 query**：`requests.post(url, params=form, headers, cookies)` → `.query()`。
3. **get_class 是 JSON body**：`.post(url, json=form)` → `.json()`，且头部要有 `Content-Type: application/json;charset=UTF-8`。
4. **Cookie 里也要放 token**：`add`/`dele` 前会 `cookie["Authorization"] = token`，即请求同时带 `Authorization` 头 **和** `Cookie: Authorization=<token>`。为保险 1:1 复刻两者都发。
5. **User-Agent** 固定为 Edge 字符串（core.py 常量），服务器可能校验。
6. **字段名**：课程用 `KCH/KXH/KCM/JXBID/secretVal/SKJS/numberOfSelected/classCapacity/SFYX`；必修课程行内含嵌套 `tcList`，**真正要 add/del 的是 `tcList` 里的子项**（选修是平铺行直接用）。
7. **`SFYX=="0"` 语义**：文档说 `"0"`=有余量，但实际代码以 `已选<容量` 为最终判据（`SFYX` 只是 check 模式的预筛），snipe 模式完全不看 `SFYX`。**以代码行为为准**。
8. **响应解析**：轮询循环直接读 `json()["msg"]`，字段缺失会抛异常——Rust 侧用 serde 结构体解析并对缺失字段报可读错误。
9. **批量获取用 set 去重**：`fetch_courses` 接收 `set[int]`，Rust 侧 `HashSet<Category>`。
10. **conf.json 布尔值是字符串 `"0"/"1"`**：保持兼容，别改成 JSON 布尔。

---

## 4. 技术选型映射表

| 能力 | Python | Rust | 说明 |
|---|---|---|---|
| HTTP | `requests` | `reqwest` (0.12) + `cookie_store` | 需要保留 cookie 会话；blocking 或 async 均可，建议 async + tokio（GUI 复用同一套异步核心） |
| JSON | 内置 | `serde` / `serde_json` | 用 `#[derive(Deserialize)]` 建 `CourseRow`、`LoginResp` 等强类型 |
| 密码加密 | `pycryptodomex` | `aes` (0.8) + `base64` + `pkcs7` | AES-128-ECB 手工实现，约 20 行 |
| OCR | `ddddocr` | 见 §5 四选一 | 抽象成 `CaptchaOcr` trait，GUI 未来可换 |
| 配置 | JSON 文件 | `toml_edit`（配置）+ `csv`（课程，默认）+ `calamine`/`rust_xlsxwriter`（xlsx，feature）+ `encoding_rs`（GBK 嗅探） | **配置与课程数据分离**：`config.toml` 存凭据/设置，`courses.csv\|xlsx` 存课程（见 §6.5） |
| 日志 | `logging` | `tracing` + `tracing-subscriber` | GUI 未来通过 subscriber 把日志接入消息队列，替代 Python 的 `log_func` 回调 |
| CLI | `argparse` | `clap` (4, derive) + `clap_complete` | §6 详述 |
| 进度显示 | `print` | `indicatif` | check/snipe 的轮询进度与 Ctrl+C 优雅退出 |
| 错误处理 | `RuntimeError` | `thiserror` + `anyhow` | 保留中文字面文案（用户已习惯） |
| 异步/取消 | `threading.Event` | `tokio` + `tokio_util::sync::CancellationToken` | `stop_event` → `CancellationToken`；Ctrl+C → `tokio::signal::ctrl_c` |
| 临时图显示 | `PIL.show` | `image` + 系统打开（`cmd /c start`/`xdg-open`） | CLI 人工验证码沿用"临时 PNG + 系统查看器" |
| 打包 | PyInstaller | `cargo build --release` + `winres` | CLI 直接单文件；GUI 期再引入安装器 |

---

## 5. ddddocr 的 Rust 替代方案调研（需求 3）

> 结论先行：**核心约束不是"没有 Rust 实现"，而是"成熟度分层 + 模型/二进制体积 + 打包复杂度"**。验证码 OCR 的保真度可以靠**复用 Python 版同一份 ONNX 模型文件**来兜底（两个项目可以共用 `common_old.onnx`，13.6MB）。

**重要前提**：本机 Python ddddocr v1.6 自带的模型（已在 `.venv/Lib/site-packages/ddddocr/` 验证）：

| 文件 | 大小 | 用途 |
|---|---|---|
| `common.onnx` | 54.0 MB | beta 高精度分类模型 |
| `common_old.onnx` | 13.6 MB | **默认分类模型**（int8 量化 CNN+LSTM），Python `DdddOcr()` 默认加载这个 |
| `common_det.onnx` | 20.1 MB | 目标检测（本工具用不到） |

`classification()` 默认管线：解码 → 等比缩放到高度 64 → 灰度化 → 归一化 `(p/255-0.5)/0.5` → CNN-LSTM 推理 → CTC 贪心解码。

### 5.1 四条可行路线

| 方案 | 引擎 | 模型来源 | 优点 | 缺点 | 成熟度 |
|---|---|---|---|---|---|
| **A. `ort` crate 直接推理** | onnxruntime | 复用 `common_old.onnx`（13.6MB） | 模型与 Python 完全一致→**保真度最高**；体积最小 | 需自己移植预处理（约 30 行）与 CTC 解码（约 15 行）；依赖 onnxruntime 原生库（约 30-50MB） | 高（ort 月下载 ~187 万，1.15.4 稳定 / 2.0 候选） |
| **B. `86maid/ddddocr` fork** | onnxruntime | 自带 `common.onnx`+charset（约 54MB） | **开箱即用**：静态链接、跨平台预编译、支持新/旧模型、还送 OCR API Server；charset 已内置 | git 依赖（非 crates.io 正式版）；模型更大；黑盒程度高 | 中高（Rust 版，~80-334 star） |
| **C. `drission` crate（feature "ocr"）** | `tract`（纯 Rust） | 首次运行自动下载 54MB `betacommon.onnx` | **纯 Rust 无原生依赖**，交叉编译/打包最省心；文档报告对 4 位字母数字验证码 16/16 全对 | 模型 54MB 且首次需下载；对西电验证码效果需实测 | 中（tract 引擎成熟） |
| **D. `mzdk100/ddddocr` crate** | ort | 自行提供 `.onnx` | API 与 Python 几乎同名（`DdddOcr::new` + `classification`） | **0.1.0，刚起步**（近期下载量个位数到几十）；async 绑定 tokio；仍需自带模型 | 低（不建议生产依赖） |

> 注：`drission` 内部是 `betacommon.onnx`（即 Python 的 beta 模型 `common.onnx`），方案 C 与 B 实际用的是同一类高精度模型，只是引擎不同。

### 5.2 推荐策略

```
CaptchaOcr (trait)  ← 核心库只依赖这个 trait
 ├─ OrtOcr        (方案 A，默认：复用 common_old.onnx，保真优先)
 └─ TractOcr      (方案 C，备选：纯 Rust，无原生依赖)
 └─ ManualOcr     (回调：CLI 弹系统看图 + stdin；GUI 弹图对话框)
```

1. **默认推荐方案 A（`ort` + 原模型）**：与 Python 行为 1:1，模型文件可以直接从本机 `site-packages/ddddocr/` 拷过来，OCR 准确率**零风险退化**。代价是要移植一遍预处理/CTC（代码量小，可参考 ddddocr 源码），且产物要捆绑 onnxruntime DLL。
2. **构建/分发最省心是方案 C（`drission`/tract）**：没有原生依赖，交叉编译友好；但 54MB 模型体积和"首次自动下载"在离线选课场景是个隐患（教学楼网络可能没外网），**必须支持本地模型文件路径覆盖**（`DRISSION_OCR_MODEL` 或自定义配置项）。
3. **上线前务必做"夹具烘焙"**：利用现有 `debug:"1"` 功能抓一批真实验证码响应（`captcha_pac.json` 里就是含 base64 图的原始响应），做成单元测试夹具，对比三种引擎的识别率。这是决定默认引擎的唯一可靠依据。
4. **方案 B/D 建议只作为备选**：B 的"开箱即用"很诱人，但 git 依赖 + 黑盒；D 太新。

---

## 6. clap CLI 重构设计（需求 2）

### 6.1 现有 argparse CLI 的 8 处人体工学缺陷

1. **必修/选修是魔法数字** `-c 0/1`，且**语义随命令变化**（选课时 0=FANKC，退课时 0=TJKC，同一数字两种含义）。
2. **课程只能改 conf.json**：不能在命令行直接 `xd-xk select KCH KXH`，每次都得编辑 JSON。
3. **`check` 无停止条件**，只能 Ctrl+C，且无退出码（退出码恒为 0），脚本没法判断成败。
4. **conf.json 路径写死**当前工作目录，无法 `--config`。
5. **没有"先看课再选课"的命令**——无法在不打开 GUI 的前提下列出某批次课程。
6. **认证信息只能明文放 conf.json**，不支持环境变量/只读配置。
7. **没有 `--dry-run`**，误操作无法预览。
8. **select/drop/check 之间逻辑大量复制**（GUI 的 `_w_sel/_w_drop` 也重复），难维护。

### 6.2 重构后的命令模型

```
xd-xk                          # 无子命令 → 打印 help
├── select [COURSE...]         # 选课（需求高频，放第一）
│     -c, --category 选修|必修   # ValueEnum，默认 选修（平铺行，无 tcList 嵌套，最简单）
│     -i, --interactive        # 交互式：登录后从课程列表挑选
│     --once                   # 只发一次请求
│     --interval <sec>         # 轮询间隔（默认 1.0，对齐 Python）
├── drop [COURSE...]           # 退课，同族参数
├── check [KCH...]             # 容量检查（抢空位）
│     --interval <sec>         # 默认 0.5
│     --no-grab                # 只监控不自动选
│     --until <n>              # 运行 N 秒/轮后自动退出
│     --json                   # 机器可读输出（供脚本/GUI 消费）
├── list [-c 类别] [关键字]     # 新增：拉课程列表并打印（搜索过滤）
├── login                      # 新增：验证凭据/批次，打印学生信息
└── conf                       # 新增：配置管理（见 §6.5）
      init  [--user --password --config ...]   # 交互式生成 config.toml（密码优先读环境变量）
      migrate                   # 旧 conf.json → config.toml + courses.csv（首次运行自动执行）
      show                      # 打印当前配置（密码打码）
      template [--xlsx]         # 生成带表头+示例行的课程文件模板
      import <文件>              # 从 CSV/XLSX 导入课程
      export <文件>              # 导出课程到 CSV/XLSX

全局：-V/--version
      --config <path>          # 默认 ./conf.json
      -v, --verbose            # -vv 更详细
      --debug                  # 等同 conf.debug=1，dump 响应
      --dry-run                # 只走流程不真正 add/del
      --json                   # 面向脚本的结构化输出
```

### 6.3 关键人体工学设计决策

1. **`Category` 用 `ValueEnum` 取代整数**，且带向后兼容别名：
   ```rust
   #[derive(Clone, Copy, ValueEnum)]
   enum Category {
       #[value(name = "选修", alias = "1", alias = "xx")] Elective,
       #[value(name = "必修", alias = "0", alias = "bx")] Required,
   }
   ```
   用户既写 `-c 选修` 也兼容 `-c 1`；`clap` 自动生成补全候选，旧肌肉记忆不丢。

2. **课程号直接进命令行**——`CourseId` 实现 `FromStr`，支持 `KCH` 或 `KCH/KXH` 两种写法（必修用后者）：
   ```
   xd-xk select EY226022 TE204003/02
   xd-xk drop   TE204003/02
   xd-xk check  EY226022 EY226023 --until 600
   ```
   不指定 COURSE 时回退读 conf.json 的 `bx/xx`（与旧行为一致）。这是"不再被迫手改 JSON"的核心。

3. **退出码约定**（脚本化的地基）：
   | 码 | 含义 |
   |---|---|
   | 0 | 全部课程达成/正常结束 |
   | 1 | 参数/配置错误 |
   | 2 | 登录失败（含验证码错误） |
   | 3 | 网络不可达（未连校园网/VPN） |
   | 4 | 批次未开放/未匹配 |
   | 130 | Ctrl+C 优雅退出 |
   `main() -> Result<(), CliError>`，`CliError` 携带退出码。

4. **凭据免明文**：`clap` 的 `env` 特性读 `XD_XK_USERNAME`/`XD_XK_PASSWORD`，优先级 **CLI 参数 > 环境变量 > conf.json**。密码不再必须落盘。

5. **交互式选课 `-i`**：用 `inquire`（或 `dialoguer`）呈现"课程列表 → 多选/模糊搜索 → 确认"，GUI 的 CourseBrowserDialog 在 CLI 上的等价物。

6. **`--dry-run` 全命令生效**：走完整登录/匹配/匹配课程流程，但把 add/del 请求替换为打印"将执行 X"。避免手滑退错课。

7. **check 可结束**：`--until 600`（秒）或 `--rounds N`，结束时打印汇总（检查了几轮、发现几个空位、抢到几门），退出码 0。配合 `--json` 可被 CI/其他脚本调用。

8. **优雅信号处理**：`tokio::signal::ctrl_c()` + `CancellationToken`——按下 Ctrl+C 立即停止轮询、打印"已停止"，退出码 130，而不是 Python 版的裸 traceback。

9. **Shell 补全**：`clap_complete` 一行生成 bash/zsh/fish/powershell 补全脚本，`xd-xk completions <shell>` 子命令即可安装。

### 6.4 与现有能力的兼容红线

- `select`/`drop`/`check` 三个名字保留，`-c 0/1`、`--once`、位置 `KCH...` 全部兼容（alias 层完成）。
- `conf.json` **不再读写**：首次运行自动 `conf migrate` 生成 `config.toml` + `courses.csv`（`bx`→必修行、`xx`→选修行），打印迁移摘要；原文件保留不动。老用户换二进制即平滑过渡。

---

### 6.5 配置格式与课程管理文件设计（新增决策：JSON → TOML + CSV/XLSX）

#### 6.5.1 原版 conf.json 的缺陷

| 缺陷 | 后果 |
|---|---|
| 无注释 | 学生看不懂 `batch_name` 怎么填、`KXH` 何时必填 |
| 嵌套手改易错 | 少个括号/逗号整文件失效 |
| `"0"/"1"` 字符串布尔 | 反直觉，需 `_conf_bool()` 转换 |
| **课程数据与配置混放** | 加课=改"配置文件"，心智负担大（这是最别扭的一条） |
| 密码明文 | 需环境变量覆盖，可选 OS keyring |

**结论：换 TOML 值得，但真正的修复是把"数据"从"配置"里拆出来。** 凭据/设置低频变化、课程高频变化，两者合体才是原版最大的设计问题。

#### 6.5.2 新文件布局

```
config.toml     # 凭据 + 应用设置（TOML：支持注释 / 真布尔 / 分区）
courses.csv     # 课程数据（默认；可用 courses.xlsx 替代）
```

`config.toml`：

```toml
# xd-xk 配置文件
[app]
ocr_captcha = true      # 自动识别验证码；false = 手动输入
debug = false           # dump 接口响应，排查用
batch_name = ""         # 选课批次关键字，留空 = 自动选第一个可选批次

[account]
loginname = ""          # 学号；也可用环境变量 XD_XK_USERNAME
password = ""           # 密码；可用 XD_XK_PASSWORD 覆盖，不建议明文提交 git
```

旧的 `data.captcha/data.uuid` 占位字段直接删除；`"0"/"1"` → 真布尔。序列化用 **`toml_edit`**（round-trip 保留手写注释与格式），GUI 自动保存不会毁掉学生写进去的注释。

`courses.csv`：

```csv
类别,KCH,KXH,KCM
选修,EY226022,01,操作系统
必修,TE204003,02,大学物理
```

- 四列 = GUI 选课池四列，**全链路一致**（同一份数据模型）。
- `KXH` 选修可空、必修必填（决定是否走 `tcList` 嵌套匹配）。
- 表头用中文；`conf template` 生成带示例行的模板。

#### 6.5.3 CSV vs XLSX 选型

| 维度 | CSV | XLSX |
|---|---|---|
| 解析依赖 | `csv`（极轻） | `calamine`(读) + `rust_xlsxwriter`(写)，两个 crate |
| 学生打开方式 | Excel/WPS/记事本双击即开 | Excel/WPS（最原生） |
| 中文乱码 | 需 UTF-8 **BOM** + 读时嗅探 GBK | 无此问题 |
| git diff / 脚本化 | ✅ 文本可 diff | ❌ zip 二进制不可读 |
| 文件大小 | KB 级 | 几十 KB+ |
| 误存脚枪 | Excel 可能"另存为"改名成 .xlsx | 无 |
| 解析报错 | 按行列定位容易 | 错误信息较晦涩 |

**推荐：默认 CSV（UTF-8 带 BOM），同时提供 `xlsx` cargo feature。** 理由：
1. 学生最后都是在 Excel/WPS 里编辑，**CSV 双击即开，体验和 xlsx 几乎无差别**，但核心库零重依赖、可 git diff、文件极小。
2. 编码坑用两条规则封死：写入带 BOM；读取时**按扩展名 + 魔数嗅探**——以 `.csv` 命名的 xlsx（`PK\x03\x04` 魔数）也按 xlsx 解析，防"另存为"脚枪。
3. XLSX 作为可选 feature，`conf import/export` 双向转换，满足"只想用 Excel 存"的学生。

#### 6.5.4 向后兼容与 CLI 集成

- 首次运行检测旧 `conf.json` → 自动迁移（`bx`→必修行、`xx`→选修行），打印迁移摘要；原文件保留。
- `select/drop/check` 默认读 `courses.csv`（`--courses <文件>` 覆盖，按扩展名/魔数识别格式），**不再从配置读课程**。
- 可选增强：`keyring` 把密码存系统凭据库（Windows Credential Manager），失败回退 `config.toml`。

#### 6.5.5 Rust 依赖小结

`toml_edit`（配置）、`csv`（默认课程）、`calamine` + `rust_xlsxwriter`（xlsx feature）、`encoding_rs`（GBK 嗅探）、可选 `keyring`（密码入库）。

---

## 7. GUI 远期规划与代码预留（需求 4）

### 7.1 gpui / gpui-component 现状（2026-08 调研）

| 项目 | 状态 |
|---|---|
| **gpui**（Zed 的 UI 框架） | **pre-1.0**；官方只维护 git 依赖（`zed-industries/zed`），crates.io 上只有社区镜像 `gpui-unofficial`（1.10.0，2026-07）与 `gpui-ce` 等；**版本间破坏性变更频繁**；官方文档明言"仍在积极开发，常有 breaking changes" |
| **gpui Windows 后端** | 已实装 **DirectX 11 + HLSL** 后端（早先用 Blade/Vulkan 在 Windows ARM 上失败后改的），Win32 窗口 + DirectWrite 文字；Zed 本体的 Windows 稳定版已发布（团队每周对齐）；仍有已知 DX 问题（如集成/独显选择 bug #36798），但对普通桌面应用影响有限 |
| **gpui-component**（Longbridge） | 0.5.1（2026-02-05）；**crates.io 有发布**，但官方推荐 git 依赖；依赖 gpui 同源；提供 40+ 组件（Button/Input/Table/List/Dock/Chart/Theme 等，融入 Windows/macOS 原生设计语言）；**要求 Rust 1.90+，Windows 10+** |

**结论：方案本身成立、长期看好，但现在是"欠成熟期"。** 如果今天就用，会遇到：git 依赖锁定、gpui 升级导致组件库需同步、编译时长与体积大、学习曲线陡。对选课工具这种"一个窗口 + 一张表 + 几个按钮 + 日志区"的 UI，**收益不足以抵消前置风险**。

### 7.2 代码层面为 GUI 预留的 6 件事（第一版就做，成本极低）

1. **Cargo workspace 拆三个 crate**，GUI 做成 feature-gated，不进默认构建：
   ```
   crates/
   ├── xd-xk-core/   # 纯业务库：无 UI、无 cli，只依赖 reqwest/serde/tokio/tracing
   ├── xd-xk-cli/    # clap 壳，依赖 core
   └── xd-xk-gui/    # 暂为 stub（一个空 lib + feature="gui"），未来实现 gpui
   ```
   `default = ["cli"]`，CLI 用户永远不用编译 gpui。

2. **OCR 抽象成 `CaptchaOcr` trait**（§5.2），GUI 未来可插入"图片显示 + 人工输入"实现，core 零改动。

3. **日志走 `tracing`，不回调**：core 里 `tracing::info!("...")`，CLI 装 `fmt` subscriber；未来 GUI 装一个 `ChannelSubscriber` 把事件转发到 `tokio::sync::mpsc`——这正是 Python 里 `log_func`/`msg_q` 模式的 Rust 等价物，**现在用 tracing 写日志，GUI 期零重构**。

4. **core 全部暴露 `async fn` + `CancellationToken`**：GUI 未来在后台线程起一个 `tokio::Runtime` 执行，事件通过 channel 投递到主线程；和 Python 的"daemon Thread + queue"一一对应。

5. **定义 `OperationStatus`/`Msg` 事件枚举**（`LoginOk/BatchMatched/StartSelect/StopReason...`），让 CLI 的进度条和未来 GUI 的状态区消费同一份事件流，而不是各自 print。

6. **core 返回强类型结果**（`Result<SelectOutcome, AppError>`，含终止消息分类），GUI 可直接据此上色（"操作成功"→绿，"冲突"→黄，等价现有 `_dispatch_log` 的颜色逻辑）。

### 7.3 未来 gpui 界面的组件映射（备忘）

| Python/ttkbootstrap | gpui-component 候选 |
|---|---|
| 登录表单（Entry/Checkbutton/Combobox） | `Input` / `Checkbox` / `Select` |
| 选课池 Treeview | `Table`（+ `Input` 搜索） |
| 课程浏览对话框 | `Toplevel` → gpui `Window` + `Dock` 或独立弹窗 |
| 操作按钮（选课/退课/容量/捡漏/停止） | `Button`（color variant）+ `Tooltip` |
| 彩色日志区 | `TextEditor`（只读）或 `List` + 颜色 |
| 连续重试/自动验证码开关 | `Switch` |
| 停止/取消 | `CancellationToken` → 按钮 disabled 状态 |
| 配置自动存取 | core 的 `Config` 结构体，GUI 只读写 |
| 课程文件管理 | "浏览课程→添加"写回 `courses.csv`/`xlsx`；提供"打开课表"按钮（调系统 Excel/WPS） |

---

## 8. 工程结构与迁移路线

### 8.1 建议目录

```
xd-xk-rs/
├── Cargo.toml            # workspace
├── crates/
│   ├── xd-xk-core/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── api.rs        # 4 个 HTTP 端点 + 头/参数构造（§3 契约）
│   │   │   ├── session.rs    # CourseSession 等价物（token/cookie/batch_code）
│   │   │   ├── batch.rs      # 学生信息/批次匹配/错误文案
│   │   │   ├── course.rs     # CourseRow serde 结构 + tcList 匹配
│   │   │   ├── ops.rs        # add/dele/_poll_operation + 终止消息常量
│   │   │   ├── ocr.rs        # CaptchaOcr trait + ManualOcr
│   │   │   ├── ocr_ort.rs    # [feature] OrtOcr
│   │   │   ├── encrypt.rs    # AES-128-ECB + PKCS7
│   │   │   ├── config.rs     # config.toml 强类型（真布尔）+ 旧 conf.json 迁移（§6.5）
│   │   ├── courses.rs    # 课程文件读写：CSV 默认 / xlsx feature / 魔数嗅探
│   │   │   └── error.rs      # AppError + 退出码映射
│   │   └── tests/            # 用 wiremock 起假服务器（夹具来自 debug dump）
│   ├── xd-xk-cli/
│   │   └── src/main.rs       # clap 定义 + 各命令实现（§6）
│   └── xd-xk-gui/            # stub，feature="gui"
```

### 8.2 四阶段迁移（每阶段可独立交付）

| 阶段 | 内容 | 验收标准 |
|---|---|---|
| **P0 核心库** | `reqwest`+`serde`+`aes`+`config`+`session`，四个端点全部接通；`tracing` 日志；`CancellationToken` | 用 wiremock 假服务器跑通 登录→批次→取课→add/del 全流程 |
| **P1 CLI** | clap 壳（§6 全部命令），`--dry-run`/`--json`/退出码/补全；`conf migrate` + 课程文件读写；人工验证码 fallback | 与 Python CLI 在 mock 与真实 VPN 下逐命令对拍；旧 conf.json 迁移验证 |
| **P2 OCR 接入** | 实现 `OrtOcr`（复用 `common_old.onnx`），夹具集做识别率回归；按实测结果决定是否切换 TractOcr | 夹具集（≥50 张真实验证码）识别率与 Python 版持平 |
| **P3 打磨** | `indicatif` 进度、Ctrl+C 优雅退出、`XD_XK_*` 环境变量、CI（clippy/fmt/test）、`winres` 打包 | 全量 `cargo clippy -D warnings` 通过；release 单文件可双击运行 |
| **P4 GUI（远期）** | `xd-xk-gui` 用 gpui-component 实现 §7.3 界面 | 待 gpui 生态稳定（≥1.0 或官方 crates.io 发布）后再启动 |

### 8.3 测试与验证策略（重点）

- **mock 服务器**：`wiremock`（或 `mockito`）按 §3 契约伪造 `/auth/captcha`、`/auth/login`、`/elective/clazz/list`、`/elective/clazz/add`、`/elective/clazz/del`。因为登录/选课需要校园网，**mock 是核心测试路径**。
- **夹具来源**：Python 版 `debug:"1"` 会 dump `captcha_pac.json`/`login_pac.json`/`classlist.json`——这些就是现成的真实响应样本，直接转成测试 fixture（验证码图片解出来存 PNG，OCR 单测复用）。
- **对拍测试**：同一组 conf + 同一 mock 下，Python 与 Rust 版本跑同一命令，逐条对比 HTTP 请求（方法/URL/query/body/headers）是否一致。这是"功能一个不落"的最硬核验证。

---

## 9. 风险清单与缓解

| # | 风险 | 影响 | 缓解 |
|---|---|---|---|
| 1 | **OCR 识别率退化** | 登录成功率下降 | 复用同一模型文件（方案 A）+ 夹具回归 + 保留人工验证码 fallback |
| 2 | **onnxruntime 原生依赖打包** | 单文件体积 60MB+、DLL 缺失 | `ort` 的 `load-dynamic`；或切 `drission`/tract 纯 Rust 版（但模型 54MB）；发布前 Windows 虚拟机冒烟测试 |
| 3 | **gpui 生态未稳** | GUI 返工 | 按 §7.2 预留接口，GUI 整体后置；GUI 只在接口稳定后启动 |
| 4 | **HTTP 细节走样**（query/body 混用、Cookie 里塞 token） | 登录/选课静默失败 | §3 逐条固化为 mock 断言；对拍测试兜底 |
| 5 | **服务器端变更** | 接口改动导致失效 | 保持 `--debug` dump 能力；把请求构造集中到 `api.rs` 便于快速适配 |
| 6 | **Windows 中文控制台编码** | 日志乱码 | 显式 UTF-8；`cargo` 下 `win10`/`enable_ansi_support`；避免依赖 GBK 控制台 |
| 7 | **选课高峰反爬/限流** | 轮询被断 | 维持 Python 版的 1s 间隔与 5/8s snipe 节奏，不擅自加速 |
| 8 | **CSV 中文编码** | 中国版 Excel 默认 GBK，中文乱码 | 写入带 UTF-8 BOM；读取嗅探（UTF-8→GBK）；提供 xlsx feature 绕开 |
| 9 | **课程文件被学生改坏** | 解析失败 | `csv` 按行列报错；`conf template` 给模板；GUI 写入保证结构；以 `.csv` 命名的 xlsx 按魔数兜底 |

---

## 10. 参考链接

- Rust ddddocr crate（mzdk100/ddddocr-rs）：https://docs.rs/ddddocr/latest/ddddocr/ 、https://github.com/mzdk100/ddddocr-rs
- 86maid/ddddocr fork（自带模型/API Server/MCP）：https://github.com/86maid/ddddocr
- drission crate（tract 纯 Rust 实现 ddddocr）：https://docs.rs/drission/latest/drission/
- ort crate（onnxruntime 安全绑定，替代 onnxruntime-rs）：https://docs.rs/ort/latest/ort/ 、https://lib.rs/crates/ort
- rten（纯 Rust ONNX 推理，v0.23 起直接加载 .onnx）：https://lib.rs/crates/rten 、https://robertknight.me.uk/posts/rten-2025/
- ddddocr ONNX 模型说明（DeepWiki）：https://deepwiki.com/sml2h3/ddddocr/5.1-onnx-models-overview
- gpui-component（Longbridge）：https://github.com/longbridge/gpui-component 、文档 https://longbridge.github.io/gpui-component/docs/installation 、DeepWiki https://deepwiki.com/longbridge/gpui-component/2-getting-started
- gpui-unofficial（crates.io 镜像）：https://lib.rs/crates/gpui-unofficial
- Zed 官方说明 gpui pre-1.0：https://github.com/zed-industries/zed （crates/gpui 文档）
- Zed Windows 支持讨论（DirectX 后端）：https://github.com/zed-industries/zed/discussions/15764 、https://windowsforum.com/threads/zed-editor-arrives-on-windows-with-native-rust-gpu-ui-and-directx-11.384963/
