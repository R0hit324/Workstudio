use ratatui::style::{Color, Modifier, Style};
use std::collections::HashMap;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SynStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

pub struct Highlighter {
    ss: &'static SyntaxSet,
    dark: &'static Theme,
    light: &'static Theme,
    cache: HashMap<String, SyntaxReference>,
}

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            ss: syntax_set(),
            dark: &theme_set().themes["base16-ocean.dark"],
            light: &theme_set().themes["InspiredGitHub"],
            cache: HashMap::new(),
        }
    }

    pub fn syntax_for(&mut self, lang_id: &str) -> &SyntaxReference {
        if !self.cache.contains_key(lang_id) {
            let syn = find_syntax(self.ss, lang_id);
            self.cache.insert(lang_id.to_string(), syn);
        }
        self.cache.get(lang_id).unwrap()
    }

    pub fn highlight_lines(
        &mut self,
        lang_id: &str,
        lines: &[&str],
        dark: bool,
    ) -> Vec<Vec<StyledSpan>> {
        let syntax = self.syntax_for(lang_id).clone();
        let theme: &Theme = if dark { self.dark } else { self.light };
        let mut hl = HighlightLines::new(&syntax, theme);
        let mut out = Vec::with_capacity(lines.len());
        for line in lines {
            let spans = hl.highlight_line(line, self.ss).unwrap_or_default();
            out.push(spans_to_styled(spans));
        }
        out
    }
}

fn find_syntax(ss: &SyntaxSet, lang_id: &str) -> SyntaxReference {
    let by_ext = match lang_id {
        "bash" => ss.find_syntax_by_extension("sh"),
        "javascript" => ss.find_syntax_by_extension("js"),
        "cpp" => ss.find_syntax_by_extension("cpp"),
        other => ss.find_syntax_by_extension(other),
    };
    by_ext
        .or_else(|| ss.find_syntax_by_token(lang_id))
        .or_else(|| ss.find_syntax_by_name(lang_id))
        .cloned()
        .unwrap_or_else(|| ss.find_syntax_plain_text().clone())
}

fn spans_to_styled(spans: Vec<(SynStyle, &str)>) -> Vec<StyledSpan> {
    let mut out = Vec::with_capacity(spans.len());
    for (s, text) in spans {
        if text.is_empty() {
            continue;
        }
        let mut style = Style::new().fg(map_color(s.foreground));
        if s.background.a > 0 {
            style = style.bg(map_color(s.background));
        }
        if s.font_style.contains(FontStyle::BOLD) {
            style = style.add_modifier(Modifier::BOLD);
        }
        if s.font_style.contains(FontStyle::ITALIC) {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if s.font_style.contains(FontStyle::UNDERLINE) {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        out.push(StyledSpan {
            text: text.to_string(),
            style,
        });
    }
    out
}

fn map_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}
