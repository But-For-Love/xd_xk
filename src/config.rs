use std::path::{Path, PathBuf};

use crate::error::XkError;
use crate::model::CourseTarget;
use crate::ui;

/// 与 Python 版一致的配置项。
#[derive(Debug, Clone)]
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
    pub required_courses: Vec<CourseTarget>,
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

/// 加载 `.env`；不存在时自动创建默认配置。
pub fn load_config(env_path: &Path, model_override: Option<&Path>) -> Result<Config, XkError> {
    let mut config = if env_path.exists() {
        dotenvy::from_path(env_path).map_err(|e| XkError::Config(format!(".env 无法读取：{e}")))?;

        Config {
            loginname: env_string("XK_LOGINNAME", ""),
            password: env_string("XK_PASSWORD", ""),
            ocr_captcha: env_string("XK_OCR_CAPTCHA", "1") == "1",
            debug: env_string("XK_DEBUG", "0") == "1",
            batch_keyword: env_string("XK_BATCH_KEYWORD", ""),
            campus: env_string("XK_CAMPUS", "S"),
            request_timeout: parse_max_f64("XK_REQUEST_TIMEOUT", 12.0, 1.0)?,
            request_interval: parse_max_f64("XK_REQUEST_INTERVAL", 1.0, 0.0)?,
            max_attempts: parse_max_u32("XK_MAX_ATTEMPTS", 0)?,
            category: parse_category()?,
            required_courses: parse_required_courses(&env_string("XK_REQUIRED_COURSES", ""))?,
            elective_courses: parse_elective_courses(&env_string("XK_ELECTIVE_COURSES", ""))?,
            ocr_model: PathBuf::from(env_string("XK_OCR_MODEL", "ddddocr.onnx")),
        }
    } else {
        let defaults = Config::default();
        save_config(env_path, &defaults)?;
        println!("未找到配置文件，已创建：{}", env_path.display());
        defaults
    };

    if let Some(model) = model_override {
        config.ocr_model = model.to_path_buf();
    }
    Ok(config)
}

/// 保存配置到 `.env`，格式与 Python 版兼容。
pub fn save_config(env_path: &Path, config: &Config) -> Result<(), XkError> {
    let required = config
        .required_courses
        .iter()
        .map(|c| format!("{}:{}", c.kch, c.kxh.as_deref().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join(",");
    let elective = config
        .elective_courses
        .iter()
        .map(|c| c.kch.clone())
        .collect::<Vec<_>>()
        .join(",");

    let entries = [
        ("XK_LOGINNAME", serde_json::to_string(&config.loginname)?),
        ("XK_PASSWORD", serde_json::to_string(&config.password)?),
        (
            "XK_OCR_CAPTCHA",
            serde_json::to_string(if config.ocr_captcha { "1" } else { "0" })?,
        ),
        (
            "XK_DEBUG",
            serde_json::to_string(if config.debug { "1" } else { "0" })?,
        ),
        (
            "XK_BATCH_KEYWORD",
            serde_json::to_string(&config.batch_keyword)?,
        ),
        ("XK_CAMPUS", serde_json::to_string(&config.campus)?),
        (
            "XK_REQUEST_TIMEOUT",
            serde_json::to_string(&config.request_timeout)?,
        ),
        (
            "XK_REQUEST_INTERVAL",
            serde_json::to_string(&config.request_interval)?,
        ),
        (
            "XK_MAX_ATTEMPTS",
            serde_json::to_string(&config.max_attempts)?,
        ),
        ("XK_CATEGORY", serde_json::to_string(&config.category)?),
        ("XK_REQUIRED_COURSES", serde_json::to_string(&required)?),
        ("XK_ELECTIVE_COURSES", serde_json::to_string(&elective)?),
        (
            "XK_OCR_MODEL",
            serde_json::to_string(&config.ocr_model.display().to_string())?,
        ),
    ];

    let content = entries
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    std::fs::write(env_path, content)?;
    Ok(())
}

/// 交互式编辑配置。
pub async fn edit_config(env_path: &Path, model_override: Option<&Path>) -> Result<(), XkError> {
    let mut config = load_config(env_path, model_override)?;
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

    save_config(env_path, &config)?;
    println!("全部配置已保存到 Git 忽略的 .env。");
    Ok(())
}

/// 只读展示当前配置（不显示密码明文）。
pub fn print_config(config: &Config) {
    println!("配置文件模型：{}", config.ocr_model.display());
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

fn env_string(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn parse_max_f64(key: &str, default: f64, min: f64) -> Result<f64, XkError> {
    let raw = env_string(key, &default.to_string());
    raw.parse::<f64>()
        .map(|v| v.max(min))
        .map_err(|_| XkError::Config(format!(".env 中的 {key} 格式错误")))
}

fn parse_max_u32(key: &str, default: u32) -> Result<u32, XkError> {
    let raw = env_string(key, &default.to_string());
    raw.parse::<u32>()
        .map_err(|_| XkError::Config(format!(".env 中的 {key} 格式错误")))
}

fn parse_category() -> Result<u8, XkError> {
    let raw = env_string("XK_CATEGORY", "0");
    match raw.as_str() {
        "0" => Ok(0),
        "1" => Ok(1),
        _ => Err(XkError::Config(
            ".env 中的 XK_CATEGORY 只能是 0 或 1".into(),
        )),
    }
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
    use super::{Config, parse_course_list};

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
}
