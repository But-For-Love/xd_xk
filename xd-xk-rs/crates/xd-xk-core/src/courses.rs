//! 课程文件读写（`courses.csv` 默认，可选 `courses.xlsx`）。
//!
//! 约定（设计文档 §6.5.3）：
//! - 写入：UTF-8 带 BOM（中国版 Excel 双击不乱码）；
//! - 读取：按扩展名 + 魔数嗅探 —— 以 `.csv` 命名的 xlsx（`PK\x03\x04`）
//!   也按 xlsx 解析，防"另存为"脚枪；文本文件先试 UTF-8（剥 BOM），
//!   失败再按 GBK 解码；
//! - 表头四列：`类别,KCH,KXH,KCM`，`KXH` 选修可空、必修必填。

use std::fs;
use std::path::Path;

use crate::course::Category;
use crate::error::truncate;
use crate::error::AppError;

/// 一行课程（选课池四列，全链路一致）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseEntry {
    pub category: Category,
    pub kch: String,
    pub kxh: String,
    pub kcm: String,
}

const HEADERS: [&str; 4] = ["类别", "KCH", "KXH", "KCM"];

/// 解析类别列值：中文标签或 Python 兼容别名（`0`/`1`/`bx`/`xx`）。
pub fn parse_category(s: &str) -> Option<Category> {
    match s.trim() {
        "必修" | "0" | "bx" => Some(Category::Required),
        "选修" | "1" | "xx" => Some(Category::Elective),
        _ => None,
    }
}

/// 读取课程文件。按扩展名 / 魔数识别格式。
pub fn read_entries(path: &Path) -> Result<Vec<CourseEntry>, AppError> {
    let bytes = fs::read(path)
        .map_err(|e| AppError::Io(format!("读取课程文件 {} 失败：{e}", path.display())))?;
    if is_xlsx_magic(&bytes) {
        return read_xlsx_bytes(&bytes, path);
    }
    let text = decode_text(&bytes, path)?;
    read_csv_text(&text, path)
}

/// 写入课程文件。扩展名为 `.xlsx`（且启用 xlsx feature）写 XLSX，
/// 否则写 CSV（UTF-8 带 BOM）。
pub fn write_entries(path: &Path, entries: &[CourseEntry]) -> Result<(), AppError> {
    if path.extension().and_then(|e| e.to_str()) == Some("xlsx") {
        #[cfg(feature = "xlsx")]
        {
            return write_xlsx(path, entries);
        }
        #[cfg(not(feature = "xlsx"))]
        {
            return Err(AppError::Config(
                "课程文件为 .xlsx，但当前构建未启用 xlsx feature（用 `--features xd-xk-core/xlsx` 重新构建）"
                    .into(),
            ));
        }
    }
    write_csv(path, entries)
}

/// 写 CSV：UTF-8 带 BOM。
pub fn write_csv(path: &Path, entries: &[CourseEntry]) -> Result<(), AppError> {
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(Vec::new());
    wtr.write_record(HEADERS)
        .map_err(|e| AppError::Io(format!("写入课程文件 {} 失败：{e}", path.display())))?;
    for e in entries {
        wtr.write_record([e.category.label(), &e.kch, &e.kxh, &e.kcm])
            .map_err(|e| AppError::Io(format!("写入课程文件 {} 失败：{e}", path.display())))?;
    }
    let mut bytes = wtr
        .into_inner()
        .map_err(|e| AppError::Io(format!("写入课程文件 {} 失败：{e}", path.display())))?;
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.append(&mut bytes);
    fs::write(path, bom)
        .map_err(|e| AppError::Io(format!("写入课程文件 {} 失败：{e}", path.display())))?;
    Ok(())
}

/// 是否以 XLSX 魔数开头（`PK\x03\x04`）。
fn is_xlsx_magic(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

/// 解码文本：剥 UTF-8 BOM 后先试 UTF-8，失败按 GBK 解码。
fn decode_text(bytes: &[u8], path: &Path) -> Result<String, AppError> {
    let body = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };
    if let Ok(s) = std::str::from_utf8(body) {
        return Ok(s.to_string());
    }
    let (cow, _, _) = encoding_rs::GBK.decode(body);
    if cow.contains('\u{FFFD}') {
        return Err(AppError::Parse(format!(
            "课程文件 {} 编码无法识别（既不是 UTF-8 也不是 GBK）",
            path.display()
        )));
    }
    Ok(cow.into_owned())
}

/// 解析 CSV 文本（按表头名定位列，容忍列顺序变化）。
fn read_csv_text(text: &str, path: &Path) -> Result<Vec<CourseEntry>, AppError> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(text.as_bytes());
    let headers = rdr
        .headers()
        .map_err(|e| AppError::Parse(format!("课程文件 {} 表头解析失败：{e}", path.display())))?
        .clone();
    let col = |name: &str, fallback: usize| -> usize {
        headers
            .iter()
            .position(|h| h.trim() == name)
            .unwrap_or(fallback)
    };
    let c_cat = col("类别", 0);
    let c_kch = col("KCH", 1);
    let c_kxh = col("KXH", 2);
    let c_kcm = col("KCM", 3);

    let mut out = Vec::new();
    for (row_idx, rec) in rdr.records().enumerate() {
        let rec = rec.map_err(|e| {
            AppError::Parse(format!(
                "课程文件 {} 第 {} 行解析失败：{e}",
                path.display(),
                row_idx + 2
            ))
        })?;
        let category = parse_category(rec.get(c_cat).unwrap_or("")).ok_or_else(|| {
            AppError::Parse(format!(
                "课程文件 {} 第 {} 行「类别」无效：{}",
                path.display(),
                row_idx + 2,
                truncate(rec.get(c_cat).unwrap_or(""), 20)
            ))
        })?;
        out.push(CourseEntry {
            category,
            kch: rec.get(c_kch).unwrap_or("").trim().to_string(),
            kxh: rec.get(c_kxh).unwrap_or("").trim().to_string(),
            kcm: rec.get(c_kcm).unwrap_or("").trim().to_string(),
        });
    }
    Ok(out)
}

// ── xlsx feature：calamine 读 + rust_xlsxwriter 写 ──────────────────

#[cfg(feature = "xlsx")]
fn read_xlsx_bytes(bytes: &[u8], path: &Path) -> Result<Vec<CourseEntry>, AppError> {
    use calamine::Reader as _;
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mut wb: calamine::Xlsx<std::io::Cursor<Vec<u8>>> = calamine::open_workbook_from_rs(cursor)
        .map_err(|e| AppError::Parse(format!("课程文件 {} 不是合法 XLSX：{e}", path.display())))?;
    let Some(range) = wb.worksheet_range_at(0) else {
        return Err(AppError::Parse(format!(
            "课程文件 {} 没有工作表",
            path.display()
        )));
    };
    let range = range
        .map_err(|e| AppError::Parse(format!("课程文件 {} 工作表解析失败：{e}", path.display())))?;
    let mut out = Vec::new();
    // 第一行表头
    let mut iter = range.rows();
    let Some(header_row) = iter.next() else {
        return Ok(out);
    };
    let cell_str = |c: &calamine::Data| -> String {
        match c {
            calamine::Data::String(s) => s.clone(),
            calamine::Data::Int(i) => i.to_string(),
            calamine::Data::Float(f) => f.to_string(),
            _ => String::new(),
        }
    };
    let hdrs: Vec<String> = header_row.iter().map(cell_str).collect();
    let col = |name: &str, fallback: usize| -> usize {
        hdrs.iter()
            .position(|h| h.trim() == name)
            .unwrap_or(fallback)
    };
    let c_cat = col("类别", 0);
    let c_kch = col("KCH", 1);
    let c_kxh = col("KXH", 2);
    let c_kcm = col("KCM", 3);

    for (row_idx, r) in iter.enumerate() {
        let category = parse_category(&cell_str(r.get(c_cat).unwrap_or(&calamine::Data::Empty)))
            .ok_or_else(|| {
                AppError::Parse(format!(
                    "课程文件 {} 第 {} 行「类别」无效",
                    path.display(),
                    row_idx + 2
                ))
            })?;
        out.push(CourseEntry {
            category,
            kch: cell_str(r.get(c_kch).unwrap_or(&calamine::Data::Empty)),
            kxh: cell_str(r.get(c_kxh).unwrap_or(&calamine::Data::Empty)),
            kcm: cell_str(r.get(c_kcm).unwrap_or(&calamine::Data::Empty)),
        });
    }
    Ok(out)
}

#[cfg(not(feature = "xlsx"))]
fn read_xlsx_bytes(bytes: &[u8], path: &Path) -> Result<Vec<CourseEntry>, AppError> {
    let _ = (bytes, path);
    Err(AppError::Config(
        "课程文件是 XLSX，但当前构建未启用 xlsx feature（用 `--features xd-xk-core/xlsx` 重新构建）"
            .into(),
    ))
}

#[cfg(feature = "xlsx")]
fn write_xlsx(path: &Path, entries: &[CourseEntry]) -> Result<(), AppError> {
    use rust_xlsxwriter::Workbook;
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (i, h) in HEADERS.iter().enumerate() {
        ws.write_string(0, i as u16, *h).map_err(xlsx_err(path))?;
    }
    for (r, e) in entries.iter().enumerate() {
        let row = (r + 1) as u32;
        ws.write_string(row, 0, e.category.label())
            .map_err(xlsx_err(path))?;
        ws.write_string(row, 1, &e.kch).map_err(xlsx_err(path))?;
        ws.write_string(row, 2, &e.kxh).map_err(xlsx_err(path))?;
        ws.write_string(row, 3, &e.kcm).map_err(xlsx_err(path))?;
    }
    wb.save(path).map_err(xlsx_err(path))?;
    Ok(())
}

#[cfg(feature = "xlsx")]
fn xlsx_err(path: &Path) -> impl Fn(rust_xlsxwriter::XlsxError) -> AppError {
    let path = path.to_path_buf();
    move |e| AppError::Io(format!("写入课程文件 {} 失败：{e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<CourseEntry> {
        vec![
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
        ]
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("xd-xk-courses-{}-{}", std::process::id(), name))
    }

    #[test]
    fn csv_roundtrip_with_bom() {
        let path = temp("rt.csv");
        write_csv(&path, &sample()).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "必须写 UTF-8 BOM");
        assert_eq!(read_entries(&path).unwrap(), sample());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_gbk_encoded_csv() {
        let path = temp("gbk.csv");
        let text = "类别,KCH,KXH,KCM\n选修,EY226022,01,操作系统\n";
        let (gbk_bytes, _, _) = encoding_rs::GBK.encode(text);
        fs::write(&path, &gbk_bytes).unwrap();
        let entries = read_entries(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kcm, "操作系统");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_xlsx_named_as_csv_by_magic() {
        let path = temp("fake.csv");
        // 用一个真正的 zip 头（xlsx 魔数）测试魔数嗅探
        fs::write(&path, [0x50, 0x4B, 0x03, 0x04, 0x00, 0x00]).unwrap();
        let err = read_entries(&path).unwrap_err().to_string();
        // 魔数命中 XLSX 必须按 XLSX 处理：feature 缺失 → 报 feature 错误；
        // feature 启用 → 报非法 XLSX。绝不能走到 CSV 编码错误分支。
        assert!(
            err.contains("xlsx feature") || err.contains("不是合法 XLSX"),
            "应按 XLSX 处理: {err}"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn column_order_tolerant() {
        let path = temp("cols.csv");
        let text = "KCM,类别,KCH,KXH\n操作系统,选修,EY226022,01\n";
        fs::write(&path, text).unwrap();
        let entries = read_entries(&path).unwrap();
        assert_eq!(entries[0].kch, "EY226022");
        assert_eq!(entries[0].category, Category::Elective);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn bad_category_reports_row() {
        let path = temp("bad.csv");
        fs::write(&path, "类别,KCH,KXH,KCM\n公选,EY1,01,课\n").unwrap();
        let err = read_entries(&path).unwrap_err();
        assert!(err.to_string().contains("第 2 行"), "{err}");
        let _ = fs::remove_file(&path);
    }

    #[cfg(feature = "xlsx")]
    #[test]
    fn xlsx_roundtrip_by_extension_and_magic() {
        let path = temp("rt.xlsx");
        write_xlsx(&path, &sample()).unwrap();
        assert_eq!(
            read_entries(&path).unwrap(),
            sample(),
            "按 .xlsx 扩展名读写"
        );
        let _ = fs::remove_file(&path);

        // 以 .csv 命名但实为 xlsx 的"另存为"脚枪：按魔数识别
        let path2 = temp("fake.xls.csv");
        write_xlsx(&path2, &sample()).unwrap();
        assert_eq!(
            read_entries(&path2).unwrap(),
            sample(),
            "按 PK 魔数识别 xlsx"
        );
        let _ = fs::remove_file(&path2);
    }
}
