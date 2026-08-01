//! xd-xk 命令行（clap 壳）。
//!
//! 结构：`lib.rs`（入口调度）+ `args.rs`（clap 定义）+ `ops.rs`
//! （select/drop/check/list/login）+ `conf_cmd.rs`（conf 子命令）+
//! `completions.rs`（shell 补全）。逻辑全部在 `xd-xk-core`，这里只做 UI 壳。
//!
//! 退出码约定（设计文档 §6.3(3)）：
//! | 码 | 含义 |
//! |---|---|
//! | 0 | 全部课程达成 / 正常结束 |
//! | 1 | 参数 / 配置错误 |
//! | 2 | 登录失败（含验证码错误） |
//! | 3 | 网络不可达（未连校园网 / VPN） |
//! | 4 | 批次未开放 / 未匹配 |
//! | 130 | Ctrl+C 优雅退出 |

mod args;
mod completions;
mod conf_cmd;
mod ops;
mod runner;

pub use args::{
    CategoryArg, CheckArgs, Cli, Command, ConfArgs, ConfCommand, CourseId, DropArgs, ListArgs,
    SelectArgs,
};

use std::collections::HashMap;
use std::ffi::OsString;

use xd_xk_core::ocr::CaptchaOcr;

/// 从命令行参数解析并运行（`main` 入口）。
pub fn run_from<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    runner::run_from(args)
}

/// 使用当前环境变量运行（测试可注入 `Cli`）。
pub fn run(cli: Cli) -> i32 {
    runner::run(cli)
}

/// 使用指定环境变量映射与 OCR 实现运行（测试注入 FakeOcr 用）。
pub fn run_with(cli: Cli, env: &HashMap<String, String>, ocr: Option<Box<dyn CaptchaOcr>>) -> i32 {
    runner::run_with(cli, env, ocr)
}

/// 异步入口：在调用方已有的 runtime 内运行（wiremock 测试用）。
pub async fn run_async(
    cli: Cli,
    env: &HashMap<String, String>,
    ocr: Option<Box<dyn CaptchaOcr>>,
) -> i32 {
    runner::run_async(cli, env, ocr).await
}
