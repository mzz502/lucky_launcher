use std::process::Command;

use windows::core::PCWSTR;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::model::{Item, ItemKind};
use crate::win_utils::to_wide;

/// 启动一个快捷方式条目。exe 用进程直接拉起（可带参数/工作目录），其余交给系统关联程序。
pub fn launch_item(item: &Item) -> Result<(), String> {
    let target = item.target.trim();
    if target.is_empty() {
        return Err("目标路径为空".to_string());
    }
    let lower = target.to_ascii_lowercase();
    let is_exe = item.kind == ItemKind::Application
        && (lower.ends_with(".exe") || lower.ends_with(".com"));
    if is_exe {
        let mut cmd = Command::new(target);
        if !item.args.trim().is_empty() {
            cmd.args(split_args(&item.args));
        }
        if !item.working_dir.trim().is_empty() {
            cmd.current_dir(item.working_dir.trim());
        }
        match cmd.spawn() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("启动失败：{e}")),
        }
    } else {
        shell_open(target, &item.args)
    }
}

fn shell_open(target: &str, args: &str) -> Result<(), String> {
    let file = to_wide(target);
    let params = to_wide(args);
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let dir = to_wide(&dir);
    let op = to_wide("open");
    // 两段式：参数已序列化到栈上局部，再调 OS API，无借用冲突。
    let res = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(params.as_ptr()),
            PCWSTR(dir.as_ptr()),
            SW_SHOWNORMAL,
        )
    };
    if (res.0 as isize) <= 32 {
        Err("系统无法打开该目标".to_string())
    } else {
        Ok(())
    }
}

/// 简易参数拆分：支持双引号包裹带空格片段。
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for c in s.chars() {
        match c {
            '"' => in_quote = !in_quote,
            ' ' if !in_quote => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_args_handles_quotes() {
        assert_eq!(split_args(r#"-a "b c" -d"#), vec!["-a", "b c", "-d"]);
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("one"), vec!["one"]);
    }

    #[test]
    fn exe_detection_only() {
        let it = Item::new("X", "C:/tools/x.exe");
        assert_eq!(it.kind, ItemKind::Application);
        assert_eq!(ItemKind::detect("https://example.com"), ItemKind::Url);
    }
}
