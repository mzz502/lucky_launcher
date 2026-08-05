use windui::prelude::{Hotkey, Key};

/// 把设置字符串解析为全局热键；未知字符串回退默认 Ctrl+Q。
pub fn parse_hotkey(s: &str) -> Hotkey {
    match s.trim() {
        "Alt+Space" => Hotkey::new(Key::Space).alt(),
        "Ctrl+Alt+L" => Hotkey::new(Key::Char('L')).ctrl().alt(),
        _ => Hotkey::new(Key::Char('Q')).ctrl(),
    }
}
