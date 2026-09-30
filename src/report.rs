/// 结果块最多列这么多行, 超过则只列调用方筛出来的那些
pub const LIST_LIMIT: usize = 200;

/// 标签列固定这么宽, 比最长的标签 `[UNREADABLE]` 再留一档
/// 固定宽度而不是按本轮内容自适应, 是为了让同一轮里所有块以及多轮之间的输出现齐
pub const TAG_WIDTH: usize = 15;

/// 计数带名词: 1 file / 2 files
///
/// 直接写 "{n} files" 会在 n = 1 时打出 "1 files", 而报告是要贴进工单的
pub fn files(n: usize) -> String {
    match n {
        1 => "1 file".to_string(),
        _ => format!("{n} files"),
    }
}

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
///
/// 返回实际打印的行数, 调用方据此决定要不要补那个分隔空行 ——
/// 无条件补的话, 一个全通过的目录会打出一份以空行开头的报告
pub fn print_rows(rows: &[Row]) -> usize {
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
        return shown + 1;
    }
    shown
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
