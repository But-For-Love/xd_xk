//! 选课 / 退课轮询（模板方法，复刻 Python `_poll_operation`）。
//!
//! 契约要点（硬性约束，1:1 复刻）：
//! - `add` 终止：5 条消息**子串匹配**；`dele` 终止：2 条消息**精确匹配**；
//! - 轮询间隔：add/del 1s，check 0.5s，snipe 5~8s；
//! - 取消用 `tokio_util::sync::CancellationToken`（等价 Python `stop_event`）。

use std::time::Duration;

use reqwest::header::{self, HeaderMap};
use tokio_util::sync::CancellationToken;

use crate::error::{truncate, AppError};

/// add 终止消息（子串匹配，逐字保留）。
pub const ADD_STOP_MSGS: [&str; 5] = [
    "该课程已在选课结果中",
    "所选课程与已选课程冲突",
    "所选课程人数已满",
    "操作成功",
    "选课门数或学分超过",
];

/// dele 终止消息（精确匹配，逐字保留）。
pub const DELE_STOP_MSGS: [&str; 2] = ["所选课程与已选课程冲突", "操作成功"];

/// 终止原因（GUI 未来据此上色，等价 Python `_dispatch_log` 的颜色逻辑）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// 操作成功。
    Success,
    /// 该课程已在选课结果中。
    AlreadySelected,
    /// 所选课程与已选课程冲突。
    Conflict,
    /// 所选课程人数已满。
    Full,
    /// 选课门数或学分超过。
    CreditLimit,
    /// 用户主动停止。
    Canceled,
    /// 未命中任何终止消息（单次请求路径可能返回）。
    Unknown,
}

/// 一次选课 / 退课操作的强类型结果。
#[derive(Debug, Clone)]
pub struct SelectOutcome {
    pub reason: StopReason,
    pub last_msg: String,
}

/// 轮询配置。
#[derive(Debug, Clone)]
pub struct PollConfig {
    /// true = 持续重试直到命中终止消息或被取消；false = 只发一次。
    pub always: bool,
    /// 轮询间隔（add/del 默认 1s）。
    pub interval: Duration,
    /// 取消令牌。
    pub cancel: CancellationToken,
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            always: true,
            interval: Duration::from_secs(1),
            cancel: CancellationToken::new(),
        }
    }
}

/// 停止条件：子串匹配（add）或精确匹配（dele）。
#[derive(Debug, Clone, Copy)]
pub(crate) enum StopCondition {
    Substring(&'static [&'static str]),
    Exact(&'static [&'static str]),
}

impl StopCondition {
    pub(crate) fn matches(&self, msg: &str) -> bool {
        match self {
            StopCondition::Substring(msgs) => msgs.iter().any(|m| msg.contains(m)),
            StopCondition::Exact(msgs) => msgs.contains(&msg),
        }
    }
}

/// 一次选课 / 退课请求的全部构造参数。
pub(crate) struct OpRequest {
    pub url: String,
    pub params: Vec<(String, String)>,
    pub headers: HeaderMap,
    pub cookie_str: String,
}

impl OpRequest {
    pub fn new(
        url: impl Into<String>,
        params: Vec<(String, String)>,
        headers: HeaderMap,
        cookie_str: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            params,
            headers,
            cookie_str: cookie_str.into(),
        }
    }
}

/// add 终止消息分类。
pub fn classify_add(msg: &str) -> StopReason {
    if msg.contains("操作成功") {
        StopReason::Success
    } else if msg.contains("该课程已在选课结果中") {
        StopReason::AlreadySelected
    } else if msg.contains("所选课程与已选课程冲突") {
        StopReason::Conflict
    } else if msg.contains("所选课程人数已满") {
        StopReason::Full
    } else if msg.contains("选课门数或学分超过") {
        StopReason::CreditLimit
    } else {
        StopReason::Unknown
    }
}

/// dele 终止消息分类。
pub fn classify_dele(msg: &str) -> StopReason {
    match msg {
        "操作成功" => StopReason::Success,
        "所选课程与已选课程冲突" => StopReason::Conflict,
        _ => StopReason::Unknown,
    }
}

/// 轮询发送请求直到命中停止条件或取消（复刻 Python `_poll_operation`）。
///
/// 注意：与 Python 一致，每次 POST 后 sleep 一次——即使本次命中终止消息，
/// 也会多等一个 `interval` 再退出。
pub(crate) async fn poll_operation(
    client: &reqwest::Client,
    req: &OpRequest,
    label: &str,
    stop_condition: StopCondition,
    cfg: &PollConfig,
) -> Result<SelectOutcome, AppError> {
    let mut k: usize = 1;
    let mut msg = String::new();
    loop {
        if cfg.cancel.is_cancelled() {
            tracing::info!("用户停止操作");
            return Ok(SelectOutcome {
                reason: StopReason::Canceled,
                last_msg: msg,
            });
        }
        if stop_condition.matches(&msg) {
            return Ok(SelectOutcome {
                reason: classify(&msg, stop_condition),
                last_msg: msg,
            });
        }
        msg = send_once(client, req).await?;
        let dashes = "-".repeat(k % 10);
        tracing::info!("{label}\t{msg}{dashes}");
        k += 1;
        tokio::time::sleep(cfg.interval).await;
    }
}

/// 单次请求并解析 `msg`（always=0 路径，无进度横杠）。
pub(crate) async fn send_once(
    client: &reqwest::Client,
    req: &OpRequest,
) -> Result<String, AppError> {
    let resp = client
        .post(&req.url)
        .headers(req.headers.clone())
        .query(&req.params)
        .header(header::COOKIE, &req.cookie_str)
        .send()
        .await
        .map_err(|e| network_err(e, "选课请求"))?;
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Parse(format!("读取响应失败：{e}")))?;
    parse_msg(&text)
}

fn classify(msg: &str, cond: StopCondition) -> StopReason {
    match cond {
        StopCondition::Substring(_) => classify_add(msg),
        StopCondition::Exact(_) => classify_dele(msg),
    }
}

/// 解析响应 JSON 的 `msg` 字段（字段缺失报可读错误）。
pub(crate) fn parse_msg(text: &str) -> Result<String, AppError> {
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|_| AppError::Parse(format!("接口返回非 JSON：{}", truncate(text, 200))))?;
    v.get("msg")
        .and_then(|m| m.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Parse(format!("接口响应缺少 msg 字段：{}", truncate(text, 200))))
}

/// 网络错误 → 中文文案（对齐 Python 的 Timeout/ConnectionError 分支）。
pub(crate) fn network_err(e: reqwest::Error, what: &str) -> AppError {
    if e.is_timeout() {
        AppError::Network(format!("{what}超时，请检查网络连接"))
    } else if e.is_connect() {
        AppError::Network("无法连接到选课服务器 xk.xidian.edu.cn，请检查网络或 VPN".into())
    } else {
        AppError::Network(format!("{what}失败：{e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_stop_is_substring_match() {
        let cond = StopCondition::Substring(&ADD_STOP_MSGS);
        // Python: `any(s in msg for s in _STOP_MSGS)`
        assert!(cond.matches("操作成功"));
        assert!(cond.matches("前缀[所选课程人数已满]后缀")); // 子串也命中
        assert!(!cond.matches("正在排队"));
    }

    #[test]
    fn dele_stop_is_exact_match() {
        let cond = StopCondition::Exact(&DELE_STOP_MSGS);
        assert!(cond.matches("操作成功"));
        assert!(cond.matches("所选课程与已选课程冲突"));
        // 精确匹配：子串不算
        assert!(!cond.matches("前缀操作成功后缀"));
        assert!(!cond.matches(""));
    }

    #[test]
    fn classify_reasons() {
        assert_eq!(classify_add("操作成功"), StopReason::Success);
        assert_eq!(
            classify_add("该课程已在选课结果中"),
            StopReason::AlreadySelected
        );
        assert_eq!(classify_add("所选课程与已选课程冲突"), StopReason::Conflict);
        assert_eq!(classify_add("所选课程人数已满"), StopReason::Full);
        assert_eq!(classify_add("选课门数或学分超过"), StopReason::CreditLimit);
        assert_eq!(classify_add("其他"), StopReason::Unknown);
        assert_eq!(classify_dele("操作成功"), StopReason::Success);
        assert_eq!(classify_dele("其他"), StopReason::Unknown);
    }

    #[test]
    fn parse_msg_errors_on_missing_field() {
        assert_eq!(parse_msg(r#"{"msg":"操作成功"}"#).unwrap(), "操作成功");
        assert!(parse_msg(r#"{"code":1}"#).is_err());
        assert!(parse_msg("not json").is_err());
    }
}
