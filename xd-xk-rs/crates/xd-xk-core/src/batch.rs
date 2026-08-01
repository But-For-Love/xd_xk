//! 学生信息解析与选课批次匹配。
//!
//! 三种批次匹配异常文案必须与 Python 版逐字一致（设计文档 §2.1）：
//! ①没找到含关键字的批次；②匹配到但未开放；③匹配到但拿不到 code。

use serde_json::Value;

use crate::error::{truncate, AppError};

/// 学生基本信息。
#[derive(Debug, Clone)]
pub struct StudentInfo {
    pub xm: String,
    pub zymc: String,
    pub school_class: String,
}

/// 一个选课批次。
#[derive(Debug, Clone)]
pub struct Batch {
    pub name: String,
    pub code: String,
    pub can_select: bool,
}

/// 解析学生基本信息（登录响应 `/data/student`）。
pub fn student_info(data: &Value) -> Result<StudentInfo, AppError> {
    let s = data
        .pointer("/data/student")
        .and_then(|v| v.as_object())
        .ok_or_else(|| parse_student_err(data))?;
    let get = |k: &str| s.get(k).and_then(|v| v.as_str()).map(str::to_string);
    Ok(StudentInfo {
        xm: get("XM").ok_or_else(|| parse_student_err(data))?,
        zymc: get("ZYMC").ok_or_else(|| parse_student_err(data))?,
        school_class: get("schoolClass").ok_or_else(|| parse_student_err(data))?,
    })
}

/// 提取选课批次列表（登录响应 `/data/student/electiveBatchList`）。
pub fn batch_list(data: &Value) -> Result<Vec<Batch>, AppError> {
    let arr = data
        .pointer("/data/student/electiveBatchList")
        .and_then(|v| v.as_array())
        .ok_or_else(|| parse_student_err(data))?;
    let mut out = Vec::with_capacity(arr.len());
    for b in arr {
        out.push(Batch {
            name: b
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            code: b
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            can_select: b.get("canSelect").and_then(|v| v.as_str()) == Some("1"),
        });
    }
    Ok(out)
}

/// 展示学生信息并匹配选课批次，返回 `batch_code`。
///
/// 等价于 Python `show_msg`。`batch_name` 为空时按 Python 行为：
/// 空字符串子串匹配所有批次，返回**最后一个**可选批次的 code。
pub fn show_msg(data: &Value, batch_name: &str) -> Result<String, AppError> {
    let student = student_info(data)?;
    let batches = batch_list(data)?;
    if batches.is_empty() {
        return Err(AppError::Batch(
            "electiveBatchList 为空，没有可用的选课批次".into(),
        ));
    }

    tracing::info!("姓名：{}", student.xm);
    tracing::info!("专业：{}", student.zymc);
    tracing::info!("班级：{}", student.school_class);
    for b in &batches {
        let can = if b.can_select { "是" } else { "否" };
        tracing::info!("  选课批次：{}\t可选：{can}", b.name);
    }

    match_batch(&batches, batch_name)
}

/// 按名称关键字匹配批次，返回 `batch_code`。
///
/// 优先返回可选的批次；多批次命中时返回最后一个（与 Python 一致）。
pub fn match_batch(batches: &[Batch], keyword: &str) -> Result<String, AppError> {
    let mut matched_open: Vec<String> = Vec::new();
    let mut matched_closed: Vec<String> = Vec::new();
    let mut batch_code = String::new();

    for b in batches {
        if b.name.contains(keyword) {
            if b.can_select {
                batch_code = b.code.clone();
                matched_open.push(b.name.clone());
            } else {
                matched_closed.push(b.name.clone());
            }
        }
    }

    if !batch_code.is_empty() {
        return Ok(batch_code);
    }

    // ── 异常路径 ──
    if matched_open.is_empty() && matched_closed.is_empty() {
        let names: Vec<&str> = batches.iter().map(|b| b.name.as_str()).collect();
        return Err(AppError::Batch(format!(
            "未找到包含「{keyword}」的选课批次\n全部批次：{names:?}"
        )));
    }
    if !matched_closed.is_empty() {
        return Err(AppError::Batch(format!(
            "本轮选课暂未开始：{matched_closed:?}\n请等待开放后再试"
        )));
    }
    let avail: Vec<&str> = batches
        .iter()
        .filter(|b| b.can_select)
        .map(|b| b.name.as_str())
        .collect();
    let avail = if avail.is_empty() { vec!["无"] } else { avail };
    Err(AppError::Batch(format!(
        "匹配到批次但未获取到 code\n可选批次：{avail:?}"
    )))
}

fn parse_student_err(data: &Value) -> AppError {
    let detail = serde_json::to_string(data).unwrap_or_default();
    AppError::Batch(format!(
        "解析学生信息失败：字段缺失或类型错误\n完整响应：{}",
        truncate(&detail, 500)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn login_data(batches: Value) -> Value {
        json!({
            "code": 200,
            "data": {
                "token": "TOKEN1",
                "student": {
                    "XM": "张三",
                    "ZYMC": "计算机科学与技术",
                    "schoolClass": "CS2301",
                    "electiveBatchList": batches
                }
            }
        })
    }

    fn batch(name: &str, code: &str, can: &str) -> Value {
        json!({ "name": name, "code": code, "canSelect": can })
    }

    #[test]
    fn keyword_matches_open_batch() {
        let data = login_data(json!([
            batch("2025级春季", "B1", "0"),
            batch("2025级秋季", "B2", "1"),
        ]));
        let code = show_msg(&data, "2025级").unwrap();
        assert_eq!(code, "B2", "应选中可选批次");
    }

    #[test]
    fn empty_keyword_returns_last_open_batch() {
        // Python：空关键字子串匹配所有，返回最后一个可选批次
        let data = login_data(json!([
            batch("A期", "B1", "1"),
            batch("B期", "B2", "1"),
            batch("C期", "B3", "0"),
        ]));
        let code = show_msg(&data, "").unwrap();
        assert_eq!(code, "B2");
    }

    #[test]
    fn no_match_error() {
        let data = login_data(json!([batch("2025级春季", "B1", "1"),]));
        let err = show_msg(&data, "2026级").unwrap_err();
        assert!(err.to_string().contains("未找到包含「2026级」"), "{err}");
        assert!(err.to_string().contains("全部批次"), "{err}");
    }

    #[test]
    fn closed_only_error() {
        let data = login_data(json!([batch("2025级春季", "B1", "0"),]));
        let err = show_msg(&data, "2025级").unwrap_err();
        assert!(err.to_string().contains("本轮选课暂未开始"), "{err}");
    }

    #[test]
    fn empty_batch_list_error() {
        let data = login_data(json!([]));
        let err = show_msg(&data, "").unwrap_err();
        assert!(err.to_string().contains("electiveBatchList 为空"), "{err}");
    }

    #[test]
    fn missing_student_error() {
        let data = json!({ "code": 200, "data": { "token": "T" } });
        let err = student_info(&data).unwrap_err();
        assert!(err.to_string().contains("解析学生信息失败"), "{err}");
    }
}
