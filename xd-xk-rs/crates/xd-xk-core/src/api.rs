//! HTTP API 客户端：5 个端点 + 请求构造（设计文档 §3 契约逐条固化）。
//!
//! 契约（违反会导致线上选课失败，必须 1:1 复刻 Python）：
//! 1. 登录 / add / del 走 URL query（`.query()`）；
//! 2. `get_class` 走 JSON body（`.json()`），带 `Content-Type: application/json;charset=UTF-8`；
//! 3. add/del 前把 token 同时塞进 Cookie（键名 `Authorization`）与请求头；
//! 4. 必修选课 `FANKC` / 退课 `TJKC`；选修都是 `XGKC`；只有选修退课带 `chooseVolunteer=1`；
//! 5. User-Agent 固定为 Edge 字符串，服务器可能校验。

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::header::{self, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Response, StatusCode};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::course::{Category, CourseRow};
use crate::encrypt::{aes_encrypt, AES_KEY};
use crate::error::{truncate, AppError};
use crate::ocr::CaptchaOcr;
use crate::ops::{
    classify_add, classify_dele, network_err, poll_operation, send_once, OpRequest, PollConfig,
    SelectOutcome, StopCondition, ADD_STOP_MSGS, DELE_STOP_MSGS,
};

/// 选课服务器基址（需校园网 / VPN）。
pub const BASE_URL: &str = "https://xk.xidian.edu.cn/xsxk";

/// 固定 User-Agent（与 Python `_USER_AGENT` 一致）。
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
AppleWebKit/537.36 (KHTML, like Gecko) \
Chrome/103.0.5060.66 Safari/537.36 Edg/103.0.1264.44";

/// 验证码接口超时（Python 10s）。
const CAPTCHA_TIMEOUT: Duration = Duration::from_secs(10);
/// 登录接口超时（Python 15s）。
const LOGIN_TIMEOUT: Duration = Duration::from_secs(15);

/// API 客户端。持有 `reqwest::Client` 与基址（测试时指向 wiremock）。
#[derive(Clone)]
pub struct Api {
    client: Client,
    base_url: String,
}

impl Api {
    /// 创建客户端，指向指定基址。
    pub fn new(base_url: impl Into<String>) -> Result<Self, AppError> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| AppError::Network(format!("创建 HTTP 客户端失败：{e}")))?;
        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    /// 指向真实选课服务器。
    pub fn live() -> Result<Self, AppError> {
        Self::new(BASE_URL)
    }

    // ── 验证码 ──────────────────────────────────────────────────

    /// 获取验证码，返回 `(code, uuid)`。
    pub async fn get_captcha(
        &self,
        cfg: &Config,
        ocr: &dyn CaptchaOcr,
        cancel: &CancellationToken,
    ) -> Result<(String, String), AppError> {
        if cancel.is_cancelled() {
            return Err(AppError::Canceled);
        }
        let url = format!("{}/auth/captcha", self.base_url);
        let resp = self
            .client
            .post(&url)
            .timeout(CAPTCHA_TIMEOUT)
            .send()
            .await
            .map_err(captcha_net_err)?;

        let status = resp.status();
        if status != StatusCode::OK {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Login(format!(
                "验证码接口返回 HTTP {status}\n响应：{}",
                truncate(&text, 200)
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::Parse(format!("读取验证码响应失败：{e}")))?;
        let text = String::from_utf8_lossy(&bytes);
        let p: Value = serde_json::from_str(&text).map_err(|_| {
            AppError::Parse(format!(
                "验证码接口返回非 JSON 数据：{}",
                truncate(&text, 200)
            ))
        })?;
        if cfg.app.debug {
            let _ = std::fs::write("captcha_pac.json", &bytes);
        }

        let msg = p.get("msg").and_then(|v| v.as_str()).unwrap_or("无 msg");
        tracing::info!("验证码接口：{msg}");

        let captcha_b64 = p
            .pointer("/data/captcha")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::Login(format!(
                    "验证码接口返回异常，缺少 data.captcha 字段。完整响应：{}",
                    truncate(&serde_json::to_string(&p).unwrap_or_default(), 300)
                ))
            })?;
        let b64 = captcha_b64
            .strip_prefix("data:image/png;base64,")
            .unwrap_or(captcha_b64);
        let img = BASE64
            .decode(b64)
            .map_err(|e| AppError::Login(format!("验证码图片 Base64 解码失败：{e}")))?;
        let uuid = p
            .pointer("/data/uuid")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Login("验证码接口返回异常，缺少 data.uuid 字段".into()))?
            .to_string();

        let code = if cfg.app.ocr_captcha {
            ocr.recognize(&img)
                .map_err(|e| AppError::Ocr(format!("验证码 OCR 识别失败：{e}")))?
        } else if ocr.is_manual() {
            ocr.recognize(&img)
                .map_err(|e| AppError::Ocr(format!("验证码输入失败：{e}")))?
        } else {
            return Err(AppError::Config(
                "验证码需要人工输入（ocr_captcha=false），但未提供 ManualOcr 人工输入实现。\
                 请启用自动验证码或提供人工输入回调。"
                    .into(),
            ));
        };
        tracing::info!("验证码识别结果：{code}");
        Ok((code, uuid))
    }

    // ── 登录 ────────────────────────────────────────────────────

    /// 登录，返回 `(json_data, cookie_map)`。登录参数走 URL query。
    pub async fn login(
        &self,
        cfg: &Config,
        ocr: &dyn CaptchaOcr,
        cancel: &CancellationToken,
    ) -> Result<(Value, HashMap<String, String>), AppError> {
        let loginname = cfg.account.loginname.clone();
        let password = cfg.account.password.clone();
        if loginname.is_empty() || password.is_empty() {
            return Err(AppError::Config(
                "配置中缺少学号或密码，请在 config.toml 的 account.loginname / account.password 中填写"
                    .into(),
            ));
        }

        let (captcha, uuid) = self.get_captcha(cfg, ocr, cancel).await?;
        let enc_pwd = aes_encrypt(AES_KEY, &password);
        tracing::info!("正在登录… 学号：{loginname}");

        let url = format!("{}/auth/login", self.base_url);
        let params = vec![
            ("loginname".to_string(), loginname),
            ("password".to_string(), enc_pwd),
            ("captcha".to_string(), captcha),
            ("uuid".to_string(), uuid),
        ];
        let resp = self
            .client
            .post(&url)
            .headers(api_headers(None, None))
            .query(&params)
            .timeout(LOGIN_TIMEOUT)
            .send()
            .await
            .map_err(login_net_err)?;

        let status = resp.status();
        let cookies = extract_cookies(&resp);
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::Parse(format!("读取登录响应失败：{e}")))?;
        if cfg.app.debug {
            let _ = std::fs::write("login_pac.json", &bytes);
        }
        if status != StatusCode::OK {
            let text = String::from_utf8_lossy(&bytes);
            return Err(AppError::Login(format!(
                "登录接口返回 HTTP {status}\n响应：{}",
                truncate(&text, 300)
            )));
        }
        let text = String::from_utf8_lossy(&bytes);
        let p: Value = serde_json::from_str(&text).map_err(|_| {
            AppError::Parse(format!("登录接口返回非 JSON：{}", truncate(&text, 300)))
        })?;

        let code = p.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        let msg = p.get("msg").and_then(|v| v.as_str()).unwrap_or("");
        tracing::info!("登录响应：code={code}, msg={msg}");
        if code != 200 {
            let detail = truncate(&serde_json::to_string(&p).unwrap_or_default(), 500);
            return Err(AppError::Login(format!(
                "登录失败（code={code}）：{msg}\n完整响应：{detail}"
            )));
        }
        if p.pointer("/data/token").and_then(|v| v.as_str()).is_none() {
            let detail = truncate(&serde_json::to_string(&p).unwrap_or_default(), 500);
            return Err(AppError::Login(format!(
                "登录响应缺少 token，完整响应：{detail}"
            )));
        }
        tracing::info!("[OK] 登录成功");
        Ok((p, cookies))
    }

    // ── 课程列表 ────────────────────────────────────────────────

    /// 获取课程列表（JSON body + Content-Type 头）。
    pub async fn get_class(
        &self,
        token: &str,
        batch: &str,
        cfg: &Config,
        category: Category,
    ) -> Result<Vec<CourseRow>, AppError> {
        let url = format!("{}/elective/clazz/list", self.base_url);
        let mut headers = api_headers(Some(token), Some(batch));
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json;charset=UTF-8"),
        );
        let body = serde_json::json!({
            "teachingClassType": category.add_clazz_type(),
            "pageNumber": 1,
            "pageSize": 300,
            "orderBy": "",
            "campus": "S",
        });
        let resp = self
            .client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|e| network_err(e, "课程列表请求"))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::Parse(format!("读取课程列表响应失败：{e}")))?;
        if cfg.app.debug {
            let _ = std::fs::write("classlist.json", &bytes);
        }
        let text = String::from_utf8_lossy(&bytes);
        let v: Value = serde_json::from_str(&text).map_err(|_| {
            AppError::Parse(format!("课程列表接口返回非 JSON：{}", truncate(&text, 200)))
        })?;
        // Python：`resp.get("data", {}).get("rows", [])` —— data 缺失时为空列表
        let rows = v
            .pointer("/data/rows")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| match serde_json::from_value::<CourseRow>(x.clone()) {
                        Ok(r) => Some(r),
                        Err(e) => {
                            tracing::warn!("跳过无法解析的课程行：{e}");
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(rows)
    }

    /// 批量获取多类别课程列表（Python `fetch_courses`，参数为类别集合）。
    pub async fn fetch_courses(
        &self,
        token: &str,
        batch: &str,
        cfg: &Config,
        categories: &HashSet<Category>,
    ) -> Result<HashMap<Category, Vec<CourseRow>>, AppError> {
        let mut out = HashMap::new();
        for &cat in categories {
            let rows = self.get_class(token, batch, cfg, cat).await?;
            out.insert(cat, rows);
        }
        Ok(out)
    }

    // ── 选课 / 退课 ─────────────────────────────────────────────

    /// 选课（URL query + Cookie 注入 token）。
    pub async fn add(
        &self,
        token: &str,
        batch: &str,
        cookies: &HashMap<String, String>,
        course: &CourseRow,
        category: Category,
        cfg: &PollConfig,
    ) -> Result<SelectOutcome, AppError> {
        let url = format!("{}/elective/clazz/add", self.base_url);
        let params = vec![
            (
                "clazzType".to_string(),
                category.add_clazz_type().to_string(),
            ),
            (
                "clazzId".to_string(),
                course.jxbid.clone().unwrap_or_default(),
            ),
            (
                "secretVal".to_string(),
                course.secret_val.clone().unwrap_or_default(),
            ),
            ("chooseVolunteer".to_string(), "1".to_string()),
        ];
        let req = OpRequest::new(
            url,
            params,
            api_headers(Some(token), Some(batch)),
            cookie_header(cookies, token),
        );
        let label = format!("{} {}\t选课", course.kch, course.kcm);

        if cfg.always {
            poll_operation(
                &self.client,
                &req,
                &label,
                StopCondition::Substring(&ADD_STOP_MSGS),
                cfg,
            )
            .await
        } else {
            let msg = send_once(&self.client, &req).await?;
            tracing::info!("{label}\t{msg}");
            Ok(SelectOutcome {
                reason: classify_add(&msg),
                last_msg: msg,
            })
        }
    }

    /// 退课（URL query + Cookie 注入 token；仅选修带 `chooseVolunteer=1`）。
    pub async fn dele(
        &self,
        token: &str,
        batch: &str,
        cookies: &HashMap<String, String>,
        course: &CourseRow,
        category: Category,
        cfg: &PollConfig,
    ) -> Result<SelectOutcome, AppError> {
        let url = format!("{}/elective/clazz/del", self.base_url);
        let mut params = vec![
            (
                "clazzType".to_string(),
                category.dele_clazz_type().to_string(),
            ),
            (
                "clazzId".to_string(),
                course.jxbid.clone().unwrap_or_default(),
            ),
            (
                "secretVal".to_string(),
                course.secret_val.clone().unwrap_or_default(),
            ),
        ];
        if category.dele_has_volunteer() {
            params.push(("chooseVolunteer".to_string(), "1".to_string()));
        }
        let req = OpRequest::new(
            url,
            params,
            api_headers(Some(token), Some(batch)),
            cookie_header(cookies, token),
        );
        let label = format!(
            "{} {}\t{}\t退课",
            course.kch,
            course.kcm,
            course.skjs.as_deref().unwrap_or("")
        );

        if cfg.always {
            poll_operation(
                &self.client,
                &req,
                &label,
                StopCondition::Exact(&DELE_STOP_MSGS),
                cfg,
            )
            .await
        } else {
            let msg = send_once(&self.client, &req).await?;
            tracing::info!("{label}\t{msg}");
            Ok(SelectOutcome {
                reason: classify_dele(&msg),
                last_msg: msg,
            })
        }
    }

    // ── 切换批次（保留 API，CLI/GUI 当前未调用）────────────────

    /// 切换选课批次。
    pub async fn choose_batch(&self, token: &str, batch_id: &str) -> Result<Value, AppError> {
        let url = format!("{}/elective/user", self.base_url);
        let headers = api_headers(Some(token), None);
        let resp = self
            .client
            .post(&url)
            .headers(headers)
            .query(&[("batchId", batch_id)])
            .send()
            .await
            .map_err(|e| network_err(e, "切换批次"))?;
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::Parse(format!("读取切换批次响应失败：{e}")))?;
        serde_json::from_str(&text).map_err(|_| {
            AppError::Parse(format!("切换批次接口返回非 JSON：{}", truncate(&text, 200)))
        })
    }
}

/// 公共请求头（Python `_api_headers`）。
pub(crate) fn api_headers(token: Option<&str>, batch_id: Option<&str>) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    if let Some(t) = token {
        h.insert(header::AUTHORIZATION, header_value(t));
    }
    if let Some(b) = batch_id {
        // HTTP 头名不区分大小写；`HeaderName::from_static` 要求小写
        h.insert(HeaderName::from_static("batchid"), header_value(b));
    }
    h
}

/// 构造 HeaderValue，非法字符回退为空值。
fn header_value(v: &str) -> HeaderValue {
    HeaderValue::from_str(v).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// 构造 Cookie 头：登录收集的 cookie + `Authorization=token`（Python `cookies=` 等价）。
fn cookie_header(cookies: &HashMap<String, String>, token: &str) -> String {
    let mut parts: Vec<String> = cookies.iter().map(|(k, v)| format!("{k}={v}")).collect();
    parts.push(format!("Authorization={token}"));
    parts.join("; ")
}

/// 从登录响应收集 Set-Cookie（Python `dict_from_cookiejar` 等价物）。
fn extract_cookies(resp: &Response) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for v in resp.headers().get_all(header::SET_COOKIE) {
        if let Ok(s) = v.to_str() {
            if let Some((k, rest)) = s.split_once('=') {
                let key = k.trim();
                let value = rest.split(';').next().unwrap_or("").trim().to_string();
                if !key.is_empty() {
                    map.insert(key.to_string(), value);
                }
            }
        }
    }
    map
}

/// 网络错误映射（对齐 Python 验证码请求的错误文案）。
fn captcha_net_err(e: reqwest::Error) -> AppError {
    if e.is_timeout() {
        AppError::Network("验证码接口超时（10s），请检查网络连接".into())
    } else if e.is_connect() {
        AppError::Network("无法连接到选课服务器 xk.xidian.edu.cn，请检查网络或 VPN".into())
    } else {
        AppError::Network(format!("验证码请求失败：{e}"))
    }
}

/// 网络错误映射（对齐 Python 登录请求的错误文案）。
fn login_net_err(e: reqwest::Error) -> AppError {
    if e.is_timeout() {
        AppError::Network("登录接口超时（15s），请检查网络连接".into())
    } else if e.is_connect() {
        AppError::Network("无法连接到选课服务器，请检查网络或 VPN".into())
    } else {
        AppError::Network(format!("登录请求失败：{e}"))
    }
}
