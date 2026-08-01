//! 课程数据结构（API 响应）与必修 tcList 匹配逻辑。
//!
//! 字段名保持服务端大小写：`KCH/KXH/KCM/JXBID/secretVal/SKJS/
//! numberOfSelected/classCapacity/SFYX/tcList`。

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// 课程类别。`Required`=必修(FANKC/TJKC)，`Elective`=选修(XGKC)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Required,
    Elective,
}

impl Category {
    /// 选课时传的 `clazzType`：必修 FANKC，选修 XGKC。
    pub fn add_clazz_type(&self) -> &'static str {
        match self {
            Category::Required => "FANKC",
            Category::Elective => "XGKC",
        }
    }

    /// 退课时传的 `clazzType`：必修 TJKC，选修 XGKC。
    pub fn dele_clazz_type(&self) -> &'static str {
        match self {
            Category::Required => "TJKC",
            Category::Elective => "XGKC",
        }
    }

    /// 退课是否带 `chooseVolunteer=1`：仅选修。
    pub fn dele_has_volunteer(&self) -> bool {
        matches!(self, Category::Elective)
    }

    /// 中文标签（选课池四列中的「类别」）。
    pub fn label(&self) -> &'static str {
        match self {
            Category::Required => "必修",
            Category::Elective => "选修",
        }
    }

    /// 从 Python 的整数类别（0=必修，1=选修）转换。
    pub fn from_index(i: usize) -> Option<Category> {
        match i {
            0 => Some(Category::Required),
            1 => Some(Category::Elective),
            _ => None,
        }
    }
}

/// 从 JSON 数字或字符串解析的非负整数（`numberOfSelected`/`classCapacity`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Num(pub u64);

impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        Ok(Num(match v {
            Value::Number(n) => n.as_u64().unwrap_or(0),
            Value::String(s) => s.trim().parse().unwrap_or(0),
            _ => 0,
        }))
    }
}

/// 课程行（API `rows` 元素）。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CourseRow {
    /// 课程号。
    #[serde(rename = "KCH", default)]
    pub kch: String,
    /// 课序号。
    #[serde(rename = "KXH", default)]
    pub kxh: String,
    /// 课程名。
    #[serde(rename = "KCM", default)]
    pub kcm: String,
    /// 教学班 ID —— add/del 的 `clazzId`。
    #[serde(rename = "JXBID", default)]
    pub jxbid: Option<String>,
    /// 选课密钥 —— add/del 的 `secretVal`。
    #[serde(rename = "secretVal", default)]
    pub secret_val: Option<String>,
    /// 授课教师。
    #[serde(rename = "SKJS", default)]
    pub skjs: Option<String>,
    /// 已选人数。
    #[serde(rename = "numberOfSelected", default)]
    pub number_selected: Option<Num>,
    /// 课容量。
    #[serde(rename = "classCapacity", default)]
    pub class_capacity: Option<Num>,
    /// 是否有余量（`"0"` = 有余量）。
    #[serde(rename = "SFYX", default)]
    pub sfyx: Option<String>,
    /// 必修课嵌套的可选子项（按 KXH 匹配真实操作对象）。
    #[serde(rename = "tcList", default)]
    pub tc_list: Vec<CourseRow>,
}

impl CourseRow {
    /// 以「已选 < 容量」为最终判据（`SFYX` 只是 check 模式的预筛）。
    pub fn has_capacity(&self) -> bool {
        let sel = self.number_selected.map(|n| n.0).unwrap_or(0);
        let cap = self.class_capacity.map(|n| n.0).unwrap_or(0);
        sel < cap
    }

    /// `SFYX == "0"` 表示预筛有余量。
    pub fn sfyx_available(&self) -> bool {
        self.sfyx.as_deref() == Some("0")
    }
}

/// 在课程列表中定位真实操作对象。
///
/// - 必修：在每门课的嵌套 `tcList` 中按 `KXH` 匹配子项；
/// - 选修：直接使用平铺行。
pub fn resolve_target<'a>(
    rows: &'a [CourseRow],
    category: Category,
    kch: &str,
    kxh: &str,
) -> Option<&'a CourseRow> {
    rows.iter().find_map(|course| {
        if course.kch != kch {
            return None;
        }
        if category == Category::Required {
            course.tc_list.iter().find(|sub| sub.kxh == kxh)
        } else {
            Some(course)
        }
    })
}

/// `/elective/clazz/list` 响应。
#[derive(Debug, Deserialize, Default)]
pub struct ClassListResp {
    #[serde(default)]
    pub code: Option<i64>,
    #[serde(default)]
    pub msg: Option<String>,
    #[serde(default)]
    pub data: Option<ClassListData>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ClassListData {
    #[serde(default)]
    pub rows: Vec<CourseRow>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kch: &str, kxh: &str) -> CourseRow {
        CourseRow {
            kch: kch.into(),
            kxh: kxh.into(),
            ..Default::default()
        }
    }

    #[test]
    fn required_resolves_tc_list_by_kxh() {
        let rows = vec![
            CourseRow {
                kch: "TE204003".into(),
                tc_list: vec![row("TE204003", "01"), row("TE204003", "02")],
                ..Default::default()
            },
            row("EY226022", "01"),
        ];
        let target = resolve_target(&rows, Category::Required, "TE204003", "02").unwrap();
        assert_eq!(target.kxh, "02");
        // 选修直接取平铺行
        let target = resolve_target(&rows, Category::Elective, "EY226022", "").unwrap();
        assert_eq!(target.kxh, "01");
        // 不存在的 KXH → None
        assert!(resolve_target(&rows, Category::Required, "TE204003", "99").is_none());
    }

    #[test]
    fn clazz_types_match_python() {
        assert_eq!(Category::Required.add_clazz_type(), "FANKC");
        assert_eq!(Category::Required.dele_clazz_type(), "TJKC");
        assert_eq!(Category::Elective.add_clazz_type(), "XGKC");
        assert_eq!(Category::Elective.dele_clazz_type(), "XGKC");
        assert!(!Category::Required.dele_has_volunteer());
        assert!(Category::Elective.dele_has_volunteer());
    }

    #[test]
    fn capacity_judgment_uses_selected_lt_capacity() {
        let mut c = CourseRow {
            sfyx: Some("1".into()), // 预筛说"无余量"，但容量判据为准
            number_selected: Some(Num(30)),
            class_capacity: Some(Num(60)),
            ..Default::default()
        };
        assert!(c.has_capacity());
        assert!(!c.sfyx_available());

        c.class_capacity = Some(Num(30));
        assert!(!c.has_capacity(), "已选=容量 → 无空位");
    }

    #[test]
    fn num_deserializes_from_number_and_string() {
        let v: Value = serde_json::json!({ "n": 5, "s": "3", "z": "abc" });
        assert_eq!(Num::deserialize(v.get("n").unwrap()).unwrap().0, 5);
        assert_eq!(Num::deserialize(v.get("s").unwrap()).unwrap().0, 3);
        assert_eq!(Num::deserialize(v.get("z").unwrap()).unwrap().0, 0);
    }
}
