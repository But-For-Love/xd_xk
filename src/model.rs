use serde_json::Value;

use crate::error::XkError;

/// 一个目标课程。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseTarget {
    pub kch: String,
    /// 必修课才有课序号；选修课为 None。
    pub kxh: Option<String>,
}

impl CourseTarget {
    pub fn required(kch: impl Into<String>, kxh: impl Into<String>) -> Self {
        Self {
            kch: kch.into(),
            kxh: Some(kxh.into()),
        }
    }

    pub fn elective(kch: impl Into<String>) -> Self {
        Self {
            kch: kch.into(),
            kxh: None,
        }
    }
}

/// 选课批次。
#[derive(Debug, Clone)]
pub struct Batch {
    pub code: String,
    pub name: String,
    pub can_select: bool,
}

/// 登录响应中的学生信息。
#[derive(Debug, Clone, Default)]
pub struct StudentInfo {
    pub name: Option<String>,
    pub major: Option<String>,
    pub class: Option<String>,
    pub batches: Vec<Batch>,
}

/// 课程列表中的教学班信息。
#[derive(Debug, Clone, Default)]
pub struct CourseRow {
    pub kch: String,
    pub kcm: Option<String>,
    pub jxbid: Option<String>,
    pub secret_val: Option<String>,
    pub tc_list: Vec<TeachingClass>,
    pub sfyx: Option<String>,
    pub number_selected: Option<String>,
    pub class_capacity: Option<String>,
}

/// 必修课父行 tcList 中的教学班。
#[derive(Debug, Clone)]
pub struct TeachingClass {
    pub kxh: String,
    pub jxbid: Option<String>,
    pub secret_val: Option<String>,
}

/// 把 JSON 值尽量转成字符串（兼容字符串/数字/布尔）。
pub(crate) fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// 从登录响应提取学生信息和选课批次。
pub fn extract_student_and_batches(payload: &Value) -> Result<(StudentInfo, Vec<Batch>), XkError> {
    let data = payload
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| XkError::Api {
            path: "/auth/login".into(),
            msg: "缺少 data 字段，接口可能已经改版".into(),
        })?;

    let student = data
        .get("student")
        .and_then(Value::as_object)
        .ok_or_else(|| XkError::Api {
            path: "/auth/login".into(),
            msg: "缺少 student 字段，接口可能已经改版".into(),
        })?;

    let batches = student
        .get("electiveBatchList")
        .and_then(Value::as_array)
        .ok_or_else(|| XkError::Api {
            path: "/auth/login".into(),
            msg: "缺少 electiveBatchList，接口可能已经改版".into(),
        })?;

    let info = StudentInfo {
        name: student.get("XM").and_then(value_to_string),
        major: student.get("ZYMC").and_then(value_to_string),
        class: student.get("schoolClass").and_then(value_to_string),
        batches: batches.iter().filter_map(parse_batch).collect(),
    };

    let batches = info.batches.clone();
    Ok((info, batches))
}

fn parse_batch(value: &Value) -> Option<Batch> {
    let obj = value.as_object()?;
    let code = obj.get("code").and_then(value_to_string)?;
    if code.is_empty() {
        return None;
    }
    let name = obj
        .get("name")
        .and_then(value_to_string)
        .unwrap_or_else(|| "<无名称>".into());
    let can_select = obj
        .get("canSelect")
        .and_then(value_to_string)
        .map(|s| s == "1")
        .unwrap_or(false);
    Some(Batch {
        code,
        name,
        can_select,
    })
}

/// 从课程列表响应提取 rows。
pub fn extract_class_rows(payload: &Value) -> Result<Vec<CourseRow>, XkError> {
    let data = payload
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| XkError::Api {
            path: "/elective/clazz/list".into(),
            msg: payload
                .get("msg")
                .and_then(Value::as_str)
                .unwrap_or("缺少 data 字段，接口可能已经改版")
                .to_string(),
        })?;

    let rows = data
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| XkError::Api {
            path: "/elective/clazz/list".into(),
            msg: "缺少 data.rows 数组，接口可能已经改版".into(),
        })?;

    Ok(rows.iter().filter_map(parse_course_row).collect())
}

fn parse_course_row(value: &Value) -> Option<CourseRow> {
    let obj = value.as_object()?;
    let kch = obj.get("KCH").and_then(value_to_string).unwrap_or_default();
    let tc_list = obj
        .get("tcList")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(parse_teaching_class).collect())
        .unwrap_or_default();

    Some(CourseRow {
        kch,
        kcm: obj.get("KCM").and_then(value_to_string),
        jxbid: obj.get("JXBID").and_then(value_to_string),
        secret_val: obj.get("secretVal").and_then(value_to_string),
        tc_list,
        sfyx: obj.get("SFYX").and_then(value_to_string),
        number_selected: obj.get("numberOfSelected").and_then(value_to_string),
        class_capacity: obj.get("classCapacity").and_then(value_to_string),
    })
}

fn parse_teaching_class(value: &Value) -> Option<TeachingClass> {
    let obj = value.as_object()?;
    let kxh = obj.get("KXH").and_then(value_to_string)?;
    Some(TeachingClass {
        kxh,
        jxbid: obj.get("JXBID").and_then(value_to_string),
        secret_val: obj.get("secretVal").and_then(value_to_string),
    })
}
