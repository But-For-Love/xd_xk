use thiserror::Error;

/// 选课程序可预期的错误类型。
#[derive(Debug, Error)]
pub enum XkError {
    #[error("配置错误：{0}")]
    Config(String),

    #[error("网络请求失败（{path}）：{source}")]
    Request {
        path: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("请求超时（{path}），请检查网络或稍后重试")]
    Timeout { path: String },

    #[error("HTTP {status} 请求失败（{path}）：{body}")]
    HttpStatus {
        path: String,
        status: u16,
        body: String,
    },

    #[error("接口 {path} 未返回 JSON，可能已经改版。响应片段：{preview}")]
    NotJson { path: String, preview: String },

    #[error("接口 {path} 返回的数据结构异常：{msg}")]
    Api { path: String, msg: String },

    #[error("登录未成功：{0}")]
    Login(String),

    #[error("验证码识别失败：{0}")]
    Ocr(String),

    #[error(
        "验证码模型未找到：{0}；请先运行 scripts/setup-model.ps1，或在配置中设置 ocr_model / 使用 --model"
    )]
    ModelNotFound(String),

    #[error("文件读写失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 解析失败：{0}")]
    Json(#[from] serde_json::Error),

    #[error("Base64 解码失败：{0}")]
    Base64(#[from] base64::DecodeError),

    #[error("密码加密失败：{0}")]
    Crypto(String),
}
