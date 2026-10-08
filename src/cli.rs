use std::path::{Component, Path, PathBuf};

use crate::anchor;
use crate::error::{Error, Result};
use crate::jsonio;
use crate::official::OfficialHash;
use crate::upstream::Platform;

/// 模型根目录的标志: 有这个子目录就说明"当前目录是模型根, 或者它的子目录"
const MARKER: &str = ".iCheck";

/// 定位模型根
///
/// 显式给了路径就用它, 不做向上查找; 没给就从给定起点往上找带 `.iCheck` 的目录 ——
/// 这样在模型目录里, 或者在它的任何子目录里(比如 `inference/`)都能直接跑
///
/// 找不到就退回起点: `check` 本来就要在还没有 `.iCheck` 的目录上新建它
fn resolve_root_in(start: &Path, given: Option<&str>) -> Result<PathBuf> {
    if let Some(p) = given {
        let root = PathBuf::from(p);
        if !root.is_dir() {
            return Err(Error::Usage(format!(
                "no such directory: {}, the first argument is the model directory on disk",
                root.display()
            )));
        }
        return Ok(root);
    }

    if start.join(MARKER).is_dir() {
        return Ok(start.to_path_buf());
    }
    let mut dir = start;
    while let Some(parent) = dir.parent() {
        if parent.join(MARKER).is_dir() {
            return Ok(parent.to_path_buf());
        }
        dir = parent;
    }
    Ok(start.to_path_buf())
}

pub fn resolve_root(given: Option<&str>) -> Result<PathBuf> {
    let start = std::env::current_dir()?;
    resolve_root_in(&start, given)
}

/// check 需要的两个东西: 模型 ID 与来源
///
/// 官方记录里已经有就省掉; 没有就必须给, 而且报错要给一条能直接复制的完整命令
pub fn resolve_check(
    root: &Path,
    model_given: Option<&str>,
    source_given: Option<Platform>,
) -> Result<(String, Platform)> {
    let state_path = anchor::official_path(root);
    let state: Option<OfficialHash> = jsonio::load(&state_path)?;
    let recorded = state
        .as_ref()
        .and_then(|s| Platform::parse(&s.source.platform));

    let source = match (source_given, recorded) {
        // 官方记录里写着来源, 命令行却给了另一个: 换来源必须先把记录清掉, 否则两边对不上
        (Some(given), Some(found)) if given != found => {
            return Err(Error::Usage(format!(
                "{} was built from {}, cannot switch to {}; delete it to start over",
                anchor::OFFICIAL_REL,
                found.display_name(),
                given.display_name()
            )));
        }
        (Some(given), _) => given,
        (None, Some(found)) => found,
        (None, None) => Platform::ModelScope,
    };

    let model_id = match (model_given, state.as_ref()) {
        (Some(given), _) => given.to_string(),
        (None, Some(s)) => s.source.model_id.clone(),
        (None, None) => {
            let id = guess_model_id(root);
            let shown = id.clone().unwrap_or_else(|| "<author>/<model>".to_string());
            return Err(Error::Usage(format!(
                "check needs <author>/<model> the first time, try: icheck check {} --source {} {shown}",
                root.display(),
                source.as_str()
            )));
        }
    };

    Ok((model_id, source))
}

/// 路径末两段像不像 `author/model`, 而且这个目录里确实有文件
///
/// 只用于拼提示里那条可以直接复制的命令, 不自动采用: 猜对了省一次输入,
/// 猜错了会安静地校验一个错的模型
///
/// "里面有文件"这一条是为了挡住 `/data/models` 这类父目录 —— 它也有两段路径,
/// 但那是用来存放一堆模型的, 猜成 `data/models` 只会把人带偏
fn guess_model_id(root: &Path) -> Option<String> {
    if !has_a_file(root) {
        return None;
    }
    let tail: Vec<&str> = root
        .components()
        .rev()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .take(2)
        .collect();
    match tail.as_slice() {
        [model, author] => Some(format!("{author}/{model}")),
        _ => None,
    }
}

fn has_a_file(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| e.path().is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("icheck-cli-{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn marker(root: &Path) {
        fs::create_dir_all(root.join(MARKER)).unwrap();
    }

    fn write_state(root: &Path, platform: &str, model_id: &str) {
        marker(root);
        fs::create_dir_all(root.join(MARKER).join("official")).unwrap();
        let state = format!(
            r#"{{"tool":"icheck test","fetched_at":"2026-01-01T00:00:00+08:00","source":{{"platform":"{platform}","model_id":"{model_id}","url":"u"}},"files":[]}}"#
        );
        fs::write(root.join(MARKER).join("official/official_hash.json"), state).unwrap();
    }

    #[test]
    fn a_given_path_wins_and_is_never_searched_upwards() {
        let dir = scratch("given");
        let deep = dir.join("a/b/c");
        fs::create_dir_all(&deep).unwrap();
        marker(&dir);

        let got = resolve_root_in(&deep, Some(deep.to_str().unwrap())).unwrap();
        assert_eq!(got, deep);
    }

    #[test]
    fn a_missing_path_is_a_usage_error() {
        let dir = scratch("missing");
        let err = resolve_root_in(&dir, Some("no/such/dir")).unwrap_err();
        assert!(format!("{err}").contains("no such directory"), "{err}");
    }

    /// 在模型目录里: 当前目录本身就是根
    #[test]
    fn the_current_directory_can_be_the_root() {
        let dir = scratch("here");
        marker(&dir);
        assert_eq!(resolve_root_in(&dir, None).unwrap(), dir);
    }

    /// 在子目录里: 往上找
    #[test]
    fn finds_the_root_by_walking_up() {
        let dir = scratch("up");
        marker(&dir);
        let deep = dir.join("inference/examples");
        fs::create_dir_all(&deep).unwrap();

        assert_eq!(resolve_root_in(&deep, None).unwrap(), dir);
    }

    /// 往上找不到就退回起点: check 要在这里新建 .iCheck
    #[test]
    fn falls_back_to_the_start() {
        let dir = scratch("fallback");
        let deep = dir.join("x/y");
        fs::create_dir_all(&deep).unwrap();
        assert_eq!(resolve_root_in(&deep, None).unwrap(), deep);
    }

    #[test]
    fn reads_the_model_id_and_source_from_the_record() {
        let root = scratch("record");
        write_state(&root, "huggingface", "deepseek-ai/X");

        let (model, source) = resolve_check(&root, None, None).unwrap();
        assert_eq!(model, "deepseek-ai/X");
        assert_eq!(source, Platform::HuggingFace);
    }

    #[test]
    fn the_command_line_overrides_the_record() {
        let root = scratch("override");
        write_state(&root, "modelscope", "old/id");

        let (model, source) = resolve_check(&root, Some("new/id"), None).unwrap();
        assert_eq!(model, "new/id");
        assert_eq!(source, Platform::ModelScope);
    }

    /// 换来源必须先把官方记录清掉, 否则快照对不上
    #[test]
    fn switching_the_source_needs_a_clean_record() {
        let root = scratch("switch");
        write_state(&root, "modelscope", "a/b");

        let err = resolve_check(&root, None, Some(Platform::HuggingFace)).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("cannot switch"), "{msg}");
        assert!(msg.contains("ModelScope") && msg.contains("HuggingFace"), "{msg}");
    }

    /// 首次校验: 两样都得给, 报错里带一条可以直接复制的命令
    #[test]
    fn the_first_check_hints_at_a_full_command() {
        let root = scratch("first").join("deepseek-ai/DeepSeek-V4.1-Flash");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("config.json"), b"{}").unwrap();

        let err = resolve_check(&root, None, None).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("the first time"), "{msg}");
        assert!(msg.contains("--source modelscope"), "{msg}");
        assert!(msg.contains("deepseek-ai/DeepSeek-V4.1-Flash"), "{msg}");
    }

    #[test]
    fn no_source_on_a_fresh_directory_defaults_to_modelscope() {
        let root = scratch("default");
        fs::create_dir_all(&root).unwrap();
        let (_, source) = resolve_check(&root, Some("a/b"), None).unwrap();
        assert_eq!(source, Platform::ModelScope);
    }

    #[test]
    fn guesses_the_model_id_from_a_model_directory() {
        let root = scratch("guess").join("deepseek-ai/DeepSeek-V4.1-Flash");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("config.json"), b"{}").unwrap();

        assert_eq!(
            guess_model_id(&root).as_deref(),
            Some("deepseek-ai/DeepSeek-V4.1-Flash")
        );
    }

    /// 存放模型的父目录也是两段路径, 但里面只有目录, 不该猜
    #[test]
    fn does_not_guess_for_a_parent_directory() {
        let parent = scratch("guess-parent").join("data/models");
        fs::create_dir_all(parent.join("deepseek-ai/X")).unwrap();

        assert_eq!(guess_model_id(&parent).as_deref(), None);
        assert_eq!(guess_model_id(Path::new("/")).as_deref(), None);
    }
}
