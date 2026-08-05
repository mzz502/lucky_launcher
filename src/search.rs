use pinyin::ToPinyin;

use crate::model::Item;

/// 名字的拼音检索键：(全拼无音调, 首字母缩写)。
pub fn pinyin_keys(name: &str) -> (String, String) {
    let mut full = String::new();
    let mut init = String::new();
    for p in name.to_pinyin().flatten() {
        full.push_str(p.plain());
        init.push_str(&p.first_letter().to_ascii_lowercase());
    }
    (full, init)
}

fn token_matches(token: &str, name_lower: &str, target_lower: &str, full: &str, init: &str) -> bool {
    name_lower.contains(token)
        || target_lower.contains(token)
        || full.contains(token)
        || init.contains(token)
}

/// 判断查询词是否命中条目。支持空格分词（全部命中才算）。
pub fn matches(query: &str, item: &Item) -> bool {
    let (full, init) = pinyin_keys(&item.name);
    matches_prekeyed(query, &item.name, &item.target, &full, &init)
}

/// 拼音键预计算版本：`full/init` 由调用方缓存，避免每次击键重复计算。
pub fn matches_prekeyed(
    query: &str,
    name: &str,
    target: &str,
    full: &str,
    init: &str,
) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let name_lower = name.to_lowercase();
    let file_stem = target
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(target);
    let target_lower = file_stem.to_lowercase();
    query
        .split_whitespace()
        .map(str::to_lowercase)
        .all(|token| token_matches(&token, &name_lower, &target_lower, full, init))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Item;

    fn item(name: &str, target: &str) -> Item {
        Item::new(name, target)
    }

    #[test]
    fn matches_substring() {
        let it = item("Visual Studio Code", "C:/dev/vscode/Code.exe");
        assert!(matches("visual", &it));
        assert!(matches("code", &it));
        assert!(!matches("chrome", &it));
    }

    #[test]
    fn matches_pinyin_full() {
        let it = item("记事本", "notepad.exe");
        assert!(matches("jishi", &it));
        assert!(matches("记事本", &it));
    }

    #[test]
    fn matches_pinyin_initials() {
        let it = item("微信", "wechat.exe");
        assert!(matches("wx", &it));
    }

    #[test]
    fn matches_space_tokens() {
        let it = item("Visual Studio", "vstudio.exe");
        assert!(matches("visual studio", &it));
        assert!(!matches("visual chrome", &it));
    }

    #[test]
    fn empty_query_matches_all() {
        let it = item("任意", "x.exe");
        assert!(matches("", &it));
        assert!(matches("   ", &it));
    }
}
