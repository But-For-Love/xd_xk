//! `conf` 子命令：init / migrate / show / template / import / export。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use xd_xk_core::config::{self, Config};
use xd_xk_core::course::Category;
use xd_xk_core::courses::{read_entries, write_entries, CourseEntry};
use xd_xk_core::error::AppError;

use crate::args::{Cli, ConfArgs, ConfCommand, ConfInitArgs};
use crate::runner::{legacy_conf_path, load_config};

pub(crate) async fn cmd_conf(
    cli: &Cli,
    env: &HashMap<String, String>,
    args: &ConfArgs,
) -> Result<i32, AppError> {
    match &args.command {
        ConfCommand::Init(ia) => cmd_init(cli, env, ia),
        ConfCommand::Migrate => cmd_migrate(cli),
        ConfCommand::Show => cmd_show(cli, env),
        ConfCommand::Template { xlsx } => cmd_template(cli, *xlsx),
        ConfCommand::Import { file } => cmd_import(cli, file),
        ConfCommand::Export { file } => cmd_export(cli, file),
    }?;
    Ok(0)
}

/// `conf init`：生成/更新 config.toml。优先级 CLI 参数 > 环境变量 > 交互式输入。
fn cmd_init(cli: &Cli, env: &HashMap<String, String>, ia: &ConfInitArgs) -> Result<(), AppError> {
    let mut cfg = if cli.config.exists() {
        load_config(cli, env)?
    } else {
        Config::default()
    };

    let loginname = ia
        .loginname
        .clone()
        .or_else(|| env.get("XD_XK_USERNAME").cloned().filter(|s| !s.is_empty()));
    let password = ia
        .password
        .clone()
        .or_else(|| env.get("XD_XK_PASSWORD").cloned().filter(|s| !s.is_empty()));

    let loginname = match loginname {
        Some(v) => v,
        None => dialoguer::Input::new()
            .with_prompt("学号")
            .allow_empty(true)
            .interact_text()
            .map_err(|e| AppError::Config(format!("读取输入失败：{e}")))?,
    };
    let password = match password {
        Some(v) => v,
        None => dialoguer::Password::new()
            .with_prompt("密码")
            .allow_empty_password(true)
            .interact()
            .map_err(|e| AppError::Config(format!("读取输入失败：{e}")))?,
    };

    cfg.account.loginname = loginname;
    cfg.account.password = password;
    config::save(&cli.config, &cfg)?;
    println!("已生成配置文件：{}", cli.config.display());
    Ok(())
}

/// `conf migrate`：旧 conf.json → config.toml + courses.csv。
fn cmd_migrate(cli: &Cli) -> Result<(), AppError> {
    let summary =
        config::migrate_if_needed(&cli.config, &cli.courses_path(), &legacy_conf_path(cli))?;
    if summary.migrated {
        println!("迁移完成：");
        println!("  配置：{}", summary.config_path.display());
        println!(
            "  课程：{}（{} 门）",
            summary.courses_path.display(),
            summary.course_count
        );
        println!("原 conf.json 已保留不动");
    } else if cli.config.exists() {
        println!("未迁移：{} 已存在", cli.config.display());
    } else {
        println!("未迁移：未找到 {}，无需迁移", config::LEGACY_CONF_PATH);
    }
    Ok(())
}

/// `conf show`：打印当前配置（密码打码）。
fn cmd_show(cli: &Cli, env: &HashMap<String, String>) -> Result<(), AppError> {
    let cfg = load_config(cli, env)?;
    let mask = |s: &str| -> String {
        if s.is_empty() {
            "(未设置)".into()
        } else {
            "********".into()
        }
    };
    if cli.json {
        println!(
            "{}",
            serde_json::json!({
                "ocr_captcha": cfg.app.ocr_captcha,
                "debug": cfg.app.debug,
                "batch_name": cfg.app.batch_name,
                "loginname": cfg.account.loginname,
                "password": mask(&cfg.account.password),
            })
        );
    } else {
        println!("配置文件：{}", cli.config.display());
        println!(
            "自动验证码：{}",
            if cfg.app.ocr_captcha { "开" } else { "关" }
        );
        println!("调试输出：{}", if cfg.app.debug { "开" } else { "关" });
        println!(
            "批次关键字：{}",
            if cfg.app.batch_name.is_empty() {
                "(留空 = 自动匹配)".to_string()
            } else {
                cfg.app.batch_name.clone()
            }
        );
        println!("学号：{}", cfg.account.loginname);
        println!("密码：{}", mask(&cfg.account.password));
    }
    Ok(())
}

/// `conf template`：生成带示例行的课程文件模板（默认 CSV，--xlsx 生成 XLSX）。
fn cmd_template(cli: &Cli, xlsx: bool) -> Result<(), AppError> {
    let path = if xlsx {
        cli.courses.clone().unwrap_or_else(|| "courses.xlsx".into())
    } else {
        cli.courses_path()
    };
    if path.exists() {
        return Err(AppError::Config(format!(
            "课程文件 {} 已存在，未覆盖。如需其他位置，用 `--courses <文件>` 指定",
            path.display()
        )));
    }
    let sample = vec![
        CourseEntry {
            category: Category::Elective,
            kch: "EY226022".into(),
            kxh: "01".into(),
            kcm: "操作系统".into(),
        },
        CourseEntry {
            category: Category::Required,
            kch: "TE204003".into(),
            kxh: "02".into(),
            kcm: "大学物理".into(),
        },
    ];
    write_entries(&path, &sample)?;
    println!("已生成课程文件模板：{}", path.display());
    Ok(())
}

/// `conf import`：从 CSV/XLSX 导入课程到课程文件（按 类别+KCH+KXH 去重）。
fn cmd_import(cli: &Cli, file: &Path) -> Result<(), AppError> {
    let target = cli.courses_path();
    let imported = read_entries(file)?;
    let mut existing: Vec<CourseEntry> = if target.exists() {
        read_entries(&target)?
    } else {
        Vec::new()
    };
    let mut seen: HashSet<(Category, String, String)> = existing
        .iter()
        .map(|e| (e.category, e.kch.clone(), e.kxh.clone()))
        .collect();
    let mut added = 0usize;
    for e in imported {
        if seen.insert((e.category, e.kch.clone(), e.kxh.clone())) {
            existing.push(e);
            added += 1;
        }
    }
    write_entries(&target, &existing)?;
    println!(
        "已导入 {added} 门课程（去重后），课程文件 {} 现有 {} 门",
        target.display(),
        existing.len()
    );
    Ok(())
}

/// `conf export`：导出课程到 CSV/XLSX（按扩展名决定格式）。
fn cmd_export(cli: &Cli, file: &Path) -> Result<(), AppError> {
    let src = cli.courses_path();
    if !src.exists() {
        return Err(AppError::Config(format!(
            "课程文件 {} 不存在",
            src.display()
        )));
    }
    let entries = read_entries(&src)?;
    write_entries(file, &entries)?;
    println!("已导出 {} 门课程到 {}", entries.len(), file.display());
    Ok(())
}
