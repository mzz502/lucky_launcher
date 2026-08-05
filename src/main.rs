#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::rc::Rc;
use std::time::Duration;

use windui::prelude::{App, Tray, TrayCtx, TrayMenuItem};

use lucky_launcher::hotkey::parse_hotkey;
use lucky_launcher::model::{Collection, Settings};
use lucky_launcher::state::{IconResult, State};
use lucky_launcher::storage;
use lucky_launcher::ui;
use lucky_launcher::ui::theme::{dark, light};
use lucky_launcher::win_utils;

fn solid_icon(size: u32, hex: u32) -> Vec<u8> {
    let (r, g, b) = (
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    );
    [r, g, b, 255].repeat((size * size) as usize)
}

/// 程序内嵌图标资源 ID（见 build.rs / app.rc）。
const APP_ICON_RES: u16 = 1;

fn build_tray(st: &Rc<State>) -> Tray {
    let s_set = st.clone();
    // 优先用内嵌 app.ico 作为托盘图标，失败回退纯色占位。
    let rgba = win_utils::load_res_icon_rgba(APP_ICON_RES, 16).unwrap_or_else(|| solid_icon(16, 0x4C8BF5));
    Tray::new()
        .tooltip("幸运启动器")
        .icon_rgba(16, 16, &rgba)
        .on_left_click(|ctx: &mut TrayCtx| ctx.show_window())
        .on_double_click(|ctx: &mut TrayCtx| ctx.show_window())
        .menu(vec![
            TrayMenuItem::item("显示主窗口", |ctx: &mut TrayCtx| ctx.show_window()),
            TrayMenuItem::item("隐藏主窗口", |ctx: &mut TrayCtx| ctx.hide_window()),
            TrayMenuItem::separator(),
            TrayMenuItem::item("设置", move |_ctx: &mut TrayCtx| {
                s_set.dlg_settings.set(true)
            }),
            TrayMenuItem::separator(),
            TrayMenuItem::item("退出", |ctx: &mut TrayCtx| ctx.quit()),
        ])
}

fn apply_theme(th: &windui::prelude::ThemeHandle, s: &Settings) {
    let theme = if s.theme == "dark" { dark() } else { light() };
    th.set(theme);
}

fn main() {
    let mut data = storage::load();
    if data.collections.is_empty() {
        data.collections.push(Collection::new("默认"));
    }
    let st = Rc::new(State::new(data));

    let theme = if st.settings_current().theme == "dark" {
        dark()
    } else {
        light()
    };
    // 恢复上次保存的窗口尺寸（设置页手动保存）；未保存过或尺寸异常时用默认值。
    let s_init = st.settings_current();
    let (win_w, win_h) = if s_init.win_w >= 720 && s_init.win_h >= 480 {
        (s_init.win_w, s_init.win_h)
    } else {
        (720, 480)
    };
    let mut app = App::new("幸运启动器", win_w, win_h)
        .frameless()
        .min_size(720, 480)
        .centered()
        .icon(APP_ICON_RES)
        .theme(theme)
        .hide_on_close()
        .tray(build_tray(&st));

    // 单实例：二次启动时唤起已有实例（windui 平台层会自动激活/显示主窗口）。
    // v0.2 测试版使用独立标识，可与 v0.1 同时运行对比。
    app = app.single_instance("com.lucky.launcher.v2", |_args| {});

    // 全局热键（设置可改绑）。
    let default_hk = parse_hotkey(&st.settings_current().global_hotkey);
    let hk_handle = app.hotkey_rc(default_hk, |ctx| ctx.show_window());
    let th_handle = app.theme_handle();
    // 呼出时在鼠标位置（设置可开关）：启动按持久化值初始化。
    let sac_handle = app.show_at_cursor_handle();
    sac_handle.set(st.settings_current().show_at_cursor);

    // 图标后台提取：UI 线程接收结果回填缓存并重建网格，避免 SHGetFileInfoW 阻塞。
    let st_icon = st.clone();
    let icon_tx = app.channel(move |msg: Vec<IconResult>| {
        for r in msg {
            st_icon.cache_icon(&r.path, r.rgba);
        }
        st_icon.rebuild_lists();
    });
    st.set_icon_sender(icon_tx);
    st.invalidate_icons_and_rebuild();

    // 轮询设置控件 + 搜索词变化：设置变化时持久化并即时应用主题/热键；搜索词变化时重算网格过滤。
    let st_poll = st.clone();
    let hk_poll = hk_handle.clone();
    let th_poll = th_handle.clone();
    let sac_poll = sac_handle.clone();
    let mut last_search = st_poll.search.version();
    app = app.on_interval(Duration::from_millis(80), move || {
        if let Some(new_s) = st_poll.sync_settings() {
            apply_theme(&th_poll, &new_s);
            hk_poll.rebind(parse_hotkey(&new_s.global_hotkey));
            sac_poll.set(new_s.show_at_cursor);
        }
        let v = st_poll.search.version();
        if v != last_search {
            last_search = v;
            st_poll.rebuild_lists();
        }
    });

    if std::env::args().any(|a| a == "--minimized") {
        app = app.start_hidden();
    }

    // 窗口尺寸句柄：设置页「保存当前窗口大小」读取当前尺寸并持久化。
    let win_size = app.window_size_handle();

    app = app.content(ui::build_root(&st, win_size));
    app = app.screenshot_from_args();
    app.run();
}
