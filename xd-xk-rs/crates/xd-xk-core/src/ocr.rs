//! 验证码识别抽象。
//!
//! - [`CaptchaOcr`] trait：核心库只依赖这个 trait（设计文档 §5.2）；
//! - [`ManualOcr`]：人工验证码 fallback —— 写临时 PNG → 系统看图 → stdin 输入；
//! - `OrtOcr`（方案 A，复用 Python 的 `common_old.onnx`）在 P2 阶段实现，
//!   通过 cargo feature 接入。

use std::io::{self, BufRead, Write};

/// 验证码 OCR 错误。
#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("{0}")]
    Msg(String),
    #[error("验证码图片处理失败：{0}")]
    Io(#[from] io::Error),
}

/// 验证码识别接口。
///
/// 实现必须 `Send + Sync`（在 async 调用中共享）。
pub trait CaptchaOcr: Send + Sync {
    /// 识别一张验证码图片（原始 PNG 字节），返回验证码字符串。
    fn recognize(&self, img: &[u8]) -> Result<String, OcrError>;

    /// 是否为人工输入实现（`ocr_captcha=false` 时要求实现为人工输入）。
    fn is_manual(&self) -> bool {
        false
    }
}

/// 人工验证码：写临时 PNG → 系统图片查看器打开 → 从 stdin 输入。
///
/// 等价于 Python CLI 的 `_manual_captcha`（`PIL.Image.show()` + `input()`）。
#[derive(Debug, Default)]
pub struct ManualOcr;

impl ManualOcr {
    pub fn new() -> Self {
        Self
    }
}

impl CaptchaOcr for ManualOcr {
    fn is_manual(&self) -> bool {
        true
    }

    fn recognize(&self, img: &[u8]) -> Result<String, OcrError> {
        let path = std::env::temp_dir().join("xd-xk-captcha.png");
        std::fs::write(&path, img)?;
        open_image(&path);
        eprintln!("验证码图片已打开，请输入验证码:");
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        let _ = stdout.flush();
        let stdin = io::stdin();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let _ = std::fs::remove_file(&path);
        Ok(line.trim().to_string())
    }
}

/// 用系统默认图片查看器打开文件。
#[cfg(target_os = "windows")]
fn open_image(path: &std::path::Path) {
    // `cmd /c start "" <path>` —— 空标题避免把文件名当窗口标题
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", path.to_str().unwrap_or_default()])
        .spawn();
}

#[cfg(target_os = "macos")]
fn open_image(path: &std::path::Path) {
    let _ = std::process::Command::new("open").arg(path).spawn();
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn open_image(path: &std::path::Path) {
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}
