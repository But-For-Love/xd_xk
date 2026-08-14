use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::XkError;
use crate::model::CourseTarget;
use crate::ui;

/// 与 Python 版一致的配置项，使用 TOML 格式保存。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub loginname: String,
    pub password: String,
    pub ocr_captcha: bool,
    pub debug: bool,
    pub batch_keyword: String,
    pub campus: String,
    pub request_timeout: f64,
    pub request_interval: f64,
    pub max_attempts: u32,
    pub category: u8,
    /// 必修课：TOML 里是 [[required_courses]] 数组表（kch + kxh）。
    #[serde(with = "course_targets::required")]
    pub required_courses: Vec<CourseTarget>,
    /// 选修课：TOML 里是课程号字符串数组。
    #[serde(with = "course_targets::elective")]
    pub elective_courses: Vec<CourseTarget>,
    pub ocr_model: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            loginname: String::new(),
            password: String::new(),
            ocr_captcha: true,
            debug: false,
            batch_keyword: String::new(),
            campus: "S".into(),
            request_timeout: 12.0,
            request_interval: 1.0,
            max_attempts: 0,
            category: 0,
            required_courses: Vec::new(),
            elective_courses: Vec::new(),
            ocr_model: PathBuf::from("ddddocr.onnx"),
        }
    }
}

/// 课程列表在 TOML 中的两种表示：
/// 必修课是 [[required_courses]]（kch/kxh 数组表），选修课是字符串数组。
mod course_targets {
    use serde::{Deserialize, Serialize};

    use super::CourseTarget;

    #[derive(Serialize)]
    struct RequiredOut<'a> {
        kch: &'a str,
        kxh: &'a str,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RequiredIn {
        kch: String,
        kxh: String,
    }

    pub mod required {
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        use super::{CourseTarget, RequiredIn, RequiredOut};

        pub fn serialize<S>(courses: &[CourseTarget], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let entries: Vec<RequiredOut<'_>> = courses
                .iter()
                .map(|course| RequiredOut {
                    kch: &course.kch,
                    kxh: course.kxh.as_deref().unwrap_or_default(),
                })
                .collect();
            entries.serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<CourseTarget>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let entries = Vec::<RequiredIn>::deserialize(deserializer)?;
            Ok(entries
                .into_iter()
                .map(|entry| CourseTarget::required(entry.kch, entry.kxh))
                .collect())
        }
    }

    pub mod elective {
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        use super::CourseTarget;

        pub fn serialize<S>(courses: &[CourseTarget], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let codes: Vec<&str> = courses.iter().map(|course| course.kch.as_str()).collect();
            codes.serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<CourseTarget>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let codes = Vec::<String>::deserialize(deserializer)?;
            Ok(codes.into_iter().map(CourseTarget::elective).collect())
        }
    }
}

/// 加载 TOML 配置；文件不存在时自动创建默认配置。
/// 兼容迁移：同目录的旧版 .env 会自动转换为 TOML。
pub fn load_config(config_path: &Path, model_override: Option<&Path>) -> Result<Config, XkError> {
    let mut config = if config_path.exists() {
        let text = std::fs::read_to_string(config_path).map_err(|error| {
            XkError::Config(format!("{} 无法读取：{error}", config_path.display()))
        })?;
        toml::from_str(&text).map_err(|error| {
            XkError::Config(format!("{} 不是有效的 TOML：{error}", config_path.display()))
        })?
    } else if let Some(config) = migrate_legacy_env(config_path)? {
        save_config(config_path, &config)?;
        println!("检测到旧版 .env，已自动迁移到：{}", config_path.display());
        println!(
            "旧文件 {} 不再使用，可自行删除。",
            config_path.with_file_name(".env").display()
        );
        config
    } else {
        let defaults = Config::default();
        save_config(config_path, &defaults)?;
        println!("未找到配置文件，已创建：{}", config_path.display());
        defaults
    };

    if let Some(model) = model_override {
        config.ocr_model = model.to_path_buf();
    }
    Ok(config)
}

/// 同目录存在旧版 .env 时读取并转换为 Config；不存在返回 Ok(None)。
fn migrate_legacy_env(config_path: &Path) -> Result<Option<Config>, XkError> {
    let legacy_path = config_path.with_file_name(".env");
    if !legacy_path.exists() {
        return Ok(None);
    }

    let text = std::fs::read_to_string(&legacy_path)
        .map_err(|error| XkError::Config(format!(".env 迁移失败，无法读取：{error}")))?;

    let mut values = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = value.trim();
        // 旧版 save_config 用 serde_json 写值，这里解掉 JSON 引号；手写裸值则原样保留。
        let value = serde_json::from_str::<String>(value).unwrap_or_else(|_| value.to_string());
        values.insert(key, value);
    }
    let get = |key: &str, default: &str| -> String {
        values.get(key).cloned().unwrap_or_else(|| default.to_string())
    };

    let mut config = Config::default();
    config.loginname = get("XK_LOGINNAME", "");
    config.password = get("XK_PASSWORD", "");
    config.ocr_captcha = get("XK_OCR_CAPTCHA", "1") == "1";
    config.debug = get("XK_DEBUG", "0") == "1";
    config.batch_keyword = get("XK_BATCH_KEYWORD", "");
    config.campus = get("XK_CAMPUS", "S");
    config.request_timeout = get("XK_REQUEST_TIMEOUT", "12")
        .parse::<f64>()
        .unwrap_or(12.0)
        .max(1.0);
    config.request_interval = get("XK_REQUEST_INTERVAL", "1")
        .parse::<f64>()
        .unwrap_or(1.0)
        .max(0.0);
    config.max_attempts = get("XK_MAX_ATTEMPTS", "0").parse().unwrap_or(0);
    config.category = if get("XK_CATEGORY", "0") == "1" { 1 } else { 0 };
    config.required_courses = parse_required_courses(&get("XK_REQUIRED_COURSES", ""))?;
    config.elective_courses = parse_elective_courses(&get("XK_ELECTIVE_COURSES", ""))?;
    config.ocr_model = PathBuf::from(get("XK_OCR_MODEL", "ddddocr.onnx"));
    Ok(Some(config))
}

/// 保存配置到 TOML 文件。
pub fn save_config(config_path: &Path, config: &Config) -> Result<(), XkError> {
    if let Some(parent) = config_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(config)
        .map_err(|error| XkError::Config(format!("配置序列化失败：{error}")))?;
    std::fs::write(config_path, text + "\n")?;
    Ok(())
}

/// 交互式编辑配置。
pub async fn edit_config(config_path: &Path, model_override: Option<&Path>) -> Result<(), XkError> {
    let mut config = load_config(config_path, model_override)?;
    println!("\n配置编辑模式：直接回车保留原值，输入 - 可清空原值。");

    config.loginname = prompt("学号", &config.loginname).await?;
    config.password = prompt("密码", &config.password).await?;
    config.ocr_captcha = prompt_bool("自动识别验证码（1/0）", config.ocr_captcha).await?;
    config.debug = prompt_bool("保存调试响应（1/0）", config.debug).await?;
    config.batch_keyword = prompt(
        "批次名称关键字（如 2026级；留空自动选择首个可选批次）",
        &config.batch_keyword,
    )
    .await?;
    config.campus = prompt("校区代码", &config.campus).await?;

    let category_text = prompt("课程类别（0 必修 / 1 选修）", &config.category.to_string()).await?;
    if !matches!(category_text.as_str(), "0" | "1") {
        return Err(XkError::Config("课程类别只能是 0 或 1".into()));
    }
    config.category = category_text.parse().unwrap_or(0);

    let required_shown = config
        .required_courses
        .iter()
        .map(|c| format!("{}:{}", c.kch, c.kxh.as_deref().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join(", ");
    let required_input = prompt(
        &format!("必修课（课程号:课序号，逗号分隔）[{}]", required_shown),
        &required_shown,
    )
    .await?;
    config.required_courses = if required_input.trim() == "-" {
        Vec::new()
    } else {
        parse_required_courses(&required_input)?
    };

    let elective_shown = config
        .elective_courses
        .iter()
        .map(|c| c.kch.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let elective_input = prompt(
        &format!("选修课（课程号，逗号分隔）[{}]", elective_shown),
        &elective_shown,
    )
    .await?;
    config.elective_courses = if elective_input.trim() == "-" {
        Vec::new()
    } else {
        parse_elective_courses(&elective_input)?
    };

    let timeout = prompt("网络超时秒数", &config.request_timeout.to_string()).await?;
    let interval = prompt("重复请求间隔秒数", &config.request_interval.to_string()).await?;
    let attempts = prompt(
        "单课最大尝试次数（0 表示不限）",
        &config.max_attempts.to_string(),
    )
    .await?;

    config.request_timeout = timeout
        .parse::<f64>()
        .map_err(|_| XkError::Config("超时秒数不是有效数字".into()))?
        .max(1.0);
    config.request_interval = interval
        .parse::<f64>()
        .map_err(|_| XkError::Config("间隔秒数不是有效数字".into()))?
        .max(0.0);
    config.max_attempts = attempts
        .parse::<u32>()
        .map_err(|_| XkError::Config("尝试次数不是有效数字".into()))?;

    save_config(config_path, &config)?;
    println!("全部配置已保存到 {}。", config_path.display());
    Ok(())
}

/// 只读展示当前配置（不显示密码明文）。
pub fn print_config(config: &Config) {
    println!("OCR 模型路径：{}", config.ocr_model.display());
    println!("学号：{}", config.loginname);
    println!(
        "密码：{}",
        if config.password.is_empty() {
            "空"
        } else {
            "已设置"
        }
    );
    println!(
        "自动识别验证码：{}",
        if config.ocr_captcha { "1" } else { "0" }
    );
    println!("保存调试响应：{}", if config.debug { "1" } else { "0" });
    println!("批次名称关键字：{}", config.batch_keyword);
    println!("校区代码：{}", config.campus);
    println!("课程类别：{}", config.category);
    println!("网络超时秒数：{}", config.request_timeout);
    println!("重复请求间隔秒数：{}", config.request_interval);
    println!("单课最大尝试次数：{}", config.max_attempts);
    println!(
        "必修课：{}",
        config
            .required_courses
            .iter()
            .map(|c| format!("{}:{}", c.kch, c.kxh.as_deref().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "选修课：{}",
        config
            .elective_courses
            .iter()
            .map(|c| c.kch.clone())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

pub fn parse_course_list(text: &str, category: u8) -> Result<Vec<CourseTarget>, XkError> {
    if category == 0 {
        parse_required_courses(text)
    } else {
        parse_elective_courses(text)
    }
}

fn parse_required_courses(text: &str) -> Result<Vec<CourseTarget>, XkError> {
    let mut courses = Vec::new();
    for item in text.replace('，', ",").split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let normalized = item.replace('：', ":");
        let mut parts = normalized.splitn(2, ':');
        let kch = parts.next().unwrap_or_default().trim().to_string();
        let kxh = parts.next().unwrap_or_default().trim().to_string();
        if kch.is_empty() || kxh.is_empty() {
            return Err(XkError::Config(format!(
                "必修课格式错误：{item}；应为 课程号:课序号"
            )));
        }
        courses.push(CourseTarget::required(kch, kxh));
    }
    Ok(courses)
}

fn parse_elective_courses(text: &str) -> Result<Vec<CourseTarget>, XkError> {
    Ok(text
        .replace('，', ",")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| CourseTarget::elective(s.to_string()))
        .collect())
}

async fn prompt(label: &str, current: &str) -> Result<String, XkError> {
    let value = ui::read_line(&format!("{label} [{current}]（回车保留，输入 - 清空）：")).await?;
    if value.is_empty() {
        Ok(current.to_string())
    } else if value == "-" {
        Ok(String::new())
    } else {
        Ok(value.to_string())
    }
}

async fn prompt_bool(label: &str, current: bool) -> Result<bool, XkError> {
    let text = prompt(label, if current { "1" } else { "0" }).await?;
    match text.as_str() {
        "1" => Ok(true),
        "0" => Ok(false),
        _ => Err(XkError::Config(format!("{label} 只能是 1 或 0"))),
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, CourseTarget, parse_course_list};

    #[test]
    fn parses_course_lists() {
        let required = parse_course_list("TE204004:06，TE204004:07", 0).unwrap();
        assert_eq!(required.len(), 2);
        assert_eq!(required[0].kxh.as_deref(), Some("06"));

        let elective = parse_course_list("FL006066,FL006121", 1).unwrap();
        assert_eq!(elective.len(), 2);
        assert_eq!(elective[1].kch, "FL006121");
    }

    #[test]
    fn defaults_are_safe() {
        let config = Config::default();
        assert!(config.ocr_captcha);
        assert_eq!(config.campus, "S");
    }

    #[test]
    fn toml_roundtrip_keeps_courses() {
        let mut config = Config::default();
        config.campus = "N".into();
        config.required_courses = vec![
            CourseTarget::required("TE204004", "06"),
            CourseTarget::required("TE204004", "07"),
        ];
        config.elective_courses = vec![CourseTarget::elective("FL006066")];

        let text = toml::to_string_pretty(&config).unwrap();
        let loaded: Config = toml::from_str(&text).unwrap();
        assert_eq!(loaded.required_courses, config.required_courses);
        assert_eq!(loaded.elective_courses, config.elective_courses);
        assert_eq!(loaded.campus, "N");
    }

    #[test]
    fn parses_course_tables_from_toml() {
        let text = r#"
            ocr_captcha = false
            elective_courses = ["FL006066", "FL006121"]

            [[required_courses]]
            kch = "TE204004"
            kxh = "06"
        "#;
        let config: Config = toml::from_str(text).unwrap();
        assert!(!config.ocr_captcha);
        assert_eq!(config.required_courses[0].kxh.as_deref(), Some("06"));
        assert_eq!(config.elective_courses.len(), 2);
        assert_eq!(config.elective_courses[1].kch, "FL006121");
        // 未出现的字段取默认值
        assert_eq!(config.campus, "S");
    }
}
