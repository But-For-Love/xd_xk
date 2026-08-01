//! 配置读写与旧 conf.json 自动迁移。
//!
//! - `config.toml`：凭据 + 应用设置，真布尔（不再是 `"0"/"1"` 字符串）。
//! - 用 `toml_edit` 做 round-trip 读写，保留手写注释与格式。
//! - 首次运行检测旧 `conf.json`，自动迁移出 `config.toml` + `courses.csv`，
//!   原文件保留不动。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml_edit::{value, DocumentMut, Item, Table};

use crate::course::Category;
use crate::courses::CourseEntry;
use crate::error::{parse_err, AppError};

/// 旧版 Python 配置文件名（迁移来源，保留不动）。
pub const LEGACY_CONF_PATH: &str = "conf.json";
/// Rust 版配置文件默认路径。
pub const CONFIG_PATH: &str = "config.toml";
/// 课程文件默认路径。
pub const COURSES_PATH: &str = "courses.csv";

/// 应用设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 自动识别验证码；false = 手动输入。
    #[serde(default)]
    pub ocr_captcha: bool,
    /// 把接口响应 dump 成文件，排查用。
    #[serde(default)]
    pub debug: bool,
    /// 选课批次关键字，留空 = 自动匹配。
    #[serde(default)]
    pub batch_name: String,
}

/// 账号凭据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    /// 学号；可用环境变量 XD_XK_USERNAME 覆盖。
    #[serde(default)]
    pub loginname: String,
    /// 密码；可用 XD_XK_PASSWORD 覆盖。
    #[serde(default)]
    pub password: String,
}

/// 完整配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub app: AppConfig,
    pub account: AccountConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app: AppConfig {
                ocr_captcha: true,
                debug: false,
                batch_name: String::new(),
            },
            account: AccountConfig {
                loginname: String::new(),
                password: String::new(),
            },
        }
    }
}

/// 迁移摘要。
#[derive(Debug, Clone)]
pub struct MigrateSummary {
    /// 是否实际执行了迁移（false = 已存在新配置或没有旧配置）。
    pub migrated: bool,
    /// 生成 / 已存在的 config.toml 路径。
    pub config_path: PathBuf,
    /// 生成 / 已存在的课程文件路径。
    pub courses_path: PathBuf,
    /// 从 conf.json 迁移出来的课程数量。
    pub course_count: usize,
}

/// 带注释的初始模板（与 `docs/examples/config.toml` 一致）。
fn template_doc() -> DocumentMut {
    let text = "# ============================================================\n\
# xd-xk 配置文件（Rust 版）
# ------------------------------------------------------------
# 说明：
#   - 凭据与应用设置放这里；课程数据单独放 courses.csv。
#   - 首次运行会自动从旧 conf.json 迁移生成本文件，无需手写。
#   - 布尔值是真正的 true/false（不再是 \"0\"/\"1\" 字符串）。
#   - 手动修改后注意保留注释，保存不会覆盖它们。
# ============================================================

[app]
ocr_captcha = true      # true = 自动识别验证码；false = 手动输入
debug = false           # true = 把接口响应 dump 成文件，排查用
batch_name = \"\"         # 选课批次关键字，留空 = 自动匹配

[account]
loginname = \"\"          # 学号；也可用环境变量 XD_XK_USERNAME 覆盖
password = \"\"           # 密码；可用 XD_XK_PASSWORD 覆盖，不建议明文提交到 git
";
    text.parse().expect("内置模板配置应可解析")
}

/// 确保 `key` 是表，返回其可变引用（缺失则创建）。
fn table_mut<'a>(doc: &'a mut DocumentMut, key: &str) -> &'a mut Table {
    if !doc.as_table().contains_key(key) {
        doc[key] = Item::Table(Table::new());
    }
    doc[key].as_table_mut().expect("已确保 key 是表")
}

/// 更新键值，同时保留该键原有的前后注释（round-trip 关键）。
fn set_value(table: &mut Table, key: &str, new_item: Item) {
    let decor = table
        .get(key)
        .and_then(|i| i.as_value())
        .map(|v| v.decor().clone())
        .unwrap_or_default();
    let mut item = new_item;
    if let Some(v) = item.as_value_mut() {
        *v.decor_mut() = decor;
    }
    table.insert(key, item);
}

/// 将 Config 的字段写入 DocumentMut（保留已有注释与无关内容）。
fn apply_config(doc: &mut DocumentMut, conf: &Config) {
    let app = table_mut(doc, "app");
    set_value(app, "ocr_captcha", value(conf.app.ocr_captcha));
    set_value(app, "debug", value(conf.app.debug));
    set_value(app, "batch_name", value(conf.app.batch_name.as_str()));

    let account = table_mut(doc, "account");
    set_value(account, "loginname", value(conf.account.loginname.as_str()));
    set_value(account, "password", value(conf.account.password.as_str()));
}

/// 从文件加载配置（缺失文件视为默认配置）。
pub fn load(path: &Path) -> Result<Config, AppError> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(path)
        .map_err(|e| AppError::Io(format!("读取配置文件 {} 失败：{e}", path.display())))?;
    parse(&text, path)
}

/// 从字符串解析配置。
pub fn parse(text: &str, path: &Path) -> Result<Config, AppError> {
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|e| parse_err(&format!("配置文件 {} 解析失败", path.display()), e))?;
    let de = toml_edit::de::Deserializer::from(doc);
    Config::deserialize(de)
        .map_err(|e| parse_err(&format!("配置文件 {} 格式错误", path.display()), e))
}

/// 保存配置到文件（round-trip：保留已有注释；不存在则生成带注释的模板）。
pub fn save(path: &Path, conf: &Config) -> Result<(), AppError> {
    let mut doc = if path.exists() {
        let text = fs::read_to_string(path)
            .map_err(|e| AppError::Io(format!("读取配置文件 {} 失败：{e}", path.display())))?;
        text.parse::<DocumentMut>().map_err(|e| {
            parse_err(
                &format!("配置文件 {} 解析失败（未覆盖，请先修正）", path.display()),
                e,
            )
        })?
    } else {
        template_doc()
    };
    apply_config(&mut doc, conf);
    fs::write(path, doc.to_string())
        .map_err(|e| AppError::Io(format!("写入配置文件 {} 失败：{e}", path.display())))?;
    Ok(())
}

/// 从旧 conf.json 迁移。仅当 config.toml 不存在且 conf.json 存在时执行。
///
/// - `bx` → 必修行，`xx` → 选修行（空 KCH 的条目跳过，与 Python 一致）；
/// - 布尔 `"0"/"1"` 字符串 → 真布尔；
/// - `data.loginname / data.password` → `[account]`；
/// - 原文件保留不动。
pub fn migrate_if_needed(
    config_path: &Path,
    courses_path: &Path,
    legacy_conf: &Path,
) -> Result<MigrateSummary, AppError> {
    let summary = MigrateSummary {
        migrated: false,
        config_path: config_path.to_path_buf(),
        courses_path: courses_path.to_path_buf(),
        course_count: 0,
    };
    if config_path.exists() {
        return Ok(summary);
    }
    if !legacy_conf.exists() {
        return Ok(summary);
    }

    let text = fs::read_to_string(legacy_conf)
        .map_err(|e| AppError::Io(format!("读取旧配置 {} 失败：{e}", legacy_conf.display())))?;
    let old: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        parse_err(
            &format!("旧配置 {} 不是合法 JSON", legacy_conf.display()),
            e,
        )
    })?;

    let str_at = |v: &serde_json::Value, path: &[&str]| -> String {
        let pointer = format!("/{}", path.join("/"));
        v.pointer(&pointer)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string()
    };

    let conf = Config {
        app: AppConfig {
            ocr_captcha: old.get("ocr_captcha").and_then(|v| v.as_str()) == Some("1"),
            debug: old.get("debug").and_then(|v| v.as_str()) == Some("1"),
            batch_name: str_at(&old, &["batch_name"]),
        },
        account: AccountConfig {
            loginname: str_at(&old, &["data", "loginname"]),
            password: str_at(&old, &["data", "password"]),
        },
    };
    save(config_path, &conf)?;

    let mut entries: Vec<CourseEntry> = Vec::new();
    entries.extend(old_to_entries(&old, "bx", Category::Required));
    entries.extend(old_to_entries(&old, "xx", Category::Elective));
    crate::courses::write_csv(courses_path, &entries)?;

    Ok(MigrateSummary {
        migrated: true,
        config_path: config_path.to_path_buf(),
        courses_path: courses_path.to_path_buf(),
        course_count: entries.len(),
    })
}

/// 提取旧 conf.json 中某分类的课程列表。
fn old_to_entries(old: &serde_json::Value, key: &str, category: Category) -> Vec<CourseEntry> {
    let mut out = Vec::new();
    let Some(arr) = old.get(key).and_then(|v| v.as_array()) else {
        return out;
    };
    for c in arr {
        let kch = c
            .get("KCH")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if kch.is_empty() {
            continue; // 与 Python `_courses_from_conf` 的 `if c.get("KCH")` 一致
        }
        let kxh = c
            .get("KXH")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let kcm = c
            .get("KCM")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.push(CourseEntry {
            category,
            kch,
            kxh,
            kcm,
        });
    }
    out
}

/// 环境变量覆盖账号（CLI 参数 > 环境变量 > 配置文件）。
pub fn apply_env_overrides(conf: &mut Config, map: &HashMap<String, String>) {
    if let Some(v) = map.get("XD_XK_USERNAME").filter(|s| !s.is_empty()) {
        conf.account.loginname = v.clone();
    }
    if let Some(v) = map.get("XD_XK_PASSWORD").filter(|s| !s.is_empty()) {
        conf.account.password = v.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bool_and_sections() {
        let text = r#"[app]
ocr_captcha = true
debug = false
batch_name = "2025级"

[account]
loginname = "12345"
password = "secret"
"#;
        let conf = parse(text, Path::new("test.toml")).unwrap();
        assert!(conf.app.ocr_captcha);
        assert!(!conf.app.debug);
        assert_eq!(conf.app.batch_name, "2025级");
        assert_eq!(conf.account.loginname, "12345");
    }

    #[test]
    fn save_roundtrip_preserves_comments_and_format() {
        let dir = std::env::temp_dir().join(format!("xd-xk-cfg-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        // 首次写入：应生成带注释的模板
        let mut conf = Config::default();
        conf.app.ocr_captcha = false;
        conf.account.loginname = "2018000000".into();
        save(&path, &conf).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("# 学号；也可用环境变量"),
            "模板注释应保留: {text}"
        );
        assert!(text.contains("ocr_captcha = false"));

        // 二次写入：注释应被保留
        conf.account.password = "newpass".into();
        save(&path, &conf).unwrap();
        let text2 = fs::read_to_string(&path).unwrap();
        assert!(
            text2.contains("# 学号；也可用环境变量"),
            "round-trip 应保留注释"
        );
        assert!(text2.contains("password = \"newpass\""));
        assert_eq!(text2.lines().count(), text.lines().count(), "行数不应变");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrate_from_old_conf_json() {
        let dir = std::env::temp_dir().join(format!("xd-xk-mig-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let legacy = dir.join("conf.json");
        let config_path = dir.join("config.toml");
        let courses_path = dir.join("courses.csv");

        fs::write(
            &legacy,
            r#"{
  "ocr_captcha": "1",
  "debug": "0",
  "batch_name": "2025级",
  "bx": [{ "KCH": "TE204003", "KXH": "02", "KCM": "大学物理" }, { "KCH": "", "KXH": "", "KCM": "" }],
  "xx": [{ "KCH": "EY226022", "KXH": "01", "KCM": "操作系统" }],
  "data": { "loginname": "2018000001", "password": "pw123", "captcha": "x", "uuid": "u" }
}"#,
        )
        .unwrap();

        let sum = migrate_if_needed(&config_path, &courses_path, &legacy).unwrap();
        assert!(sum.migrated, "应执行迁移");
        assert_eq!(sum.course_count, 2, "空 KCH 条目应被跳过");

        let conf = load(&config_path).unwrap();
        assert!(conf.app.ocr_captcha, "字符串 \"1\" → 真布尔 true");
        assert!(!conf.app.debug);
        assert_eq!(conf.app.batch_name, "2025级");
        assert_eq!(conf.account.loginname, "2018000001");
        assert_eq!(conf.account.password, "pw123");

        let courses = crate::courses::read_entries(&courses_path).unwrap();
        assert_eq!(courses.len(), 2);
        assert_eq!(courses[0].category, Category::Required);
        assert_eq!(courses[0].kch, "TE204003");
        assert_eq!(courses[1].category, Category::Elective);
        assert_eq!(courses[1].kcm, "操作系统");

        // 原文件保留不动
        assert!(legacy.exists());

        // 二次调用：已存在 config.toml，不再迁移
        let sum2 = migrate_if_needed(&config_path, &courses_path, &legacy).unwrap();
        assert!(!sum2.migrated);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_overrides_apply() {
        let mut conf = Config::default();
        let map = HashMap::from([
            ("XD_XK_USERNAME".to_string(), "envuser".to_string()),
            ("XD_XK_PASSWORD".to_string(), "envpass".to_string()),
        ]);
        apply_env_overrides(&mut conf, &map);
        assert_eq!(conf.account.loginname, "envuser");
        assert_eq!(conf.account.password, "envpass");
    }
}
