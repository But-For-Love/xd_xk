//! shell 补全脚本生成（clap_complete 一行生成）。

use clap::CommandFactory;

use xd_xk_core::error::AppError;

use crate::args::Cli;

pub(crate) async fn cmd_completions(shell: clap_complete::Shell) -> Result<i32, AppError> {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "xd-xk", &mut std::io::stdout());
    Ok(0)
}
