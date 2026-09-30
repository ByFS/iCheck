/// 结果块最多列这么多行, 超过则只列调用方筛出来的那些
pub const LIST_LIMIT: usize = 200;

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

/// 按本轮出现的最长标签对齐后打印, 超过上限就截断并注明
pub fn print_rows(rows: &[Row]) {
    let width = rows.iter().map(|r| r.tag.len()).max().unwrap_or(3);
    let shown = rows.len().min(LIST_LIMIT);

    for r in &rows[..shown] {
        println!("{tag:<width$} {name}", tag = r.tag, name = r.name);
        for line in &r.detail {
            println!("{:width$}   {line}", "");
        }
    }
    if shown < rows.len() {
        println!(
            "{:width$} ... {} more not listed",
            "",
            rows.len() - shown
        );
    }
}
