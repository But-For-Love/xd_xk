use std::future::Future;
use std::path::Path;

use anyhow::Result;
use clap::Parser;

use xd_xk_rust::app::{Action, compatibility_test, perform_action};
use xd_xk_rust::cli::{Cli, Command};
use xd_xk_rust::config;
use xd_xk_rust::error::XkError;
use xd_xk_rust::ui;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Menu) | None => run_menu(&cli.config, cli.model.as_deref()).await?,
        Some(command) => {
            run_with_cancel(execute_command(&cli.config, cli.model.as_deref(), command)).await?
        }
    }

    Ok(())
}

/// 交互式主菜单，与原版 xk_main.py 保持一致。
async fn run_menu(config_path: &Path, model: Option<&Path>) -> Result<()> {
    loop {
        println!("\n{}", "-".repeat(34));
        println!("西电选课工具");
        println!("1. 正常选课");
        println!("2. 只读兼容性检测（推荐先运行）");
        println!("3. 编辑配置");
        println!("4. 退课");
        println!("0. 退出");

        let choice = tokio::select! {
            line = ui::read_line("请选择：") => line?,
            _ = tokio::signal::ctrl_c() => {
                println!("\n操作已取消，返回主菜单。");
                continue;
            }
        };

        match choice.as_str() {
            "1" => {
                run_menu_command(execute_command(
                    config_path,
                    model,
                    Command::Select {
                        category: 0,
                        courses: None,
                        once: false,
                    },
                ))
                .await;
            }
            "2" => run_menu_command(execute_command(config_path, model, Command::Compat)).await,
            "3" => {
                run_menu_command(execute_command(
                    config_path,
                    model,
                    Command::Config { show: false },
                ))
                .await;
            }
            "4" => {
                run_menu_command(execute_command(
                    config_path,
                    model,
                    Command::Drop {
                        category: 0,
                        courses: None,
                        once: false,
                    },
                ))
                .await;
            }
            "0" => {
                println!("已退出。");
                return Ok(());
            }
            _ => println!("请输入 0～4。"),
        }
    }
}

/// 执行一个子命令，出错时返回 XkError 给菜单层展示。
async fn execute_command(
    config_path: &Path,
    model: Option<&Path>,
    command: Command,
) -> Result<(), XkError> {
    match command {
        Command::Menu => Ok(()),
        Command::Select {
            category,
            courses,
            once,
        } => {
            let config = config::load_config(config_path, model)?;
            perform_action(config, Action::Select, category, courses, !once).await
        }
        Command::Drop {
            category,
            courses,
            once,
        } => {
            let config = config::load_config(config_path, model)?;
            perform_action(config, Action::Drop, category, courses, !once).await
        }
        Command::Compat => {
            let config = config::load_config(config_path, model)?;
            compatibility_test(config).await
        }
        Command::Config { show } => {
            let config = config::load_config(config_path, model)?;
            if show {
                config::print_config(&config);
                Ok(())
            } else {
                config::edit_config(config_path, model).await
            }
        }
    }
}

/// 菜单里运行一个命令：Ctrl+C 取消并返回菜单，错误显示原因后返回菜单。
async fn run_menu_command<F>(future: F)
where
    F: Future<Output = Result<(), XkError>>,
{
    if let Err(error) = run_with_cancel(future).await {
        println!("\n操作未完成：{error}");
    }
}

/// 让任务与 Ctrl+C 竞争；收到信号后取消当前任务。
async fn run_with_cancel<F>(future: F) -> Result<()>
where
    F: Future<Output = Result<(), XkError>>,
{
    let outcome = tokio::select! {
        result = future => result,
        _ = tokio::signal::ctrl_c() => {
            println!("\n操作已取消。");
            return Ok(());
        }
    };
    outcome.map_err(Into::into)
}
