//! xd-xk 命令行入口（薄壳：解析 → 运行 → 退出码）。

use std::process::ExitCode;

fn main() -> ExitCode {
    let code = xd_xk_cli::run_from(std::env::args_os());
    ExitCode::from(code.clamp(0, 255) as u8)
}
