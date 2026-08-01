//! CLI 测试共享辅助：wiremock 5 端点 mock、FakeOcr、临时目录、请求断言。
//!
//! 响应结构等价于 Python `debug:"1"` 的 dump 夹具（同 xd-xk-core 测试），
//! 保证 P0/P1 对拍使用同一份契约。

#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use xd_xk_core::ocr::{CaptchaOcr, OcrError};

/// 1x1 透明 PNG（合法验证码图片，供 base64 解码路径用）。
pub const PNG_1X1_B64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// 固定返回指定验证码的假 OCR（测试避开真实 OCR / 人工输入）。
pub struct FakeOcr(pub String);

impl CaptchaOcr for FakeOcr {
    fn recognize(&self, _img: &[u8]) -> Result<String, OcrError> {
        Ok(self.0.clone())
    }
}

static DIR_SEQ: AtomicU64 = AtomicU64::new(0);

/// 独立临时目录（每次调用唯一，Drop 时清理）。
pub struct TestDir {
    pub path: PathBuf,
}

impl TestDir {
    pub fn new(name: &str) -> Self {
        let n = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("xd-xk-cli-{name}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    pub fn write(&self, name: &str, content: &str) {
        fs::write(self.join(name), content).unwrap();
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// 写带凭据与批次的 config.toml。
pub fn write_config(dir: &TestDir, ocr_captcha: bool) -> PathBuf {
    let path = dir.join("config.toml");
    let text = format!(
        "[app]\nocr_captcha = {ocr_captcha}\ndebug = false\nbatch_name = \"2025级\"\n\n\
         [account]\nloginname = \"2018000001\"\npassword = \"123456\"\n"
    );
    fs::write(&path, text).unwrap();
    path
}

/// 写课程文件：1 门选修（平铺）+ 1 门必修。
pub fn write_courses(dir: &TestDir) -> PathBuf {
    let path = dir.join("courses.csv");
    fs::write(
        &path,
        "类别,KCH,KXH,KCM\n选修,EY226022,01,操作系统\n必修,TE204003,02,大学物理\n",
    )
    .unwrap();
    path
}

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

pub async fn mock_login(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
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
                }))
                .insert_header("Set-Cookie", "JSESSIONID=abc123; Path=/"),
        )
        .mount(server)
        .await;
}

pub async fn mock_classlist(server: &MockServer) {
    let elective_body = json!({
        "teachingClassType": "XGKC",
        "pageNumber": 1, "pageSize": 300, "orderBy": "", "campus": "S",
    });
    let required_body = json!({
        "teachingClassType": "FANKC",
        "pageNumber": 1, "pageSize": 300, "orderBy": "", "campus": "S",
    });
    Mock::given(method("POST"))
        .and(path("/elective/clazz/list"))
        .and(body_json(elective_body))
        .respond_with(ResponseTemplate::new(200).set_body_json(elective_classlist_payload()))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/elective/clazz/list"))
        .and(body_json(required_body))
        .respond_with(ResponseTemplate::new(200).set_body_json(required_classlist_payload()))
        .mount(server)
        .await;
}

/// 选修课程列表：平铺的 EY226022，SFYX=0、30/60 有空位。
pub fn elective_classlist_payload() -> serde_json::Value {
    json!({
        "code": 200, "msg": "ok",
        "data": { "rows": [{
            "KCH": "EY226022", "KXH": "01", "KCM": "操作系统",
            "JXBID": "JXB-E-1", "secretVal": "SV-E-1",
            "SKJS": "王老师", "numberOfSelected": 30,
            "classCapacity": 60, "SFYX": "0"
        }] }
    })
}

/// 必修课程列表：含 tcList 的 TE204003（顶层无 SFYX，check 模式不会误加）。
pub fn required_classlist_payload() -> serde_json::Value {
    json!({
        "code": 200, "msg": "ok",
        "data": { "rows": [{
            "KCH": "TE204003", "KXH": "01", "KCM": "大学物理",
            "JXBID": "JXB-R-1", "secretVal": "SV-R-1",
            "tcList": [
                { "KCH": "TE204003", "KXH": "01", "KCM": "大学物理(01)",
                  "JXBID": "JXB-R-01", "secretVal": "SV-R-01" },
                { "KCH": "TE204003", "KXH": "02", "KCM": "大学物理(02)",
                  "JXBID": "JXB-R-02", "secretVal": "SV-R-02" }
            ]
        }] }
    })
}

pub async fn mock_add(server: &MockServer, msg: &str) {
    Mock::given(method("POST"))
        .and(path("/elective/clazz/add"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "msg": msg })))
        .mount(server)
        .await;
}

pub async fn mock_del(server: &MockServer, msg: &str) {
    Mock::given(method("POST"))
        .and(path("/elective/clazz/del"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "msg": msg })))
        .mount(server)
        .await;
}

/// 登录返回固定失败响应（code=500）。
pub async fn mock_login_fail(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "code": 500, "msg": "验证码错误", "data": null
        })))
        .mount(server)
        .await;
}

/// 登录返回只有未开放批次的响应。
pub async fn mock_login_closed_batch(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "code": 200, "msg": "ok",
            "data": {
                "token": "TOKEN1",
                "student": {
                    "XM": "张三", "ZYMC": "计科", "schoolClass": "CS01",
                    "electiveBatchList": [
                        { "name": "2025级春季", "code": "B1", "canSelect": "0" }
                    ]
                }
            }
        })))
        .mount(server)
        .await;
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

/// 请求头取值（取首个同名头）。
pub fn header_value(req: &Request, name: &str) -> Option<String> {
    req.headers
        .get(name)
        .map(|v| v.to_str().unwrap_or_default().to_string())
}

/// 统计某路径请求数量。
pub async fn count_requests(server: &MockServer, path_suffix: &str) -> usize {
    server
        .received_requests()
        .await
        .map(|reqs| {
            reqs.iter()
                .filter(|r| r.url.path().ends_with(path_suffix))
                .count()
        })
        .unwrap_or(0)
}

/// 读文本文件（剥 BOM）。
pub fn read_text(path: &Path) -> String {
    let bytes = fs::read(path).unwrap();
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    String::from_utf8_lossy(body).into_owned()
}
