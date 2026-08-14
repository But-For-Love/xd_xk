use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xd-xk-rust", version, about = "西电选课工具（Rust 重构版）")]
pub struct Cli {
    /// TOML 配置文件路径，默认读取当前目录 config.toml
    #[arg(long, global = true, default_value = "config.toml", value_name = "PATH")]
    pub config: PathBuf,

    /// ddddocr ONNX 模型路径；默认取配置中的 ocr_model，再默认 ddddocr.onnx
    #[arg(long, global = true, value_name = "PATH")]
    pub model: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// 交互式主菜单（不带参数时默认进入）
    Menu,

    /// 正常选课
    Select {
        /// 课程类别：0 必修 / 1 选修
        #[arg(long, default_value_t = 0, value_name = "0|1")]
        category: u8,

        /// 直接指定课程，例如 TE204004:06,TE204004:07（选修只写课程号）
        #[arg(long, value_name = "COURSES")]
        courses: Option<String>,

        /// 每门课只尝试一次，不循环重试
        #[arg(long)]
        once: bool,
    },

    /// 退课
    Drop {
        /// 课程类别：0 必修 / 1 选修
        #[arg(long, default_value_t = 0, value_name = "0|1")]
        category: u8,

        /// 直接指定课程，例如 TE204004:06,TE204004:07（选修只写课程号）
        #[arg(long, value_name = "COURSES")]
        courses: Option<String>,

        /// 每门课只尝试一次，不循环重试
        #[arg(long)]
        once: bool,
    },

    /// 只读兼容性检测（推荐先运行）
    Compat,

    /// 编辑配置；加 --show 只显示当前配置
    Config {
        /// 只显示当前配置，不进入编辑
        #[arg(long)]
        show: bool,
    },
}
