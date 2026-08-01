//! 西电自动选课工具核心业务库。
//!
//! 纯业务，无 UI、无 CLI 依赖。移植自 Python `xd_xk/core.py`，
//! 行为 1:1 复刻（接口契约见设计文档 §3）。
//!
//! 为未来 GUI 预留的扩展点（设计文档 §7.2）：
//! - 日志走 `tracing` 事件流，GUI 期用 subscriber 接事件，无需改 core；
//! - 全部操作为 `async fn` + `CancellationToken`，GUI 在后台 runtime 执行；
//! - OCR 抽象成 [`CaptchaOcr`] trait，GUI 可插入"弹图 + 人工输入"实现；
//! - 操作返回强类型结果 [`SelectOutcome`]，GUI 可直接据此上色。

pub mod api;
pub mod batch;
pub mod config;
pub mod course;
pub mod courses;
pub mod encrypt;
pub mod error;
pub mod ocr;
pub mod ops;
pub mod session;

pub use api::{Api, BASE_URL};
pub use batch::{batch_list, match_batch, show_msg, student_info, Batch, StudentInfo};
pub use config::{
    migrate_if_needed, AccountConfig, AppConfig, Config, MigrateSummary, LEGACY_CONF_PATH,
};
pub use course::{resolve_target, Category, ClassListResp, CourseRow, Num};
pub use courses::{read_entries, write_csv, CourseEntry};
pub use encrypt::{aes_encrypt, AES_KEY};
pub use error::AppError;
pub use ocr::{CaptchaOcr, ManualOcr, OcrError};
pub use ops::{PollConfig, SelectOutcome, StopReason, ADD_STOP_MSGS, DELE_STOP_MSGS};
pub use session::CourseSession;
