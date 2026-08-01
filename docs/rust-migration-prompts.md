# xd-xk Rust 重构 · 多轮对话提示词

> **用途**：在新会话（或新 Claude Code 实例）中粘贴，把已完成的设计与决策交给 AI，直接续做后续开发，不用重新调研。
>
> **配套文件**（AI 续做前必读）：
> - `docs/rust-migration-analysis.md` — 重构设计依据（功能清单 / 接口契约 / 技术选型 / OCR 方案 / clap 设计 / gpui 预留 / 迁移路线）
> - `docs/examples/config.toml`、`docs/examples/courses.csv` — 已定稿的新配置格式
> - `xd_xk/core.py`、`xd_xk/cli.py`、`xd_xk/encrypt.py`、`xd_xk/gui.py`、`build.py` — Python 源码，是移植的事实来源
> - `CLAUDE.md` — 项目说明

---

## 提示词 A · 总启动提示词（幂等，续跑通用）

> 每个新会话先粘贴这一条。它让 AI 先评估当前进度，从上次停下的地方继续，而不是重做。

```text
你是资深 Rust 工程师。项目 xd-xk 是西安电子科技大学自动选课工具，当前是 Python 版
（Tkinter GUI + argparse CLI + ddddocr OCR）。团队已决定重构为 Rust，并已完成完整设计分析。
你的任务：先阅读设计文档、评估当前进度，然后从上次停下的地方按阶段继续实现。
全程遵循设计文档的结论，不要擅自推翻设计。

【第一步：建立上下文（先读再动手）】
1. 读 CLAUDE.md（项目说明）。
2. 读 docs/rust-migration-analysis.md —— 必读，整个重构的设计依据：
   功能清单(§2) / 接口契约(§3) / 技术选型(§4) / OCR 方案(§5) / clap 设计(§6) /
   配置格式(§6.5) / gpui 预留(§7) / 工程结构(§8) / 风险(§9)。
3. 读 docs/examples/config.toml 与 docs/examples/courses.csv —— 已定稿的新配置格式。
4. 读 Python 源码 xd_xk/core.py、xd_xk/cli.py、xd_xk/encrypt.py、xd_xk/gui.py、build.py
   —— 它们是移植的事实来源，任何行为差异以 Python 源码为准。

【第二步：评估当前进度】
- 查看是否已有 crates/ 目录、Cargo.toml、Cargo.lock；git log 最近提交。为了避免与原python项目混乱，新创建文件夹，在xd-xk-rs里完成任务
- 按报告 §8.2 的四阶段（P0 核心库 / P1 CLI / P2 OCR / P3 打磨 / P4 GUI）判断做到哪一步。
- 已完成的不要重做；先报告「现状 + 下一步」，然后继续。粘贴后每完成一个阶段汇报一次。

【硬性约束（违反会导致线上选课失败，必须 1:1 复刻 Python 行为）】
1. 登录 / add / del 的请求参数走 URL query（reqwest .query()）；
   get_class 走 JSON body（.json()），且带 Content-Type: application/json;charset=UTF-8。
2. add/del 前把 token 同时塞进 Cookie（键名 Authorization）与请求头。
3. 必修(category=0) 选课 clazzType=FANKC、退课 TJKC；选修(1) 都是 XGKC；
   只有选修退课才带 chooseVolunteer=1。
4. add 终止消息是 5 条「子串匹配」；dele 是 2 条「精确匹配」（报告 §2.1 有逐字清单）。
5. 必修课程从每门课嵌套的 tcList 里按 KXH 匹配子项；选修直接平铺行。
6. 轮询间隔：add/del 1s；check 0.5s；snipe 5~8s。选课高峰期节奏不要改。
7. 旧 conf.json 的布尔是字符串 "0"/"1"；迁移到 config.toml 后才是真布尔。
8. 用户可见文案保持中文，与 Python 版一致。
9. AES-128-ECB + PKCS7，密钥 MWMqg2tPcDkxcm11，输出 Base64（见 encrypt.py）。

【代码结构要求】
- cargo workspace：crates/xd-xk-core（纯业务，无 UI、无 cli 依赖）、
  crates/xd-xk-cli（clap 壳）、crates/xd-xk-gui（stub，feature="gui"，不进默认构建）。
  workspace 默认 default = ["cli"]。
- core 全 async（tokio）；日志用 tracing（不要用回调），GUI 未来靠 subscriber 接事件；
  取消用 tokio_util::sync::CancellationToken。
- OCR 抽象成 CaptchaOcr trait：默认实现用方案 A（ort + 复用 Python 的 common_old.onnx，
  模型可从 .venv/Lib/site-packages/ddddocr/ 拷贝），tract 纯 Rust 留作 feature；
  人工验证码做 ManualOcr fallback（临时 PNG + 系统看图 + stdin 输入）。
- 配置：toml_edit 读写 config.toml（round-trip 保留注释）；课程文件 courses.rs
  支持 CSV（默认，写 UTF-8 BOM，读时编码嗅探 UTF-8→GBK + 魔数识别 xlsx）；
  xlsx feature 用 calamine(读) + rust_xlsxwriter(写)；
  首次运行检测旧 conf.json 自动 migrate 出 config.toml + courses.csv。
- 测试：wiremock 假服务器 mock 全部 5 个端点（/auth/captcha、/auth/login、
  /elective/clazz/list、/elective/clazz/add、/elective/clazz/del）；
  夹具来自 Python debug dump（captcha_pac.json / login_pac.json / classlist.json），
  把验证码 base64 解成 PNG 做 OCR 回归。
- 错误处理 thiserror + anyhow；CLI 退出码约定见报告 §6.3(3)。

【GUI（重要，勿提前做）】
- 现在不要实现 GUI。只需按报告 §7.2 把核心库接口留好：
  tracing 事件流、async + CancellationToken、CaptchaOcr trait、强类型操作结果。
  GUI 等 gpui 生态稳定（P4）后再做。

【验收方式】
- 每完成一个阶段：cargo fmt / cargo clippy -D warnings / cargo test 全绿。
- 汇报用短清单：完成什么 / 测试结果 / 遗留问题 / 下一步建议。不要贴大段代码。

【工作方式】
- 设计文档没覆盖的决策点，先问用户，不要自作主张。
- 若发现文档与现实冲突（如接口字段变了），先指出再改，不要默默偏离。
```

---

## 提示词 B · 指定阶段继续

> 当你想让 AI 跳过评估、直奔某个阶段时，粘贴 A 之后再贴这一条（替换 `<阶段名>`）。

```text
继续 xd-xk Rust 重构（上下文见 docs/rust-migration-analysis.md，先读）。
本次目标：完成并交付 <阶段名> 阶段。

各阶段目标与验收标准：
- P0 核心库：reqwest + serde + aes + toml_edit + csv 打通四个接口与会话，
  含旧 conf.json 迁移；wiremock 全流程 test 通过（登录→批次→取课→add/del）。
- P1 CLI：clap 全部子命令（select/drop/check/list/login/conf），--dry-run/--json/
  退出码/shell 补全，人工验证码 fallback；mock 与真实 VPN 下与 Python CLI 对拍。
- P2 OCR：实现 CaptchaOcr trait 的 OrtOcr（复用 common_old.onnx）+ ManualOcr；
  用夹具集做识别率回归，识别率与 Python 版持平。
- P3 打磨：indicatif 进度、Ctrl+C 优雅退出(退出码130)、XD_XK_USERNAME/PASSWORD
  环境变量、CI（clippy/fmt/test）、winres 打包单文件。
- P4 GUI：暂缓，仅在 gpui 生态稳定后启动。

开始前先报告：当前 git 状态 + 上一步进度 + 你打算先动哪些文件。
```

---

## 提示词 C · 收尾 / 交付检查

> 全部功能完成后，粘贴这一条做整体验收。

```text
xd-xk Rust 重构已实现完毕。请做一次最终交付检查，产出简短报告：

1. 功能对照：对照 docs/rust-migration-analysis.md §2 的清单，逐项确认已移植，
   列出任何遗漏或与 Python 行为不一致的点。
2. 质量门禁：cargo fmt --check、cargo clippy -- -D warnings、cargo test、cargo build --release
   全部通过；给出各命令输出摘要。
3. 配置兼容：确认旧 conf.json 自动迁移逻辑有测试覆盖，config.toml 与 courses.csv
   读写正确（含 BOM / GBK / xlsx 魔数嗅探路径）。
4. 文档：按需更新 CLAUDE.md（新命令、依赖、构建方式）；确认 README 与实现一致。
5. 已知问题：列出仍开放的风险（如 OCR 识别率、onnxruntime 打包体积、未测的真实网络路径）。
6. 下一步：给出 P4 GUI 启动前的最后准备清单。
```

---

## 附 1 · AI 续做时必读文件清单

| 文件 | 为什么读 |
|---|---|
| `docs/rust-migration-analysis.md` | 设计依据：全部结论、契约、路线、风险 |
| `docs/examples/config.toml` | 新配置格式（真布尔、分区、注释） |
| `docs/examples/courses.csv` | 课程文件格式（四列 = GUI 选课池） |
| `xd_xk/core.py` | 业务逻辑事实来源（登录/选课/退课/轮询） |
| `xd_xk/cli.py` | CLI 行为事实来源（当前 argparse 版） |
| `xd_xk/encrypt.py` | AES-128-ECB + PKCS7 + 密钥 |
| `xd_xk/gui.py` | 未来 GUI 要保留的能力清单（§2.4） |
| `CLAUDE.md` | 项目运行/构建命令 |

## 附 2 · 关键契约速查（不可从代码推导的关键事实）

- **BASE_URL**：`https://xk.xidian.edu.cn/xsxk`（需校园网/VPN）
- **AES**：AES-128-ECB + PKCS7，密钥 `MWMqg2tPcDkxcm11`，Base64 输出
- **请求方式**：登录/add/del = URL query；get_class = JSON body（带 Content-Type）
- **Cookie**：add/del 时 `Authorization` 同时进请求头与 Cookie
- **clazzType**：必修 选课 FANKC / 退课 TJKC；选修 均为 XGKC；选修退课带 chooseVolunteer=1
- **add 终止消息（子串匹配）**：该课程已在选课结果中 / 所选课程与已选课程冲突 / 所选课程人数已满 / 操作成功 / 选课门数或学分超过
- **dele 终止消息（精确匹配）**：所选课程与已选课程冲突 / 操作成功
- **课程字段**：KCH KXH KCM JXBID secretVal SKJS numberOfSelected classCapacity SFYX tcList
- **必修嵌套**：真实操作对象在每门课的 tcList 子项（按 KXH 匹配）；选修平铺
- **轮询间隔**：add/del 1s；check 0.5s；snipe 5~8s
- **判据**：`SFYX=="0"` 是 check 的预筛；最终以「已选<容量」为准；snipe 不看 SFYX

## 附 3 · 会话建议

- 建议在 git 上新建分支（如 `rust-migration`）再开 workspace，与 Python 版并存。
- Python 版保持可用，作为对拍基准与真实网络的对照组。
- 每个阶段结束让 AI 写一次简短交接摘要，方便下个会话的「提示词 A」快速对齐进度。
