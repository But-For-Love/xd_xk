//! 操作命令：select / drop / check / list / login。
//!
//! 行为 1:1 对齐 Python `cli.py`，并叠加设计文档 §6 新增能力
//! （`--interval` / `--no-grab` / `--until` / `--rounds` / `--json` / `--dry-run`）。

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde_json::json;
use tokio_util::sync::CancellationToken;

use xd_xk_core::config::Config;
use xd_xk_core::course::{resolve_target, Category, CourseRow};
use xd_xk_core::courses::{read_entries, CourseEntry};
use xd_xk_core::error::AppError;
use xd_xk_core::ocr::CaptchaOcr;
use xd_xk_core::ops::{PollConfig, StopReason};
use xd_xk_core::session::CourseSession;

use crate::args::{CategoryArg, CheckArgs, Cli, CourseId, ListArgs, SelectArgs};
use crate::runner::{build_api, load_config};

const EXIT_OK: i32 = 0;
const EXIT_CANCELED: i32 = 130;

/// 登录 → 展示信息 → 匹配批次，返回会话与加载好的配置。
async fn login_session(
    cli: &Cli,
    env: &HashMap<String, String>,
    ocr: &dyn CaptchaOcr,
) -> Result<(CourseSession, Config), AppError> {
    let cfg = load_config(cli, env)?;
    let cancel = CancellationToken::new();
    let api = build_api(cli)?;
    let session = CourseSession::create_with_api(api, &cfg, ocr, &cancel).await?;
    Ok((session, cfg))
}

/// 从课程文件读取指定类别的课程列表（KCH 为空的行跳过，与 Python 一致）。
fn entries_for(cli: &Cli, category: Category) -> Result<Vec<CourseEntry>, AppError> {
    let path = cli.courses_path();
    if !path.exists() {
        return Err(AppError::Config(format!(
            "课程文件 {} 不存在。请先运行 `xd-xk conf template` 生成模板，或 `xd-xk conf migrate` 从旧配置迁移",
            path.display()
        )));
    }
    let all = read_entries(&path)?;
    Ok(all.into_iter().filter(|e| e.category == category).collect())
}

/// 选课/退课操作类别。
#[derive(Debug, Clone, Copy)]
enum Operation {
    Select,
    Drop,
}

impl Operation {
    fn verb(self) -> &'static str {
        match self {
            Operation::Select => "选课",
            Operation::Drop => "退课",
        }
    }

    fn char(self) -> &'static str {
        match self {
            Operation::Select => "选",
            Operation::Drop => "退",
        }
    }
}

/// select 命令。
pub(crate) async fn cmd_select(
    cli: &Cli,
    env: &HashMap<String, String>,
    ocr: &dyn CaptchaOcr,
    args: &SelectArgs,
) -> Result<i32, AppError> {
    let params = OpParams {
        category: args.category,
        interactive: args.interactive,
        once: args.once,
        interval: args.interval,
        courses: &args.courses,
    };
    cmd_operation(cli, env, ocr, Operation::Select, params).await
}

/// drop 命令。
pub(crate) async fn cmd_drop(
    cli: &Cli,
    env: &HashMap<String, String>,
    ocr: &dyn CaptchaOcr,
    args: &crate::args::DropArgs,
) -> Result<i32, AppError> {
    let params = OpParams {
        category: args.category,
        interactive: false,
        once: args.once,
        interval: args.interval,
        courses: &args.courses,
    };
    cmd_operation(cli, env, ocr, Operation::Drop, params).await
}

/// 选课 / 退课操作参数（打包以减少参数个数）。
struct OpParams<'a> {
    category: CategoryArg,
    interactive: bool,
    once: bool,
    interval: f64,
    courses: &'a [CourseId],
}

async fn cmd_operation(
    cli: &Cli,
    env: &HashMap<String, String>,
    ocr: &dyn CaptchaOcr,
    op: Operation,
    p: OpParams<'_>,
) -> Result<i32, AppError> {
    let category = p.category.to_core();
    // 课程来自课程文件时提前校验（登录前失败，与 Python 读 conf 课程一致）
    let file_entries: Option<Vec<CourseEntry>> = if !p.interactive && p.courses.is_empty() {
        Some(entries_for(cli, category)?)
    } else {
        None
    };

    let (session, cfg) = login_session(cli, env, ocr).await?;
    tracing::info!("[OK] 登录成功，批次 code：{}", session.batch_code);

    tracing::info!("正在获取课程列表…");
    let rows = session.get_class(&cfg, category).await?;
    tracing::info!("  获取到 {} 门课程", rows.len());

    let targets = match file_entries {
        Some(entries) => entries.into_iter().map(|e| (e.kch, e.kxh)).collect(),
        None => collect_targets(p.category, p.interactive, p.courses, &rows).await?,
    };
    if targets.is_empty() {
        return Err(AppError::Config(format!(
            "没有要{}的{}课程",
            op.char(),
            p.category.label()
        )));
    }

    let cancel = CancellationToken::new();
    spawn_ctrl_c(&cancel);
    let poll = PollConfig {
        always: !p.once,
        interval: Duration::from_secs_f64(p.interval),
        cancel: cancel.clone(),
    };

    let mut results: Vec<serde_json::Value> = Vec::new();
    for (kch, kxh) in &targets {
        let Some(row) = resolve_target(&rows, category, kch, kxh) else {
            tracing::warn!("未找到课程 {kch} {kxh}（该类别共 {} 门）", rows.len());
            continue;
        };
        if cli.dry_run {
            tracing::info!("[dry-run] 将执行{}：{} {}", op.verb(), row.kch, row.kcm);
            continue;
        }
        let outcome = match op {
            Operation::Select => session.add(row, category, &poll).await?,
            Operation::Drop => session.dele(row, category, &poll).await?,
        };
        let reason = reason_label(outcome.reason);
        tracing::info!("[{}完成] {} {}：{}", op.char(), row.kch, row.kcm, reason);
        if cli.json {
            results.push(json!({
                "op": if matches!(op, Operation::Select) { "select" } else { "drop" },
                "kch": row.kch,
                "kxh": row.kxh,
                "kcm": row.kcm,
                "reason": format!("{:?}", outcome.reason),
                "msg": outcome.last_msg,
            }));
        }
    }

    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&results).unwrap_or_default()
        );
    }
    Ok(if cancel.is_cancelled() {
        EXIT_CANCELED
    } else {
        EXIT_OK
    })
}

/// 确定要操作的目标课程列表：交互式多选 > 命令行 CourseId（课程文件路径已在 cmd_operation 预取）。
async fn collect_targets(
    category_arg: CategoryArg,
    interactive: bool,
    course_args: &[CourseId],
    rows: &[CourseRow],
) -> Result<Vec<(String, String)>, AppError> {
    let category = category_arg.to_core();
    if interactive {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let items: Vec<String> = rows.iter().map(|r| course_label(r, category)).collect();
        let chosen = dialoguer::MultiSelect::new()
            .with_prompt("请选择课程（空格选中，回车确认，Esc 取消）")
            .items(&items)
            .interact_opt()
            .map_err(|e| AppError::Config(format!("交互选择失败：{e}")))?
            .unwrap_or_default();
        return Ok(chosen
            .into_iter()
            .map(|i| {
                let r = &rows[i];
                let kxh = if category == Category::Required {
                    r.kxh.clone()
                } else {
                    String::new()
                };
                (r.kch.clone(), kxh)
            })
            .collect());
    }
    Ok(course_args
        .iter()
        .map(|c| (c.kch.clone(), c.kxh.clone().unwrap_or_default()))
        .collect())
}

/// 课程行显示串（交互选择用）。
fn course_label(r: &CourseRow, category: Category) -> String {
    let sel = r.number_selected.map(|n| n.0).unwrap_or(0);
    let cap = r.class_capacity.map(|n| n.0).unwrap_or(0);
    format!(
        "{} {} {} {} {} 已选{sel}/容量{cap}",
        category.label(),
        r.kch,
        r.kxh,
        r.kcm,
        r.skjs.as_deref().unwrap_or(""),
    )
}

/// 终止原因中文标签（GUI 未来上色的等价物）。
fn reason_label(reason: StopReason) -> &'static str {
    match reason {
        StopReason::Success => "操作成功",
        StopReason::AlreadySelected => "该课程已在选课结果中",
        StopReason::Conflict => "所选课程与已选课程冲突",
        StopReason::Full => "所选课程人数已满",
        StopReason::CreditLimit => "选课门数或学分超过",
        StopReason::Canceled => "用户停止",
        StopReason::Unknown => "未命中终止消息",
    }
}

/// check 命令：容量检查。
///
/// 与 Python `cmd_check` 一致：CLI 指定的 KCH 当作选修；否则读课程文件按类别分组。
/// 空位判据 = `SFYX=="0"`（预筛）且 `已选 < 容量`（最终）。
pub(crate) async fn cmd_check(
    cli: &Cli,
    env: &HashMap<String, String>,
    ocr: &dyn CaptchaOcr,
    args: &CheckArgs,
) -> Result<i32, AppError> {
    let (session, cfg) = login_session(cli, env, ocr).await?;
    tracing::info!("[OK] 登录成功，批次 code：{}", session.batch_code);

    let mut by_cat: HashMap<Category, HashSet<String>> = HashMap::new();
    if !args.kch.is_empty() {
        by_cat.insert(Category::Elective, args.kch.iter().cloned().collect());
    } else {
        let path = cli.courses_path();
        if !path.exists() {
            return Err(AppError::Config(format!(
                "课程文件 {} 不存在。请先运行 `xd-xk conf template` 生成模板",
                path.display()
            )));
        }
        let entries = read_entries(&path)?;
        for e in entries {
            by_cat.entry(e.category).or_default().insert(e.kch);
        }
        if by_cat.is_empty() {
            return Err(AppError::Config(
                "未指定课程号，且课程文件中没有课程".into(),
            ));
        }
    }
    let all_kch: Vec<&str> = by_cat.values().flatten().map(|s| s.as_str()).collect();
    tracing::info!("开始容量检查，目标课程号：{all_kch:?}");

    let interval = Duration::from_secs_f64(args.interval);
    let cancel = CancellationToken::new();
    spawn_ctrl_c(&cancel);
    let once_cfg = PollConfig {
        always: false,
        interval,
        cancel: cancel.clone(),
    };

    let start = Instant::now();
    let mut round: u32 = 0;
    let mut slots_found: u32 = 0;
    let mut grabbed: u32 = 0;

    loop {
        if cancel.is_cancelled() {
            break;
        }
        round += 1;
        for (&cat, targets) in &by_cat {
            let rows = session.get_class(&cfg, cat).await?;
            for course in &rows {
                if !targets.contains(&course.kch) || !course.sfyx_available() {
                    continue;
                }
                let sel = course.number_selected.map(|n| n.0).unwrap_or(0);
                let cap = course.class_capacity.map(|n| n.0).unwrap_or(0);
                tracing::info!("{} 已选{sel}/容量{cap}", course.kcm);
                if !course.has_capacity() {
                    continue;
                }
                slots_found += 1;
                if args.no_grab {
                    continue;
                }
                if cli.dry_run {
                    tracing::info!("[dry-run] 发现空位将选课：{} {}", course.kch, course.kcm);
                } else {
                    let outcome = session.add(course, cat, &once_cfg).await?;
                    tracing::info!("[选课] {} {}：{}", course.kch, course.kcm, outcome.last_msg);
                    grabbed += 1;
                }
            }
        }
        tracing::info!("第 {round} 次检查");

        if let Some(r) = args.rounds {
            if round >= r {
                break;
            }
        }
        if let Some(secs) = args.until {
            if start.elapsed().as_secs() >= secs {
                break;
            }
        }
        tokio::time::sleep(interval).await;
    }

    let canceled = cancel.is_cancelled();
    if cli.json {
        println!(
            "{}",
            json!({
                "rounds": round,
                "slots_found": slots_found,
                "grabbed": grabbed,
                "canceled": canceled,
            })
        );
    } else {
        let grab_note = if args.no_grab {
            "未选课（--no-grab）".to_string()
        } else {
            format!("抢到 {grabbed} 门课程")
        };
        tracing::info!("检查结束：共 {round} 轮，发现 {slots_found} 个空位，{grab_note}");
    }
    Ok(if canceled { EXIT_CANCELED } else { EXIT_OK })
}

/// list 命令：拉课程列表并打印（搜索过滤）。
pub(crate) async fn cmd_list(
    cli: &Cli,
    env: &HashMap<String, String>,
    ocr: &dyn CaptchaOcr,
    args: &ListArgs,
) -> Result<i32, AppError> {
    let (session, cfg) = login_session(cli, env, ocr).await?;
    let cats: Vec<Category> = match args.category {
        Some(c) => vec![c.to_core()],
        None => vec![Category::Required, Category::Elective],
    };
    let mut all: Vec<(Category, CourseRow)> = Vec::new();
    for cat in cats {
        let rows = session.get_class(&cfg, cat).await?;
        all.extend(rows.into_iter().map(|r| (cat, r)));
    }
    let kw = args.keyword.clone().unwrap_or_default();
    let filtered: Vec<(Category, CourseRow)> = all
        .into_iter()
        .filter(|(_, r)| {
            kw.is_empty()
                || r.kch.contains(&kw)
                || r.kcm.contains(&kw)
                || r.skjs.as_deref().map(|s| s.contains(&kw)).unwrap_or(false)
        })
        .collect();

    if cli.json {
        let arr: Vec<serde_json::Value> = filtered
            .iter()
            .map(|(cat, r)| {
                json!({
                    "类别": cat.label(),
                    "KCH": r.kch,
                    "KXH": r.kxh,
                    "KCM": r.kcm,
                    "SKJS": r.skjs.as_deref().unwrap_or(""),
                    "numberOfSelected": r.number_selected.map(|n| n.0),
                    "classCapacity": r.class_capacity.map(|n| n.0),
                    "SFYX": r.sfyx.as_deref().unwrap_or(""),
                    "有余量": r.has_capacity(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr).unwrap_or_default());
    } else {
        println!("类别\tKCH\tKXH\tKCM\t教师\t已选/容量");
        for (cat, r) in &filtered {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}/{}",
                cat.label(),
                r.kch,
                r.kxh,
                r.kcm,
                r.skjs.as_deref().unwrap_or(""),
                r.number_selected.map(|n| n.0).unwrap_or(0),
                r.class_capacity.map(|n| n.0).unwrap_or(0),
            );
        }
        println!("共 {} 门课程", filtered.len());
    }
    Ok(EXIT_OK)
}

/// login 命令：验证凭据与批次。
pub(crate) async fn cmd_login(
    cli: &Cli,
    env: &HashMap<String, String>,
    ocr: &dyn CaptchaOcr,
) -> Result<i32, AppError> {
    let (session, _cfg) = login_session(cli, env, ocr).await?;
    if cli.json {
        println!(
            "{}",
            json!({
                "batch_code": session.batch_code,
            })
        );
    } else {
        tracing::info!("[OK] 登录成功，批次 code：{}", session.batch_code);
    }
    Ok(EXIT_OK)
}

/// Ctrl+C → 取消令牌（等价 Python `stop_event`）。
fn spawn_ctrl_c(cancel: &CancellationToken) {
    let c = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            c.cancel();
        }
    });
}
