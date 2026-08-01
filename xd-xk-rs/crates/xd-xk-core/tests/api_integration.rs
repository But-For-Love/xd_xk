//! P0 验收：wiremock 假服务器跑通「登录 → 批次 → 取课 → add/del」全流程，
//! 并对接口契约逐条断言（设计文档 §3）。

mod common;

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use wiremock::MockServer;

use common::*;
use xd_xk_core::api::Api;
use xd_xk_core::config::Config;
use xd_xk_core::course::{resolve_target, Category};
use xd_xk_core::ops::{PollConfig, StopReason};
use xd_xk_core::session::CourseSession;

/// 指向 mock 服务器的 Api。
fn mock_api(server: &MockServer) -> Api {
    Api::new(server.uri()).expect("创建 Api")
}

#[tokio::test]
async fn login_query_contract_and_cookie_harvest() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;

    let api = mock_api(&server);
    let ocr = FakeOcr("1234".into());
    let cancel = test_cancel();
    let (data, cookies) = api
        .login(&test_config(), &ocr, &cancel)
        .await
        .expect("登录应成功");

    // 响应解析
    assert_eq!(data["data"]["token"], "TOKEN1");
    assert_eq!(
        cookies.get("JSESSIONID").map(String::as_str),
        Some("abc123")
    );

    // 契约：登录参数走 URL query
    let req = find_request(&server, "/auth/login").await;
    let q = query_map(&req);
    assert_eq!(q.get("loginname").map(String::as_str), Some("2018000001"));
    // AES-128-ECB 密文（Python 权威向量）
    assert_eq!(
        q.get("password").map(String::as_str),
        Some("OSfRhnd673K1Lp6cP4L6nA==")
    );
    assert_eq!(q.get("captcha").map(String::as_str), Some("1234"));
    assert_eq!(q.get("uuid").map(String::as_str), Some("UUID1"));
    assert_eq!(req.method, "POST");

    // 契约：验证码请求只发一次、路径正确
    let reqs = server.received_requests().await.unwrap();
    assert_eq!(
        reqs.iter()
            .filter(|r| r.url.path().ends_with("/auth/captcha"))
            .count(),
        1
    );
}

#[tokio::test]
async fn full_flow_login_batch_class_add_del() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_classlist(&server).await;
    mock_add(&server, "操作成功").await;
    mock_del(&server, "操作成功").await;

    let api = Arc::new(mock_api(&server));
    let ocr = FakeOcr("1234".into());
    let cancel = test_cancel();
    let cfg = test_config();

    // 登录 → 展示 → 匹配批次
    let session = CourseSession::create_with_api(api.clone(), &cfg, &ocr, &cancel)
        .await
        .expect("会话创建应成功");
    assert_eq!(session.batch_code, "B2", "关键字应命中可选批次");

    // 取课：选修平铺 + 必修 tcList 解析
    let elective_rows = session.get_class(&cfg, Category::Elective).await.unwrap();
    let required_rows = session.get_class(&cfg, Category::Required).await.unwrap();
    assert_eq!(elective_rows.len(), 1);
    assert_eq!(elective_rows[0].kch, "EY226022");
    assert_eq!(required_rows.len(), 1);
    let target = resolve_target(&required_rows, Category::Required, "TE204003", "02").unwrap();
    assert_eq!(target.jxbid.as_deref(), Some("JXB-R-02"));

    // add（选修，单次）
    let poll = PollConfig {
        always: false,
        ..Default::default()
    };
    let outcome = session
        .add(&elective_course(), Category::Elective, &poll)
        .await
        .unwrap();
    assert_eq!(outcome.reason, StopReason::Success);
    assert_eq!(outcome.last_msg, "操作成功");

    // dele（必修，单次）
    let outcome = session
        .dele(target, Category::Required, &poll)
        .await
        .unwrap();
    assert_eq!(outcome.reason, StopReason::Success);
}

#[tokio::test]
async fn get_class_uses_json_body_and_content_type() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_classlist(&server).await;

    let api = Arc::new(mock_api(&server));
    let session =
        CourseSession::create_with_api(api, &test_config(), &FakeOcr("1".into()), &test_cancel())
            .await
            .unwrap();

    let _ = session
        .get_class(&test_config(), Category::Elective)
        .await
        .unwrap();
    let req = find_request(&server, "/elective/clazz/list").await;

    // Content-Type 契约
    assert_eq!(
        header_value(&req, "content-type").as_deref(),
        Some("application/json;charset=UTF-8")
    );
    // 无 Cookie（Python get_class 不传 cookies）
    assert!(
        header_value(&req, "cookie").is_none(),
        "get_class 不应带 Cookie"
    );
    // JSON body 契约
    let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body["teachingClassType"], "XGKC");
    assert_eq!(body["pageNumber"], 1);
    assert_eq!(body["pageSize"], 300);
    assert_eq!(body["orderBy"], "");
    assert_eq!(body["campus"], "S");
}

#[tokio::test]
async fn add_sends_query_cookie_and_headers() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_add(&server, "操作成功").await;

    let api = Arc::new(mock_api(&server));
    let session =
        CourseSession::create_with_api(api, &test_config(), &FakeOcr("1".into()), &test_cancel())
            .await
            .unwrap();

    let poll = PollConfig {
        always: false,
        ..Default::default()
    };
    let _ = session
        .add(&elective_course(), Category::Elective, &poll)
        .await
        .unwrap();
    let req = find_request(&server, "/elective/clazz/add").await;
    let q = query_map(&req);

    assert_eq!(q.get("clazzType").map(String::as_str), Some("XGKC"));
    assert_eq!(q.get("clazzId").map(String::as_str), Some("JXB-E-1"));
    assert_eq!(q.get("secretVal").map(String::as_str), Some("SV-E-1"));
    assert_eq!(q.get("chooseVolunteer").map(String::as_str), Some("1"));

    // 契约：token 同时进请求头与 Cookie
    assert_eq!(
        header_value(&req, "authorization").as_deref(),
        Some("TOKEN1")
    );
    assert_eq!(header_value(&req, "batchid").as_deref(), Some("B2"));
    let cookie = header_value(&req, "cookie").unwrap_or_default();
    assert!(
        cookie.contains("Authorization=TOKEN1"),
        "Cookie 应含 Authorization: {cookie}"
    );
    assert!(
        cookie.contains("JSESSIONID=abc123"),
        "Cookie 应含登录会话: {cookie}"
    );
}

#[tokio::test]
async fn required_add_uses_fankc() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_add(&server, "操作成功").await;

    let api = Arc::new(mock_api(&server));
    let session =
        CourseSession::create_with_api(api, &test_config(), &FakeOcr("1".into()), &test_cancel())
            .await
            .unwrap();

    let poll = PollConfig {
        always: false,
        ..Default::default()
    };
    let _ = session
        .add(&required_course_02(), Category::Required, &poll)
        .await
        .unwrap();
    let q = query_map(&find_request(&server, "/elective/clazz/add").await);
    assert_eq!(q.get("clazzType").map(String::as_str), Some("FANKC"));
}

#[tokio::test]
async fn dele_category_contracts() {
    // 必修退课：TJKC，无 chooseVolunteer
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_del(&server, "操作成功").await;

    let api = Arc::new(mock_api(&server));
    let session =
        CourseSession::create_with_api(api, &test_config(), &FakeOcr("1".into()), &test_cancel())
            .await
            .unwrap();

    let poll = PollConfig {
        always: false,
        ..Default::default()
    };
    let _ = session
        .dele(&required_course_02(), Category::Required, &poll)
        .await
        .unwrap();
    let req = find_request(&server, "/elective/clazz/del").await;
    let q = query_map(&req);
    assert_eq!(q.get("clazzType").map(String::as_str), Some("TJKC"));
    assert!(
        !q.contains_key("chooseVolunteer"),
        "必修退课不带 chooseVolunteer"
    );
    let cookie = header_value(&req, "cookie").unwrap_or_default();
    assert!(cookie.contains("Authorization=TOKEN1"));

    // 选修退课：XGKC + chooseVolunteer=1（同一 server 复用，mock 仍响应）
    let _ = session
        .dele(&elective_course(), Category::Elective, &poll)
        .await
        .unwrap();
    let reqs = server.received_requests().await.unwrap();
    let del_reqs: Vec<_> = reqs
        .iter()
        .filter(|r| r.url.path().ends_with("/elective/clazz/del"))
        .collect();
    let q2 = query_map(del_reqs[1]);
    assert_eq!(q2.get("clazzType").map(String::as_str), Some("XGKC"));
    assert_eq!(q2.get("chooseVolunteer").map(String::as_str), Some("1"));
}

#[tokio::test]
async fn poll_stops_on_stop_message() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_add(&server, "该课程已在选课结果中").await; // 子串命中即停

    let api = Arc::new(mock_api(&server));
    let session =
        CourseSession::create_with_api(api, &test_config(), &FakeOcr("1".into()), &test_cancel())
            .await
            .unwrap();

    let poll = PollConfig {
        always: true,
        interval: Duration::from_millis(10),
        ..Default::default()
    };
    let outcome = session
        .add(&elective_course(), Category::Elective, &poll)
        .await
        .unwrap();
    assert_eq!(outcome.reason, StopReason::AlreadySelected);
    // Python 语义：无论是否命中都会先发一次请求
    let reqs = server.received_requests().await.unwrap();
    assert!(!reqs.is_empty());
}

#[tokio::test]
async fn cancel_stops_polling_with_canceled_reason() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_add(&server, "正在排队").await; // 永不命中终止消息

    let api = Arc::new(mock_api(&server));
    let session =
        CourseSession::create_with_api(api, &test_config(), &FakeOcr("1".into()), &test_cancel())
            .await
            .unwrap();

    let cancel = CancellationToken::new();
    let poll = PollConfig {
        always: true,
        interval: Duration::from_millis(20),
        cancel: cancel.clone(),
    };
    let course = elective_course();
    let handle = tokio::spawn(async move { session.add(&course, Category::Elective, &poll).await });

    tokio::time::sleep(Duration::from_millis(120)).await;
    cancel.cancel();
    let outcome = handle.await.unwrap().unwrap();
    assert_eq!(outcome.reason, StopReason::Canceled);
}

#[tokio::test]
async fn login_failure_maps_to_login_error() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/auth/login"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 500,
                "msg": "验证码错误",
                "data": null
            })),
        )
        .mount(&server)
        .await;

    let api = mock_api(&server);
    let err = api
        .login(&test_config(), &FakeOcr("wrong".into()), &test_cancel())
        .await
        .expect_err("应登录失败");
    assert!(err.to_string().contains("登录失败"), "{err}");
    assert!(err.to_string().contains("验证码错误"), "{err}");
    assert_eq!(err.exit_code(), 2, "登录失败 → 退出码 2");
}

#[tokio::test]
async fn batch_not_open_maps_to_batch_error() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    // 只有已关闭批次
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/auth/login"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 200,
                "msg": "ok",
                "data": {
                    "token": "T",
                    "student": {
                        "XM": "张三", "ZYMC": "计科", "schoolClass": "CS01",
                        "electiveBatchList": [
                            { "name": "2025级春季", "code": "B1", "canSelect": "0" }
                        ]
                    }
                }
            })),
        )
        .mount(&server)
        .await;

    let api = Arc::new(mock_api(&server));
    let err = match CourseSession::create_with_api(
        api,
        &test_config(),
        &FakeOcr("1".into()),
        &test_cancel(),
    )
    .await
    {
        Ok(_) => panic!("应失败：批次未开放"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("本轮选课暂未开始"), "{err}");
    assert_eq!(err.exit_code(), 4, "批次未开放 → 退出码 4");
}

#[tokio::test]
async fn missing_credentials_is_config_error() {
    let server = MockServer::start().await;
    let api = mock_api(&server);
    let mut cfg = Config::default();
    cfg.account.loginname.clear();
    let err = api
        .login(&cfg, &FakeOcr("1".into()), &test_cancel())
        .await
        .expect_err("缺学号应报错");
    assert!(err.to_string().contains("缺少学号或密码"), "{err}");
    assert_eq!(err.exit_code(), 1);
}
