use std::io::{self, Write};
use std::sync::OnceLock;

use tokio::io::{AsyncBufReadExt, BufReader, Stdin};
use tokio::sync::Mutex;

use crate::error::XkError;

static STDIN: OnceLock<Mutex<BufReader<Stdin>>> = OnceLock::new();

fn stdin_reader() -> &'static Mutex<BufReader<Stdin>> {
    STDIN.get_or_init(|| Mutex::new(BufReader::new(tokio::io::stdin())))
}

/// 明文读取一行输入；配合 `tokio::select!` 可实现 Ctrl+C 取消。
pub async fn read_line(prompt: &str) -> Result<String, XkError> {
    print!("{prompt}");
    io::stdout().flush()?;

    let mut reader = stdin_reader().lock().await;
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    Ok(line.trim().to_string())
}
