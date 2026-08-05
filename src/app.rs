use serde_json::Value;

use crate::api::{ApiClient, LoginResult, prompt_credentials};
use crate::config::Config;
use crate::error::XkError;
use crate::model::{
    Batch, CourseRow, CourseTarget, extract_class_rows, extract_student_and_batches,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Select,
    Drop,
}

impl Action {
    fn is_drop(self) -> bool {
        matches!(self, Action::Drop)
    }
}

/// 对配置中的课程执行选课或退课。
pub async fn perform_action(
    config: Config,
    action: Action,
    category: u8,
    courses_override: Option<String>,
    always: bool,
) -> Result<(), XkError> {
    if category > 1 {
        return Err(XkError::Config(format!(
            "未知课程类别：{category}，只支持 0（必修）或 1（选修）"
        )));
    }

    let requested = if let Some(text) = courses_override {
        crate::config::parse_course_list(&text, category)?
    } else if category == 0 {
        config.required_courses.clone()
    } else {
        config.elective_courses.clone()
    };
    if requested.is_empty() {
        return Err(XkError::Config(
            "没有配置目标课程，请先进入配置编辑模式（config）".into(),
        ));
    }

    let api = ApiClient::new(config.clone())?;
    let (loginname, password) = prompt_credentials(&config).await?;
    let login = api.login(loginname, password).await?;
    let batch = show_msg(&login, &config.batch_keyword)?;

    let payload = api.get_class(&login, &batch, category).await?;
    let rows = extract_class_rows(&payload)?;
    let matches = find_classes(&rows, &requested, category);

    for (target, class_info) in matches {
        let Some(class_info) = class_info else {
            if category == 0 {
                println!(
                    "未找到课程 {}，课序号 {}；可能是课程不存在或接口字段已变化。",
                    target.kch,
                    target.kxh.as_deref().unwrap_or_default()
                );
            } else {
                println!(
                    "未找到课程 {}；可能是课程不存在或接口字段已变化。",
                    target.kch
                );
            }
            continue;
        };

        api.run_action(
            &login,
            &class_info,
            &batch,
            category,
            action.is_drop(),
            always,
        )
        .await?;
    }

    Ok(())
}

/// 只读兼容性检测：登录、读批次与课程列表，不调用选/退课接口。
pub async fn compatibility_test(config: Config) -> Result<(), XkError> {
    let api = ApiClient::new(config.clone())?;
    let (loginname, password) = prompt_credentials(&config).await?;

    println!("\n开始只读兼容性检测，不会发送选课或退课请求。");
    println!("[1/4] 获取验证码并登录……");
    let login = api.login(loginname, password).await?;
    println!("      成功：登录响应包含 token。");

    println!("[2/4] 检查四年间使用的学生与批次定位字段……");
    let (student, batches) = extract_student_and_batches(&login.payload)?;
    let mut old_fields = Vec::new();
    if student.name.is_none() {
        old_fields.push("XM");
    }
    if student.major.is_none() {
        old_fields.push("ZYMC");
    }
    if student.class.is_none() {
        old_fields.push("schoolClass");
    }
    if old_fields.is_empty() {
        println!("      成功：旧学生字段仍存在。");
    } else {
        println!("      警告：缺少旧字段：{}", old_fields.join(", "));
    }

    let old_2020 = batches
        .iter()
        .filter(|batch| batch.name.contains("2020级"))
        .count();
    println!(
        "      旧定位条件“批次名称包含 2020级”：{}",
        if old_2020 > 0 {
            format!("仍能找到 {old_2020} 项。")
        } else {
            "已找不到；正常模式会自动选择当前可选批次。".to_string()
        }
    );
    let batch_info = choose_available_batch(&batches, &config.batch_keyword)?;
    println!("      当前检测批次：{}", batch_info.name);

    println!("[3/4] 检查必修课列表接口和旧字段……");
    let required_payload = api.get_class(&login, &batch_info.code, 0).await?;
    let required_rows = extract_class_rows(&required_payload)?;
    if required_rows.is_empty() {
        println!("      接口可访问，但返回 0 门课，无法抽样验证教学班字段。");
    } else {
        let first = required_payload
            .get("data")
            .and_then(Value::as_object)
            .and_then(|data| data.get("rows"))
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .cloned()
            .unwrap_or_default();
        let mut missing = missing_fields(&first, &["KCH", "KCM", "tcList"]);
        if let Some(tc_list) = first.get("tcList").and_then(Value::as_array) {
            if let Some(first_tc) = tc_list.first() {
                missing.extend(
                    missing_fields(first_tc, &["KXH", "JXBID", "secretVal"])
                        .into_iter()
                        .map(|field| format!("tcList[].{field}")),
                );
            } else if !missing.iter().any(|field| field == "tcList") {
                missing.push("tcList[]（列表为空，无法验证内部字段）".into());
            }
        } else if !missing.iter().any(|field| field == "tcList") {
            missing.push("tcList[]（列表为空，无法验证内部字段）".into());
        }
        if missing.is_empty() {
            println!("      成功：旧必修课定位字段仍存在。");
        } else {
            println!("      警告：缺少 {}", missing.join(", "));
        }
    }

    println!("[4/4] 检查选修课列表接口和旧字段……");
    let elective_payload = api.get_class(&login, &batch_info.code, 1).await?;
    let elective_rows = extract_class_rows(&elective_payload)?;
    if elective_rows.is_empty() {
        println!("      接口可访问，但返回 0 门课，无法抽样验证课程字段。");
    } else {
        let first = elective_payload
            .get("data")
            .and_then(Value::as_object)
            .and_then(|data| data.get("rows"))
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .cloned()
            .unwrap_or_default();
        let missing = missing_fields(&first, &["KCH", "KCM", "JXBID", "secretVal"]);
        if missing.is_empty() {
            println!("      成功：旧选修课定位字段仍存在。");
        } else {
            println!("      警告：缺少选修定位字段 {}", missing.join(", "));
        }
        let monitor_missing =
            missing_fields(&first, &["SFYX", "numberOfSelected", "classCapacity"]);
        if monitor_missing.is_empty() {
            println!("      成功：旧余量监控字段仍存在。");
        } else {
            println!(
                "      余量监控字段有变化或缺失：{}",
                monitor_missing.join(", ")
            );
        }
    }

    println!("\n检测完成：以上过程未调用 /clazz/add 或 /clazz/del。");
    Ok(())
}

/// 打印学生/批次信息，并返回选定的批次号。
fn show_msg(login: &LoginResult, preferred_keyword: &str) -> Result<String, XkError> {
    let (student, batches) = extract_student_and_batches(&login.payload)?;
    println!("姓名：{}", student.name.as_deref().unwrap_or("<字段缺失>"));
    println!("专业：{}", student.major.as_deref().unwrap_or("<字段缺失>"));
    println!("班级：{}", student.class.as_deref().unwrap_or("<字段缺失>"));
    for batch in &batches {
        println!(
            "选课批次：{}\t是否可选：{}",
            batch.name,
            if batch.can_select { "1" } else { "0" }
        );
    }
    let selected = choose_available_batch(&batches, preferred_keyword)?;
    println!("使用批次：{}", selected.name);
    Ok(selected.code)
}

fn choose_available_batch(batches: &[Batch], preferred_keyword: &str) -> Result<Batch, XkError> {
    let keyword = preferred_keyword.trim();
    if !keyword.is_empty()
        && let Some(batch) = batches
            .iter()
            .find(|batch| batch.can_select && batch.name.contains(keyword))
    {
        return Ok(batch.clone());
    }
    batches
        .iter()
        .find(|batch| batch.can_select && !batch.code.is_empty())
        .cloned()
        .ok_or_else(|| XkError::Config("没有找到当前可选且包含 code 的选课批次".into()))
}

fn find_classes(
    rows: &[CourseRow],
    requested: &[CourseTarget],
    category: u8,
) -> Vec<(CourseTarget, Option<CourseRow>)> {
    requested
        .iter()
        .map(|target| {
            let matched = if category == 1 {
                rows.iter().find(|row| row.kch == target.kch).cloned()
            } else {
                rows.iter()
                    .find(|row| row.kch == target.kch)
                    .and_then(|row| {
                        row.tc_list
                            .iter()
                            .find(|tc| Some(tc.kxh.as_str()) == target.kxh.as_deref())
                            .map(|tc| CourseRow {
                                kch: row.kch.clone(),
                                kcm: row.kcm.clone(),
                                jxbid: tc.jxbid.clone(),
                                secret_val: tc.secret_val.clone(),
                                tc_list: Vec::new(),
                                sfyx: None,
                                number_selected: None,
                                class_capacity: None,
                            })
                    })
            };
            (target.clone(), matched)
        })
        .collect()
}

fn missing_fields(value: &Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .filter(|key| value.get(**key).is_none())
        .map(|key| key.to_string())
        .collect()
}
