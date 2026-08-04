use crate::languages::Language;

pub struct Completion {
    pub word: String,
    pub kind: &'static str,
}

/// Given the text before the cursor, compute completions for the current word.
pub fn compute(lang: &Language, text_before_cursor: &str) -> Option<(String, Vec<Completion>)> {
    let word = text_before_cursor.rsplit(|c: char| !c.is_alphanumeric() && c != '_' && c != '.').next()?;
    if word.len() < 2 {
        return None;
    }
    let items: Vec<Completion> = lang
        .keywords
        .iter()
        .filter(|k| k.starts_with(word) && **k != word)
        .take(10)
        .map(|k| {
            let kind = if k.contains('.') {
                "method"
            } else if k.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                "class"
            } else {
                "keyword"
            };
            Completion {
                word: k.to_string(),
                kind,
            }
        })
        .collect();
    if items.is_empty() {
        None
    } else {
        Some((word.to_string(), items))
    }
}
