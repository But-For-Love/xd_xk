//! 核心业务错误类型与 CLI 退出码约定。

use thiserror::Error;

/// 核心业务错误。所有错误文案保持中文，与 Python 版一致。
#[derive(Debug, Error)]
pub enum AppError {
    /// 配置 / 参数 / 文件 / 解析错误。
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Param(String),
    /// 网络不可达（未连校园网 / VPN）。
    #[error("{0}")]
    Network(String),
    /// 登录失败（含验证码错误、验证码 OCR 失败）。
    #[error("{0}")]
    Login(String),
    /// 批次未开放 / 未匹配。
    #[error("{0}")]
    Batch(String),
    /// 一般接口错误。
    #[error("{0}")]
    Api(String),
    /// 验证码识别失败。
    #[error("{0}")]
    Ocr(String),
    /// I/O 错误。
    #[error("{0}")]
    Io(String),
    /// 响应 / 数据解析错误。
    #[error("{0}")]
    Parse(String),
    /// 用户主动停止。
    #[error("用户停止操作")]
    Canceled,
}

impl AppError {
    /// CLI 退出码约定（设计文档 §6.3(3)）。
    ///
    /// | 码 | 含义 |
    /// |---|---|
    /// | 0 | 全部课程达成 / 正常结束 |
    /// | 1 | 参数 / 配置错误 |
    /// | 2 | 登录失败（含验证码错误） |
    /// | 3 | 网络不可达（未连校园网 / VPN） |
    /// | 4 | 批次未开放 / 未匹配 |
    /// | 130 | Ctrl+C 优雅退出 |
    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::Config(_)
            | AppError::Param(_)
            | AppError::Io(_)
            | AppError::Parse(_)
            | AppError::Api(_) => 1,
            AppError::Login(_) | AppError::Ocr(_) => 2,
            AppError::Network(_) => 3,
            AppError::Batch(_) => 4,
            AppError::Canceled => 130,
        }
    }
}

/// 截断字符串到指定字节数（Python `text[:n]` 的等价物）。
pub(crate) fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
    }
}

/// serde / 文件解析错误 → AppError::Parse。
pub(crate) fn parse_err(context: &str, e: impl std::fmt::Display) -> AppError {
    AppError::Parse(format!("{context}：{e}"))
}
