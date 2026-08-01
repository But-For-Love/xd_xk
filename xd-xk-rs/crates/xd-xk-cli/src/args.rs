//! clap 命令行定义（设计文档 §6）。

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

use xd_xk_core::course::Category as CoreCategory;

/// 全局命令行入口。
#[derive(Debug, Parser)]
#[command(
    name = "xd-xk",
    version,
    about = "西安电子科技大学（XDU）自动选课工具 —— 登录教务系统，自动完成选课、退课与容量监控",
    subcommand_required = true
)]
pub struct Cli {
    /// 配置文件路径（默认 config.toml）
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        default_value = "config.toml"
    )]
    pub config: PathBuf,

    /// 课程文件路径（默认 courses.csv；按扩展名/魔数识别 CSV 或 XLSX）
    #[arg(long, global = true, value_name = "PATH")]
    pub courses: Option<PathBuf>,

    /// 详细日志：-v 调试，-vv 跟踪
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,

    /// 等同 conf.debug=true，dump 接口响应
    #[arg(long, global = true)]
    pub debug: bool,

    /// 只走完整流程，不真正选课/退课
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// 面向脚本的结构化输出（stdout）
    #[arg(long, global = true)]
    pub json: bool,

    /// 服务器基址覆盖（测试 / 代理用，隐藏）
    #[arg(long, global = true, hide = true, value_name = "URL")]
    pub base_url: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// 默认课程文件路径。
    pub fn courses_path(&self) -> PathBuf {
        self.courses
            .clone()
            .unwrap_or_else(|| PathBuf::from("courses.csv"))
    }
}

/// 子命令。
#[derive(Debug, Subcommand)]
pub enum Command {
    /// 自动选课：登录后按课程列表提交选课
    Select(SelectArgs),
    /// 自动退课：登录后按课程列表提交退课
    Drop(DropArgs),
    /// 容量检查：监控余量，有空位自动抢课
    Check(CheckArgs),
    /// 列出某批次课程（搜索过滤）
    List(ListArgs),
    /// 验证凭据与批次，打印学生信息
    Login,
    /// 配置与课程文件管理
    Conf(ConfArgs),
    /// 生成 shell 补全脚本
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

/// 课程类别（ValueEnum，兼容旧的 0/1/bx/xx 写法）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CategoryArg {
    /// 选修（XGKC，平铺行）
    #[value(name = "选修", alias = "1", alias = "xx")]
    Elective,
    /// 必修（FANKC/TJKC，嵌套 tcList）
    #[value(name = "必修", alias = "0", alias = "bx")]
    Required,
}

impl CategoryArg {
    pub fn to_core(self) -> CoreCategory {
        match self {
            CategoryArg::Elective => CoreCategory::Elective,
            CategoryArg::Required => CoreCategory::Required,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CategoryArg::Elective => "选修",
            CategoryArg::Required => "必修",
        }
    }
}

/// 课程标识：`KCH` 或 `KCH/KXH`（必修用后者）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseId {
    pub kch: String,
    pub kxh: Option<String>,
}

impl fmt::Display for CourseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kxh {
            Some(kxh) if !kxh.is_empty() => write!(f, "{}/{}", self.kch, kxh),
            _ => write!(f, "{}", self.kch),
        }
    }
}

impl FromStr for CourseId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err("课程号不能为空".into());
        }
        match s.split_once('/') {
            Some((kch, kxh)) => Ok(CourseId {
                kch: kch.trim().to_string(),
                kxh: Some(kxh.trim().to_string()),
            }),
            None => Ok(CourseId {
                kch: s.to_string(),
                kxh: None,
            }),
        }
    }
}

/// `select` 参数。
#[derive(Debug, Args)]
pub struct SelectArgs {
    /// 课程，格式 KCH 或 KCH/KXH；不指定则读课程文件
    #[arg(value_name = "COURSE")]
    pub courses: Vec<CourseId>,
    /// 课程类别（默认选修）
    #[arg(short, long, value_enum, default_value = "选修")]
    pub category: CategoryArg,
    /// 交互式选课：登录后从课程列表多选
    #[arg(short, long)]
    pub interactive: bool,
    /// 只发一次请求，不持续重试
    #[arg(long)]
    pub once: bool,
    /// 轮询间隔（秒）
    #[arg(long, value_name = "SEC", default_value_t = 1.0)]
    pub interval: f64,
}

/// `drop` 参数（同 select 家族，无交互式）。
#[derive(Debug, Args)]
pub struct DropArgs {
    /// 课程，格式 KCH 或 KCH/KXH；不指定则读课程文件
    #[arg(value_name = "COURSE")]
    pub courses: Vec<CourseId>,
    /// 课程类别（默认选修）
    #[arg(short, long, value_enum, default_value = "选修")]
    pub category: CategoryArg,
    /// 只发一次请求，不持续重试
    #[arg(long)]
    pub once: bool,
    /// 轮询间隔（秒）
    #[arg(long, value_name = "SEC", default_value_t = 1.0)]
    pub interval: f64,
}

/// `check` 参数。
#[derive(Debug, Args)]
pub struct CheckArgs {
    /// 要监控的课程号（KCH）列表；不指定则读课程文件
    #[arg(value_name = "KCH")]
    pub kch: Vec<String>,
    /// 轮询间隔（秒）
    #[arg(long, value_name = "SEC", default_value_t = 0.5)]
    pub interval: f64,
    /// 只监控不自动选课
    #[arg(long)]
    pub no_grab: bool,
    /// 运行 N 秒后自动退出
    #[arg(long, value_name = "SEC")]
    pub until: Option<u64>,
    /// 运行 N 轮后自动退出
    #[arg(long, value_name = "N")]
    pub rounds: Option<u32>,
}

/// `list` 参数。
#[derive(Debug, Args)]
pub struct ListArgs {
    /// 课程类别；不指定则列出全部
    #[arg(short, long, value_enum)]
    pub category: Option<CategoryArg>,
    /// 搜索关键字（匹配 KCH / KCM / 教师）
    #[arg(value_name = "关键字")]
    pub keyword: Option<String>,
}

/// `conf` 参数。
#[derive(Debug, Args)]
pub struct ConfArgs {
    #[command(subcommand)]
    pub command: ConfCommand,
}

/// `conf` 子命令。
#[derive(Debug, Subcommand)]
pub enum ConfCommand {
    /// 生成/更新 config.toml（可指定学号密码，或交互式输入）
    Init(ConfInitArgs),
    /// 从旧 conf.json 迁移出 config.toml + courses.csv
    Migrate,
    /// 打印当前配置（密码打码）
    Show,
    /// 生成课程文件模板（CSV；--xlsx 生成 XLSX）
    Template {
        /// 生成 XLSX 模板
        #[arg(long)]
        xlsx: bool,
    },
    /// 从 CSV/XLSX 导入课程到课程文件
    Import {
        /// 要导入的课程文件
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
    /// 导出课程到 CSV/XLSX
    Export {
        /// 目标文件路径（按扩展名决定格式）
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
}

/// `conf init` 参数。
#[derive(Debug, Args)]
pub struct ConfInitArgs {
    /// 学号（也可用环境变量 XD_XK_USERNAME）
    #[arg(short, long, alias = "user", value_name = "LOGINNAME")]
    pub loginname: Option<String>,
    /// 密码（也可用环境变量 XD_XK_PASSWORD）
    #[arg(short, long, value_name = "PASSWORD")]
    pub password: Option<String>,
}
