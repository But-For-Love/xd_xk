//! P1 验收：wiremock 假服务器下逐命令对拍 Python CLI 行为；
//! 真实二进制 e2e（conf 迁移 / 补全 / 退出码）。
//!
//! 进程内测试通过 `run_async` + FakeOcr 在同一 runtime 内运行，
//! 断言请求契约与退出码；spawn 测试用 `CARGO_BIN_EXE_xd-xk` 跑真实二进制。

mod common;

use std::collections::HashMap;

use wiremock::MockServer;

use common::*;
use xd_xk_cli::{
    run_async, CategoryArg, CheckArgs, Cli, Command, ConfArgs, ConfCommand, CourseId, DropArgs,
    ListArgs, SelectArgs,
};

/// 进程内运行一次 CLI，返回退出码。
async fn run_cli(cli: Cli) -> i32 {
    run_async(cli, &HashMap::new(), Some(Box::new(FakeOcr("1234".into())))).await
}

fn base(cli: Cli, base_url: String) -> Cli {
    Cli {
        base_url: Some(base_url),
        ..cli
    }
}

fn empty_cli(config: std::path::PathBuf) -> Cli {
    Cli {
        config,
        courses: None,
        verbose: 0,
        debug: false,
        dry_run: false,
        json: false,
        base_url: None,
        command: Command::Login,
    }
}

#[tokio::test]
async fn select_elective_sends_contract_compliant_request() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_classlist(&server).await;
    mock_add(&server, "操作成功").await;

    let dir = TestDir::new("sel-e");
    let config = write_config(&dir, true);
    let courses = write_courses(&dir);
    let cli = base(
        Cli {
            config,
            courses: Some(courses),
            command: Command::Select(SelectArgs {
                courses: vec![],
                category: CategoryArg::Elective,
                interactive: false,
                once: true,
                interval: 1.0,
            }),
            ..empty_cli(Default::default())
        },
        server.uri(),
    );

    let code = run_cli(cli).await;
    assert_eq!(code, 0, "选课应成功");

    let req = find_request(&server, "/elective/clazz/add").await;
    let q = query_map(&req);
    assert_eq!(q.get("clazzType").map(String::as_str), Some("XGKC"));
    assert_eq!(q.get("clazzId").map(String::as_str), Some("JXB-E-1"));
    assert_eq!(q.get("secretVal").map(String::as_str), Some("SV-E-1"));
    assert_eq!(q.get("chooseVolunteer").map(String::as_str), Some("1"));
    // token 同时进请求头与 Cookie
    assert_eq!(
        header_value(&req, "authorization").as_deref(),
        Some("TOKEN1")
    );
    assert_eq!(header_value(&req, "batchid").as_deref(), Some("B2"));
    let cookie = header_value(&req, "cookie").unwrap_or_default();
    assert!(cookie.contains("Authorization=TOKEN1"), "{cookie}");
    assert!(cookie.contains("JSESSIONID=abc123"), "{cookie}");
}

#[tokio::test]
async fn select_required_resolves_tc_list_by_kxh() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_classlist(&server).await;
    mock_add(&server, "操作成功").await;

    let dir = TestDir::new("sel-r");
    let config = write_config(&dir, true);
    let courses = write_courses(&dir);
    let cli = base(
        Cli {
            config,
            courses: Some(courses),
            command: Command::Select(SelectArgs {
                courses: vec![],
                category: CategoryArg::Required,
                interactive: false,
                once: true,
                interval: 1.0,
            }),
            ..empty_cli(Default::default())
        },
        server.uri(),
    );

    assert_eq!(run_cli(cli).await, 0);
    let req = find_request(&server, "/elective/clazz/add").await;
    let q = query_map(&req);
    assert_eq!(q.get("clazzType").map(String::as_str), Some("FANKC"));
    assert_eq!(q.get("clazzId").map(String::as_str), Some("JXB-R-02"));
}

#[tokio::test]
async fn select_from_command_line_course_args() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_classlist(&server).await;
    mock_add(&server, "操作成功").await;

    let dir = TestDir::new("sel-arg");
    let config = write_config(&dir, true);
    // 不提供课程文件：课程从命令行给
    let cli = base(
        Cli {
            config,
            courses: None,
            command: Command::Select(SelectArgs {
                courses: vec![CourseId {
                    kch: "EY226022".into(),
                    kxh: None,
                }],
                category: CategoryArg::Elective,
                interactive: false,
                once: true,
                interval: 1.0,
            }),
            ..empty_cli(Default::default())
        },
        server.uri(),
    );

    assert_eq!(run_cli(cli).await, 0);
    assert_eq!(count_requests(&server, "/elective/clazz/add").await, 1);
}

#[tokio::test]
async fn drop_category_contracts() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_classlist(&server).await;
    mock_del(&server, "操作成功").await;

    let dir = TestDir::new("drop");
    let config = write_config(&dir, true);
    let courses = write_courses(&dir);
    let mk = |cat: CategoryArg| {
        base(
            Cli {
                config: config.clone(),
                courses: Some(courses.clone()),
                command: Command::Drop(DropArgs {
                    courses: vec![],
                    category: cat,
                    once: true,
                    interval: 1.0,
                }),
                ..empty_cli(Default::default())
            },
            server.uri(),
        )
    };

    // 必修退课：TJKC，无 chooseVolunteer
    assert_eq!(run_cli(mk(CategoryArg::Required)).await, 0);
    let req = find_request(&server, "/elective/clazz/del").await;
    let q = query_map(&req);
    assert_eq!(q.get("clazzType").map(String::as_str), Some("TJKC"));
    assert!(
        !q.contains_key("chooseVolunteer"),
        "必修退课不带 chooseVolunteer"
    );

    // 选修退课：XGKC + chooseVolunteer=1
    assert_eq!(run_cli(mk(CategoryArg::Elective)).await, 0);
    let reqs = server.received_requests().await.unwrap();
    let del_reqs: Vec<_> = reqs
        .iter()
        .filter(|r| r.url.path().ends_with("/elective/clazz/del"))
        .collect();
    assert_eq!(del_reqs.len(), 2);
    let q2 = query_map(del_reqs[1]);
    assert_eq!(q2.get("clazzType").map(String::as_str), Some("XGKC"));
    assert_eq!(q2.get("chooseVolunteer").map(String::as_str), Some("1"));
}

#[tokio::test]
async fn dry_run_sends_no_add_request() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_classlist(&server).await;

    let dir = TestDir::new("dry");
    let config = write_config(&dir, true);
    let courses = write_courses(&dir);
    let cli = base(
        Cli {
            config,
            courses: Some(courses),
            dry_run: true,
            command: Command::Select(SelectArgs {
                courses: vec![],
                category: CategoryArg::Elective,
                interactive: false,
                once: true,
                interval: 1.0,
            }),
            ..empty_cli(Default::default())
        },
        server.uri(),
    );

    assert_eq!(run_cli(cli).await, 0);
    assert_eq!(
        count_requests(&server, "/elective/clazz/add").await,
        0,
        "dry-run 不应发选课请求"
    );
}

#[tokio::test]
async fn check_grabs_open_slot_and_exits_after_rounds() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_classlist(&server).await;
    mock_add(&server, "操作成功").await;

    let dir = TestDir::new("check");
    let config = write_config(&dir, true);
    let courses = write_courses(&dir);
    let cli = base(
        Cli {
            config,
            courses: Some(courses),
            command: Command::Check(CheckArgs {
                kch: vec![],
                interval: 0.05,
                no_grab: false,
                until: None,
                rounds: Some(1),
            }),
            ..empty_cli(Default::default())
        },
        server.uri(),
    );

    assert_eq!(run_cli(cli).await, 0, "check 结束退出码应为 0");
    assert!(
        count_requests(&server, "/elective/clazz/add").await >= 1,
        "发现空位应自动选课"
    );
}

#[tokio::test]
async fn check_no_grab_only_monitors() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_classlist(&server).await;

    let dir = TestDir::new("check-nograb");
    let config = write_config(&dir, true);
    let courses = write_courses(&dir);
    let cli = base(
        Cli {
            config,
            courses: Some(courses),
            command: Command::Check(CheckArgs {
                kch: vec![],
                interval: 0.05,
                no_grab: true,
                until: None,
                rounds: Some(1),
            }),
            ..empty_cli(Default::default())
        },
        server.uri(),
    );

    assert_eq!(run_cli(cli).await, 0);
    assert_eq!(
        count_requests(&server, "/elective/clazz/add").await,
        0,
        "--no-grab 不应自动选课"
    );
}

#[tokio::test]
async fn list_prints_matching_courses() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_classlist(&server).await;

    let dir = TestDir::new("list");
    let config = write_config(&dir, true);
    let cli = base(
        Cli {
            config,
            courses: None,
            json: true,
            command: Command::List(ListArgs {
                category: Some(CategoryArg::Elective),
                keyword: Some("操作系统".into()),
            }),
            ..empty_cli(Default::default())
        },
        server.uri(),
    );

    assert_eq!(run_cli(cli).await, 0);
    // JSON 输出无法在进程内捕获，仅验证不报错；spawn 测试验证真实 stdout
}

#[tokio::test]
async fn login_failure_exit_code_2() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login_fail(&server).await;

    let dir = TestDir::new("login-fail");
    let config = write_config(&dir, true);
    let cli = base(empty_cli(config), server.uri());
    assert_eq!(run_cli(cli).await, 2, "登录失败 → 退出码 2");
}

#[tokio::test]
async fn batch_closed_exit_code_4() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login_closed_batch(&server).await;

    let dir = TestDir::new("batch-closed");
    let config = write_config(&dir, true);
    let cli = base(empty_cli(config), server.uri());
    assert_eq!(run_cli(cli).await, 4, "批次未开放 → 退出码 4");
}

#[tokio::test]
async fn missing_credentials_exit_code_1() {
    let server = MockServer::start().await;
    let dir = TestDir::new("no-cred");
    // 空配置（无学号密码）
    let config = dir.join("config.toml");
    std::fs::write(&config, "[app]\nocr_captcha = true\n[account]\n").unwrap();
    let cli = base(empty_cli(config), server.uri());
    assert_eq!(run_cli(cli).await, 1, "缺少凭据 → 退出码 1");
}

#[tokio::test]
async fn missing_courses_file_exit_code_1() {
    let server = MockServer::start().await;
    mock_captcha(&server).await;
    mock_login(&server).await;
    mock_classlist(&server).await;

    let dir = TestDir::new("no-courses");
    let config = write_config(&dir, true);
    // courses 指向不存在的文件
    let cli = base(
        Cli {
            config,
            courses: Some(dir.join("nope.csv")),
            command: Command::Select(SelectArgs {
                courses: vec![],
                category: CategoryArg::Elective,
                interactive: false,
                once: true,
                interval: 1.0,
            }),
            ..empty_cli(Default::default())
        },
        server.uri(),
    );
    assert_eq!(run_cli(cli).await, 1, "课程文件缺失 → 退出码 1");
}

#[tokio::test]
async fn conf_migrate_creates_config_and_courses() {
    let dir = TestDir::new("migrate");
    dir.write(
        "conf.json",
        r#"{
  "ocr_captcha": "1",
  "debug": "0",
  "batch_name": "2025级",
  "bx": [{ "KCH": "TE204003", "KXH": "02", "KCM": "大学物理" }],
  "xx": [{ "KCH": "EY226022", "KXH": "01", "KCM": "操作系统" }],
  "data": { "loginname": "2018000001", "password": "pw123", "captcha": "x", "uuid": "u" }
}"#,
    );

    let cli = Cli {
        config: dir.join("config.toml"),
        courses: Some(dir.join("courses.csv")),
        command: Command::Conf(ConfArgs {
            command: ConfCommand::Migrate,
        }),
        ..empty_cli(Default::default())
    };
    assert_eq!(run_cli(cli).await, 0);

    let config_text = read_text(&dir.join("config.toml"));
    assert!(config_text.contains("ocr_captcha = true"), "{config_text}");
    assert!(
        config_text.contains("loginname = \"2018000001\""),
        "{config_text}"
    );
    assert!(
        config_text.contains("password = \"pw123\""),
        "{config_text}"
    );

    let courses_text = read_text(&dir.join("courses.csv"));
    assert!(
        courses_text.contains("必修,TE204003,02,大学物理"),
        "{courses_text}"
    );
    assert!(
        courses_text.contains("选修,EY226022,01,操作系统"),
        "{courses_text}"
    );
    // 原文件保留
    assert!(dir.join("conf.json").exists());
}

#[tokio::test]
async fn conf_migrate_is_idempotent() {
    let dir = TestDir::new("migrate-2");
    let cli = Cli {
        config: dir.join("config.toml"),
        courses: Some(dir.join("courses.csv")),
        command: Command::Conf(ConfArgs {
            command: ConfCommand::Migrate,
        }),
        ..empty_cli(Default::default())
    };
    // 无 conf.json → 不应报错、不创建文件
    assert_eq!(run_cli(cli).await, 0);
    assert!(!dir.join("config.toml").exists());
}

// ── 真实二进制 spawn 测试（stdout 捕获）─────────────────────────

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_xd-xk")
}

#[test]
fn binary_version_prints_to_stdout() {
    let out = std::process::Command::new(bin())
        .arg("-V")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("xd-xk"), "{text}");
}

#[test]
fn binary_help_lists_subcommands() {
    let out = std::process::Command::new(bin())
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for cmd in [
        "select",
        "drop",
        "check",
        "list",
        "login",
        "conf",
        "completions",
    ] {
        assert!(text.contains(cmd), "help 应含 {cmd}");
    }
}

#[test]
fn binary_completions_bash_emits_script() {
    let out = std::process::Command::new(bin())
        .args(["completions", "bash"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("_xd-xk"), "{text}");
}

#[test]
fn binary_conf_migrate_e2e_from_legacy_json() {
    let dir = TestDir::new("bin-migrate");
    dir.write(
        "conf.json",
        r#"{
  "ocr_captcha": "1",
  "debug": "1",
  "bx": [{ "KCH": "TE204003", "KXH": "02", "KCM": "大学物理" }],
  "xx": [],
  "data": { "loginname": "2018000001", "password": "pw123", "captcha": "x", "uuid": "u" }
}"#,
    );

    let out = std::process::Command::new(bin())
        .current_dir(&dir.path)
        .args(["conf", "migrate"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("迁移完成"), "{text}");
    assert!(text.contains("courses.csv（1 门）"), "{text}");
    assert!(dir.join("config.toml").exists());
    assert!(dir.join("courses.csv").exists());
    let courses_text = read_text(&dir.join("courses.csv"));
    assert!(
        courses_text.contains("必修,TE204003,02,大学物理"),
        "{courses_text}"
    );
    assert!(dir.join("conf.json").exists(), "原文件保留");
}

#[test]
fn binary_select_without_config_exits_1() {
    let dir = TestDir::new("bin-noconf");
    // 无 config.toml、无 conf.json → 缺少凭据 → 退出码 1（不联网、不 OCR）
    let out = std::process::Command::new(bin())
        .current_dir(&dir.path)
        .args(["select", "--once"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
