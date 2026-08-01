//! wiremock 测试共享辅助：假 OCR、测试配置、5 端点 mock、请求断言工具。
//!
//! 响应结构与字段名来自 Python `core.py` 的真实解析逻辑
//! （`/data/captcha`、`/data/token`、`/data/student/electiveBatchList`、
//! 课程行的 `KCH/KXH/KCM/JXBID/secretVal/tcList` 等），等价于
//! Python `debug:"1"` 的 dump 夹具。

#![allow(dead_code)]

use std::collections::HashMap;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use xd_xk_core::config::Config;
use xd_xk_core::course::CourseRow;
use xd_xk_core::ocr::{CaptchaOcr, OcrError};

/// 1x1 透明 PNG（合法验证码图片，供 base64 解码路径用）。
pub const PNG_1X1_B64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// 固定返回指定验证码的假 OCR。
#[derive(Clone)]
pub struct FakeOcr(pub String);

impl CaptchaOcr for FakeOcr {
    fn recognize(&self, _img: &[u8]) -> Result<String, OcrError> {
        Ok(self.0.clone())
    }
}

/// 测试配置：学号 `2018000001`，密码 `123456`（AES 密文已知向量）。
pub fn test_config() -> Config {
    let mut c = Config::default();
    c.app.batch_name = "2025级".into();
    c.account.loginname = "2018000001".into();
    c.account.password = "123456".into();
    c
}

/// 测试用取消令牌。
pub fn test_cancel() -> CancellationToken {
    CancellationToken::new()
}

/// mock `/auth/captcha`：返回 base64 PNG + uuid。
pub async fn mock_captcha(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/auth/captcha"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "code": 200,
            "msg": "ok",
            "data": {
                "captcha": format!("data:image/png;base64,{PNG_1X1_B64}"),
                "uuid": "UUID1"
            }
        })))
        .mount(server)
        .await;
}

/// 登录响应负载（真实字段结构）。
pub fn login_payload() -> serde_json::Value {
    json!({
        "code": 200,
        "msg": "登录成功",
        "data": {
            "token": "TOKEN1",
            "student": {
                "XM": "张三",
                "ZYMC": "计算机科学与技术",
                "schoolClass": "CS2301",
                "electiveBatchList": [
                    { "name": "2025级春季", "code": "B1", "canSelect": "0" },
                    { "name": "2025级秋季", "code": "B2", "canSelect": "1" }
                ]
            }
        }
    })
}

/// mock `/auth/login`：返回 token + 学生信息 + 一个会话 cookie。
pub async fn mock_login(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(login_payload())
                .insert_header("Set-Cookie", "JSESSIONID=abc123; Path=/"),
        )
        .mount(server)
        .await;
}

/// 课程列表负载：1 门选修（平铺）+ 1 门必修（含 tcList）。
pub fn classlist_payload() -> serde_json::Value {
    json!({
        "code": 200,
        "msg": "ok",
        "data": {
            "rows": [
                {
                    "KCH": "EY226022", "KXH": "01", "KCM": "操作系统",
                    "JXBID": "JXB-E-1", "secretVal": "SV-E-1",
                    "SKJS": "王老师", "numberOfSelected": 30,
                    "classCapacity": 60, "SFYX": "0"
                },
                {
                    "KCH": "TE204003", "KXH": "01", "KCM": "大学物理",
                    "JXBID": "JXB-R-1", "secretVal": "SV-R-1",
                    "tcList": [
                        { "KCH": "TE204003", "KXH": "01", "KCM": "大学物理(01)",
                          "JXBID": "JXB-R-01", "secretVal": "SV-R-01" },
                        { "KCH": "TE204003", "KXH": "02", "KCM": "大学物理(02)",
                          "JXBID": "JXB-R-02", "secretVal": "SV-R-02" }
                    ]
                }
            ]
        }
    })
}

/// mock `/elective/clazz/list`：按 `teachingClassType` 返回对应负载。
pub async fn mock_classlist(server: &MockServer) {
    use wiremock::matchers::body_json;
    let elective_body = json!({
        "teachingClassType": "XGKC",
        "pageNumber": 1,
        "pageSize": 300,
        "orderBy": "",
        "campus": "S",
    });
    let required_body = json!({
        "teachingClassType": "FANKC",
        "pageNumber": 1,
        "pageSize": 300,
        "orderBy": "",
        "campus": "S",
    });
    // 选修（XGKC）：只返回平铺的选修课
    Mock::given(method("POST"))
        .and(path("/elective/clazz/list"))
        .and(body_json(elective_body))
        .respond_with(ResponseTemplate::new(200).set_body_json(elective_classlist_payload()))
        .mount(server)
        .await;
    // 必修（FANKC）：只返回含 tcList 的必修课
    Mock::given(method("POST"))
        .and(path("/elective/clazz/list"))
        .and(body_json(required_body))
        .respond_with(ResponseTemplate::new(200).set_body_json(required_classlist_payload()))
        .mount(server)
        .await;
}

/// 选修课程列表负载（平铺）。
pub fn elective_classlist_payload() -> serde_json::Value {
    json!({
        "code": 200,
        "msg": "ok",
        "data": { "rows": [classlist_payload()["data"]["rows"][0].clone()] }
    })
}

/// 必修课程列表负载（含 tcList）。
pub fn required_classlist_payload() -> serde_json::Value {
    json!({
        "code": 200,
        "msg": "ok",
        "data": { "rows": [classlist_payload()["data"]["rows"][1].clone()] }
    })
}

/// mock `/elective/clazz/add`：返回指定 msg（默认「操作成功」）。
pub async fn mock_add(server: &MockServer, msg: &str) {
    Mock::given(method("POST"))
        .and(path("/elective/clazz/add"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "msg": msg })))
        .mount(server)
        .await;
}

/// mock `/elective/clazz/del`。
pub async fn mock_del(server: &MockServer, msg: &str) {
    Mock::given(method("POST"))
        .and(path("/elective/clazz/del"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "msg": msg })))
        .mount(server)
        .await;
}

/// 选修课程行（平铺）。
pub fn elective_course() -> CourseRow {
    CourseRow {
        kch: "EY226022".into(),
        kxh: "01".into(),
        kcm: "操作系统".into(),
        jxbid: Some("JXB-E-1".into()),
        secret_val: Some("SV-E-1".into()),
        skjs: Some("王老师".into()),
        number_selected: Some(xd_xk_core::course::Num(30)),
        class_capacity: Some(xd_xk_core::course::Num(60)),
        sfyx: Some("0".into()),
        ..Default::default()
    }
}

/// 必修 tcList 子项（02 班）。
pub fn required_course_02() -> CourseRow {
    CourseRow {
        kch: "TE204003".into(),
        kxh: "02".into(),
        kcm: "大学物理(02)".into(),
        jxbid: Some("JXB-R-02".into()),
        secret_val: Some("SV-R-02".into()),
        ..Default::default()
    }
}

/// 提取请求的 query 参数表。
pub fn query_map(req: &Request) -> HashMap<String, String> {
    req.url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// 从服务器收到的请求中按路径后缀查找。
pub async fn find_request(server: &MockServer, path_suffix: &str) -> Request {
    let reqs = server.received_requests().await.expect("应有收到请求");
    reqs.iter()
        .find(|r| r.url.path().ends_with(path_suffix))
        .cloned()
        .unwrap_or_else(|| panic!("未找到路径含 {path_suffix} 的请求"))
}

/// 请求头取值（可能多个同名头，取首个）。
pub fn header_value(req: &Request, name: &str) -> Option<String> {
    req.headers
        .get(name)
        .map(|v| v.to_str().unwrap_or_default().to_string())
}
