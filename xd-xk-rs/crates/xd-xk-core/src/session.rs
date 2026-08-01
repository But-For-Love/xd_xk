//! 已认证的选课会话（外观模式，等价 Python `CourseSession`）。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::api::Api;
use crate::batch::show_msg;
use crate::config::Config;
use crate::course::{Category, CourseRow};
use crate::error::AppError;
use crate::ocr::CaptchaOcr;
use crate::ops::{PollConfig, SelectOutcome};

/// 封装一个已认证的选课会话。
///
/// 将散落的 token / cookie / batch_code / 原始登录响应打包为单一对象，
/// 避免到处传递裸参数。
#[derive(Clone)]
pub struct CourseSession {
    pub token: String,
    pub cookies: HashMap<String, String>,
    pub batch_code: String,
    /// 原始登录响应，保留用于兼容。
    pub data: Value,
    api: Arc<Api>,
}

impl CourseSession {
    /// 指向真实服务器的便捷工厂：登录 → 展示信息 → 匹配批次。
    pub async fn create(
        cfg: &Config,
        ocr: &dyn CaptchaOcr,
        cancel: &CancellationToken,
    ) -> Result<CourseSession, AppError> {
        let api = Api::live()?;
        Self::create_with_api(Arc::new(api), cfg, ocr, cancel).await
    }

    /// 注入自定义 API 客户端的工厂（测试用 wiremock 指向假服务器）。
    pub async fn create_with_api(
        api: Arc<Api>,
        cfg: &Config,
        ocr: &dyn CaptchaOcr,
        cancel: &CancellationToken,
    ) -> Result<CourseSession, AppError> {
        let (data, cookies) = api.login(cfg, ocr, cancel).await?;
        let token = data
            .pointer("/data/token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Login("登录响应缺少 token".into()))?
            .to_string();
        let batch_code = show_msg(&data, &cfg.app.batch_name)?;
        Ok(Self {
            token,
            cookies,
            batch_code,
            data,
            api,
        })
    }

    /// 获取课程列表。
    pub async fn get_class(
        &self,
        cfg: &Config,
        category: Category,
    ) -> Result<Vec<CourseRow>, AppError> {
        self.api
            .get_class(&self.token, &self.batch_code, cfg, category)
            .await
    }

    /// 批量获取多类别课程列表（Python `fetch_courses`，参数为类别集合）。
    pub async fn fetch_courses(
        &self,
        cfg: &Config,
        categories: &HashSet<Category>,
    ) -> Result<HashMap<Category, Vec<CourseRow>>, AppError> {
        self.api
            .fetch_courses(&self.token, &self.batch_code, cfg, categories)
            .await
    }

    /// 选课（持续轮询或单次，取决于 `PollConfig`）。
    pub async fn add(
        &self,
        course: &CourseRow,
        category: Category,
        cfg: &PollConfig,
    ) -> Result<SelectOutcome, AppError> {
        self.api
            .add(
                &self.token,
                &self.batch_code,
                &self.cookies,
                course,
                category,
                cfg,
            )
            .await
    }

    /// 退课（持续轮询或单次，取决于 `PollConfig`）。
    pub async fn dele(
        &self,
        course: &CourseRow,
        category: Category,
        cfg: &PollConfig,
    ) -> Result<SelectOutcome, AppError> {
        self.api
            .dele(
                &self.token,
                &self.batch_code,
                &self.cookies,
                course,
                category,
                cfg,
            )
            .await
    }

    /// 切换选课批次（保留 API）。
    pub async fn choose_batch(&self, batch_id: &str) -> Result<Value, AppError> {
        self.api.choose_batch(&self.token, batch_id).await
    }
}
