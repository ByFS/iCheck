/// 结果块最多列这么多行, 超过则只列调用方筛出来的那些
pub const LIST_LIMIT: usize = 200;

/// 标签列固定这么宽, 取最长标签 `[SIZE-MISMATCH]` 的长度
/// 固定宽度而不是按本轮内容自适应, 是为了让同一轮里所有块以及多轮之间的输出现齐
pub const TAG_WIDTH: usize = 15;

/// 一行结果: 一个标签, 一个文件名, 若干缩进的明细行
pub struct Row {
    pub tag: &'static str,
    pub name: String,
    pub detail: Vec<String>,
}

impl Row {
    pub fn new(tag: &'static str, name: &str) -> Self {
        Row {
            tag,
            name: name.to_string(),
            detail: Vec::new(),
        }
    }

    pub fn with_detail(tag: &'static str, name: &str, detail: Vec<String>) -> Self {
        Row {
            tag,
            name: name.to_string(),
            detail,
        }
    }
}

/// 打印一批结果行, 标签列定宽, 明细按同样的缩进展开
pub fn print_rows(rows: &[Row]) {
    let shown = rows.len().min(LIST_LIMIT);

    for r in &rows[..shown] {
        println!("{tag:<width$} {name}", tag = r.tag, name = r.name, width = TAG_WIDTH);
        for line in &r.detail {
            println!("{:width$}   {line}", "", width = TAG_WIDTH);
        }
    }
    if shown < rows.len() {
        println!(
            "{:width$} ... {} more not listed",
            "",
            rows.len() - shown,
            width = TAG_WIDTH
        );
    }
}

/// 摘要块: 若干个 key: value 加上收尾的 Result
///
/// 键与值都对齐到最长的键, `Result:` 与它们同宽, 所以整块看起来是一张表
pub fn print_summary(pairs: &[(&str, String)], result: &str) {
    let width = pairs
        .iter()
        .map(|(k, _)| k.len() + 1)
        .chain(std::iter::once("Result".len() + 1))
        .max()
        .unwrap_or(7);

    for (k, v) in pairs {
        println!("{:<width$} {v}", format!("{k}:"), width = width);
    }
    println!();
    println!("{:<width$} {result}", "Result:", width = width);
}
