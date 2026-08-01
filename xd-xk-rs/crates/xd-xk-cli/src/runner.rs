//! 入口调度：解析、tracing 初始化、配置加载、命令分发、退出码。

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use xd_xk_core::api::Api;
use xd_xk_core::config::{self, Config};
use xd_xk_core::error::AppError;
use xd_xk_core::ocr::{CaptchaOcr, ManualOcr};

use crate::args::{Cli, Command, ConfArgs, ConfCommand};
use crate::{completions, conf_cmd, ops};

/// 旧 conf.json 路径：与目标 config 同目录（默认 cwd 下的 conf.json）。
pub(crate) fn legacy_conf_path(cli: &Cli) -> PathBuf {
    cli.config
        .parent()
        .map(|p| p.join(config::LEGACY_CONF_PATH))
        .unwrap_or_else(|| PathBuf::from(config::LEGACY_CONF_PATH))
}

/// 是否跳过自动迁移（conf migrate / init 显式管理配置；completions 无关）。
fn skip_auto_migrate(cli: &Cli) -> bool {
    matches!(cli.command, Command::Completions { .. })
        || matches!(
            &cli.command,
            Command::Conf(ConfArgs {
                command: ConfCommand::Migrate,
                ..
            }) | Command::Conf(ConfArgs {
                command: ConfCommand::Init(_),
                ..
            })
        )
}

/// 从命令行参数解析并运行（`main` 入口）。
pub(crate) fn run_from<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    use clap::error::ErrorKind;
    match Cli::try_parse_from(args) {
        Ok(cli) => run(cli),
        Err(e) => {
            let _ = e.print();
            // --help / --version / 无子命令打印 help → 0；参数错误 → 1
            match e.kind() {
                ErrorKind::DisplayHelp
                | ErrorKind::DisplayVersion
                | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => 0,
                _ => 1,
            }
        }
    }
}

/// 使用当前环境变量运行。
pub(crate) fn run(cli: Cli) -> i32 {
    run_with(cli, &std::env::vars().collect(), None)
}

/// 使用指定环境变量映射与 OCR 实现运行（测试注入 FakeOcr 用）。
pub(crate) fn run_with(
    cli: Cli,
    env: &HashMap<String, String>,
    ocr: Option<Box<dyn CaptchaOcr>>,
) -> i32 {
    init_tracing(cli.verbose);
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("错误：创建异步运行时失败：{e}");
            return 1;
        }
    };
    rt.block_on(run_async(cli, env, ocr))
}

/// 异步入口（测试在同一 runtime 内调用 wiremock 用）。
pub(crate) async fn run_async(
    cli: Cli,
    env: &HashMap<String, String>,
    ocr: Option<Box<dyn CaptchaOcr>>,
) -> i32 {
    init_tracing(cli.verbose);
    let ocr: Box<dyn CaptchaOcr> = ocr.unwrap_or_else(|| Box::new(ManualOcr::new()));
    dispatch(cli, env, ocr).await
}

/// 加载配置：config.toml + 环境变量覆盖 + `--debug`。
pub(crate) fn load_config(cli: &Cli, env: &HashMap<String, String>) -> Result<Config, AppError> {
    let mut cfg = config::load(&cli.config)?;
    config::apply_env_overrides(&mut cfg, env);
    if cli.debug {
        cfg.app.debug = true;
    }
    Ok(cfg)
}

/// 构建 API 客户端（`--base-url` 覆盖用于测试）。
pub(crate) fn build_api(cli: &Cli) -> Result<Arc<Api>, AppError> {
    match &cli.base_url {
        Some(url) => Ok(Arc::new(Api::new(url.clone())?)),
        None => Ok(Arc::new(Api::live()?)),
    }
}

/// 初始化 tracing：默认 info；`-v` → debug；`-vv` → trace。
/// 日志走 stderr，stdout 留给结构化输出。`try_init` 保证可重复调用（测试友好）。
fn init_tracing(verbose: u8) {
    let level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level)),
        )
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();
}

async fn dispatch(cli: Cli, env: &HashMap<String, String>, ocr: Box<dyn CaptchaOcr>) -> i32 {
    // 首次运行：旧 conf.json → config.toml + courses.csv 自动迁移
    if !skip_auto_migrate(&cli) {
        match config::migrate_if_needed(&cli.config, &cli.courses_path(), &legacy_conf_path(&cli)) {
            Ok(summary) if summary.migrated && !cli.json => {
                println!(
                    "已从旧 conf.json 迁移：生成 {} 与 {}（{} 门课程），原文件保留",
                    summary.config_path.display(),
                    summary.courses_path.display(),
                    summary.course_count
                );
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("错误：{e}");
                return e.exit_code();
            }
        }
    }

    let result = match &cli.command {
        Command::Select(args) => ops::cmd_select(&cli, env, ocr.as_ref(), args).await,
        Command::Drop(args) => ops::cmd_drop(&cli, env, ocr.as_ref(), args).await,
        Command::Check(args) => ops::cmd_check(&cli, env, ocr.as_ref(), args).await,
        Command::List(args) => ops::cmd_list(&cli, env, ocr.as_ref(), args).await,
        Command::Login => ops::cmd_login(&cli, env, ocr.as_ref()).await,
        Command::Conf(args) => conf_cmd::cmd_conf(&cli, env, args).await,
        Command::Completions { shell } => completions::cmd_completions(*shell).await,
    };

    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("错误：{e}");
            e.exit_code()
        }
    }
}
