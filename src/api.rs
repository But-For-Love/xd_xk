use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use reqwest::cookie::Jar;
use reqwest::{Client, Url};
use serde_json::{Value, json};

use crate::config::Config;
use crate::error::XkError;
use crate::model::CourseRow;
use crate::ocr::OcrEngine;
use crate::ui;

const API_BASE: &str = "https://xk.xidian.edu.cn/xsxk/";
const USER_AGENT: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ",
    "AppleWebKit/537.36 Chrome/103 Safari/537.36"
);

/// 登录成功后的会话信息。
#[derive(Debug, Clone)]
pub struct LoginResult {
    pub payload: Value,
    pub token: String,
}

/// 所有接口共用的异步客户端。
pub struct ApiClient {
    config: Config,
    client: Client,
    jar: Arc<Jar>,
    base_url: Url,
    ocr: OcrEngine,
}

impl ApiClient {
    pub fn new(config: Config) -> Result<Self, XkError> {
        let base_url =
            Url::parse(API_BASE).map_err(|e| XkError::Config(format!("API 地址解析失败：{e}")))?;
        let jar = Arc::new(Jar::default());
        let timeout = Duration::from_secs_f64(config.request_timeout.clamp(1.0, 600.0));
        let client = Client::builder()
            .cookie_provider(jar.clone())
            .user_agent(USER_AGENT)
            .timeout(timeout)
            .build()
            .map_err(|e| XkError::Config(format!("初始化 HTTP 客户端失败：{e}")))?;

        let ocr_model = config.ocr_model.clone();
        Ok(Self {
            config,
            client,
            jar,
            base_url,
            ocr: OcrEngine::new(ocr_model),
        })
    }

    /// 获取验证码并识别/输入，返回 (验证码, uuid)。
    pub async fn get_captcha(&self) -> Result<(String, String), XkError> {
        let payload = self
            .post_json(
                "/auth/captcha",
                None,
                None,
                None,
                None,
                Some("captcha_pac.json"),
            )
            .await?;

        if let Some(msg) = payload.get("msg").and_then(Value::as_str) {
            println!("{msg}");
        }

        let data = payload
            .get("data")
            .and_then(Value::as_object)
            .ok_or_else(|| XkError::Api {
                path: "/auth/captcha".into(),
                msg: "缺少 data 字段，接口可能已经改版".into(),
            })?;
        let image_data =
            data.get("captcha")
                .and_then(Value::as_str)
                .ok_or_else(|| XkError::Api {
                    path: "/auth/captcha".into(),
                    msg: "缺少 captcha 字段，接口可能已经改版".into(),
                })?;
        let uuid = data
            .get("uuid")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| XkError::Api {
                path: "/auth/captcha".into(),
                msg: "缺少 uuid 字段，接口可能已经改版".into(),
            })?;

        let encoded = image_data
            .rsplit_once(',')
            .map(|(_, b)| b)
            .unwrap_or(image_data);
        let image_bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(XkError::Base64)?;

        let code = if self.config.ocr_captcha {
            let code = self.ocr.recognize(&image_bytes).await?;
            println!("验证码识别结果：{code}");
            code
        } else {
            tokio::fs::write("captcha.png", &image_bytes).await?;
            println!("验证码图片已保存到 captcha.png，请打开后输入验证码：");
            ui::read_line("请输入验证码：").await?
        };

        if code.is_empty() {
            return Err(XkError::Config("验证码为空".into()));
        }
        Ok((code, uuid))
    }

    /// 登录，返回登录 JSON 与 token；Cookie 由客户端自动维护。
    pub async fn login(&self, loginname: String, password: String) -> Result<LoginResult, XkError> {
        if loginname.is_empty() || password.is_empty() {
            return Err(XkError::Config("学号或密码为空".into()));
        }

        let encrypted_password = crate::crypto::aes_encrypt(&password)?;
        let (captcha, uuid) = self.get_captcha().await?;

        let params = [
            ("loginname", loginname.as_str()),
            ("password", encrypted_password.as_str()),
            ("captcha", captcha.as_str()),
            ("uuid", uuid.as_str()),
        ];
        let payload = self
            .post_json(
                "/auth/login",
                Some(&params),
                None,
                None,
                None,
                Some("login_pac.json"),
            )
            .await?;

        let token = payload
            .get("data")
            .and_then(Value::as_object)
            .and_then(|data| data.get("token"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                let msg = payload
                    .get("msg")
                    .and_then(Value::as_str)
                    .unwrap_or("响应中没有 token");
                XkError::Login(msg.to_string())
            })?;

        Ok(LoginResult { payload, token })
    }

    /// 获取必修/选修课程列表原始 JSON。
    pub async fn get_class(
        &self,
        login: &LoginResult,
        batch: &str,
        category: u8,
    ) -> Result<Value, XkError> {
        let clazz_type = match category {
            0 => "FANKC",
            1 => "XGKC",
            _ => {
                return Err(XkError::Config(format!(
                    "未知课程类别：{category}，只支持 0（必修）或 1（选修）"
                )));
            }
        };
        let body = json!({
            "teachingClassType": clazz_type,
            "pageNumber": 1,
            "pageSize": 300,
            "orderBy": "",
            "campus": self.config.campus,
        });
        let debug_file = if category == 0 {
            Some("classlist_required.json")
        } else {
            Some("classlist_elective.json")
        };
        self.post_json(
            "/elective/clazz/list",
            None,
            Some(body),
            Some(&login.token),
            Some(batch),
            debug_file,
        )
        .await
    }

    /// 选课或退课，带重试与间隔。
    pub async fn run_action(
        &self,
        login: &LoginResult,
        class: &CourseRow,
        batch: &str,
        category: u8,
        deleting: bool,
        always: bool,
    ) -> Result<bool, XkError> {
        let path = if deleting {
            "/elective/clazz/del"
        } else {
            "/elective/clazz/add"
        };
        let action_name = if deleting { "退课" } else { "选课" };

        let jxbid = class
            .jxbid
            .as_deref()
            .ok_or_else(|| XkError::Config("教学班缺少关键字段：JXBID".into()))?;
        let secret = class
            .secret_val
            .as_deref()
            .ok_or_else(|| XkError::Config("教学班缺少关键字段：secretVal".into()))?;

        let clazz_type = if deleting && category == 0 {
            "TJKC"
        } else if category == 0 {
            "FANKC"
        } else {
            "XGKC"
        };
        let mut params = vec![
            ("clazzType", clazz_type),
            ("clazzId", jxbid),
            ("secretVal", secret),
        ];
        if !deleting || category == 1 {
            params.push(("chooseVolunteer", "1"));
        }

        let success_words: &[&str] = if deleting {
            &["操作成功", "退课成功", "未查询到选课结果"]
        } else {
            &[
                "操作成功",
                "选课成功",
                "该课程已在选课结果中",
                "所选课程与已选课程冲突",
            ]
        };

        let mut attempt: u32 = 0;
        let mut consecutive_errors: u32 = 0;
        loop {
            attempt += 1;
            match self
                .post_json(
                    path,
                    Some(&params),
                    None,
                    Some(&login.token),
                    Some(batch),
                    None,
                )
                .await
            {
                Ok(payload) => {
                    consecutive_errors = 0;
                    let message = payload
                        .get("msg")
                        .and_then(Value::as_str)
                        .unwrap_or("<响应中没有 msg>")
                        .to_string();
                    println!(
                        "{} {} {} {}",
                        class.kch,
                        class.kcm.as_deref().unwrap_or("<无课程名>"),
                        action_name,
                        message
                    );
                    if success_words.iter().any(|word| message.contains(word)) {
                        return Ok(true);
                    }
                }
                Err(error) => {
                    consecutive_errors += 1;
                    println!("第 {attempt} 次{action_name}请求失败：{error}");
                    if consecutive_errors >= 5 {
                        println!("连续失败 5 次，已停止当前课程操作，避免无休止报错。");
                        return Ok(false);
                    }
                }
            }

            if !always || (self.config.max_attempts > 0 && attempt >= self.config.max_attempts) {
                return Ok(false);
            }
            if self.config.request_interval > 0.0 {
                tokio::time::sleep(Duration::from_secs_f64(self.config.request_interval)).await;
            }
        }
    }

    /// 通用 POST JSON 请求。
    #[allow(clippy::too_many_arguments)]
    async fn post_json(
        &self,
        path: &str,
        params: Option<&[(&str, &str)]>,
        json_body: Option<Value>,
        auth: Option<&str>,
        batch: Option<&str>,
        debug_file: Option<&str>,
    ) -> Result<Value, XkError> {
        let url = self
            .base_url
            .join(path.trim_start_matches('/'))
            .map_err(|e| XkError::Config(format!("接口地址解析失败（{path}）：{e}")))?;

        let mut request = self.client.post(url.clone());
        if let Some(params) = params {
            request = request.query(params);
        }
        if let Some(body) = json_body {
            request = request.json(&body);
        }
        if let Some(token) = auth {
            request = request.header("Authorization", token);
            self.jar
                .add_cookie_str(&format!("Authorization={token}"), &url);
        }
        if let Some(batch) = batch {
            request = request.header("batchId", batch);
        }

        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                XkError::Timeout {
                    path: path.to_string(),
                }
            } else {
                XkError::Request {
                    path: path.to_string(),
                    source: error,
                }
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(XkError::HttpStatus {
                path: path.to_string(),
                status: status.as_u16(),
                body,
            });
        }

        let bytes = response.bytes().await.map_err(|error| XkError::Request {
            path: path.to_string(),
            source: error,
        })?;
        if self.config.debug
            && let Some(file) = debug_file
        {
            tokio::fs::write(file, &bytes).await?;
        }

        let limit = bytes.len().min(160);
        let preview = String::from_utf8_lossy(&bytes[..limit]).replace('\n', " ");
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| XkError::NotJson {
            path: path.to_string(),
            preview: preview.to_string(),
        })?;
        if !value.is_object() {
            return Err(XkError::Api {
                path: path.to_string(),
                msg: "返回的数据结构不是对象".into(),
            });
        }
        Ok(value)
    }
}

/// 从配置或交互输入获取学号/密码。
pub async fn prompt_credentials(config: &Config) -> Result<(String, String), XkError> {
    let loginname = if config.loginname.is_empty() {
        ui::read_line("学号：").await?
    } else {
        config.loginname.clone()
    };

    let password = if config.password.is_empty() {
        ui::read_line("密码：").await?
    } else {
        config.password.clone()
    };

    Ok((loginname, password))
}
