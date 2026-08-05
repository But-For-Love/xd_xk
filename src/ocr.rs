use std::path::PathBuf;

use ddddocr::DdddOcr;
use tokio::sync::Mutex;

use crate::error::XkError;

/// 验证码 OCR 引擎：懒加载 ddddocr-rs，ONNX 推理由 ort 在库内部完成。
pub struct OcrEngine {
    model: PathBuf,
    inner: Mutex<Option<DdddOcr>>,
}

impl OcrEngine {
    pub fn new(model: PathBuf) -> Self {
        Self {
            model,
            inner: Mutex::new(None),
        }
    }

    pub async fn recognize(&self, image: &[u8]) -> Result<String, XkError> {
        let mut guard = self.inner.lock().await;
        if guard.is_none() {
            if !self.model.exists() {
                return Err(XkError::ModelNotFound(self.model.display().to_string()));
            }
            let ocr = DdddOcr::new(&self.model).map_err(|e| {
                XkError::Ocr(format!("加载模型 {} 失败：{e}", self.model.display()))
            })?;
            *guard = Some(ocr);
        }

        guard
            .as_mut()
            .expect("OCR 引擎已初始化")
            .classification(image)
            .await
            .map_err(|e| XkError::Ocr(format!("验证码识别失败：{e}")))
    }
}
