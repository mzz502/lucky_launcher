use std::rc::Rc;

use windui::prelude::{Align, Element, PickDialog, WindowSizeHandle};
use windui::theme;

use crate::state::{State, HOTKEYS};

/// 设置对话框：外观 / 启动行为 / 全局热键 / 窗口 / 数据。
pub fn settings_dialog(st: Rc<State>, win_size: WindowSizeHandle) -> Element {
    let fs = theme::current().metrics.font_sm;
    let dlg = st.dlg_settings;
    let ok_st = st.clone();
    let import_st = st.clone();
    let export_st = st.clone();
    let win_st = st.clone();
    let hotkeys: Vec<&str> = HOTKEYS.to_vec();
    let body = Element::col()
        .width_match()
        .spacing(14)
        .child(
            Element::label("外观")
                .font_size(fs)
                .font_weight(700)
                .width_match()
                .height(20),
        )
        .child(
            Element::row()
                .width_match()
                .cross(Align::Center)
                .child(Element::label("主题").width(120).font_size(fs))
                .child(Element::segmented(vec!["浅色", "深色"], st.theme_idx).weight(1.0)),
        )
        .child(
            Element::row()
                .width_match()
                .cross(Align::Center)
                .child(Element::label("图标大小").width(120).font_size(fs))
                .child(Element::segmented(vec!["小", "中", "大"], st.icon_idx).weight(1.0)),
        )
        .child(
            Element::row()
                .width_match()
                .cross(Align::Center)
                .child(Element::label("显示名称").width(120).font_size(fs))
                .child(Element::checkbox("在图标下方显示名称", st.show_labels)),
        )
        .child(Element::divider())
        .child(
            Element::label("启动行为")
                .font_size(fs)
                .font_weight(700)
                .width_match()
                .height(20),
        )
        .child(
            Element::row()
                .width_match()
                .cross(Align::Center)
                .child(Element::label("启动方式").width(120).font_size(fs))
                .child(Element::segmented(vec!["双击", "单击"], st.act_idx).weight(1.0)),
        )
        .child(
            Element::row()
                .width_match()
                .cross(Align::Center)
                .child(Element::label("启动后").width(120).font_size(fs))
                .child(Element::checkbox("隐藏主窗口", st.hide_after)),
        )
        .child(
            Element::row()
                .width_match()
                .cross(Align::Center)
                .child(Element::label("开机自启").width(120).font_size(fs))
                .child(Element::checkbox("登录 Windows 时自动启动", st.autostart)),
        )
        .child(Element::divider())
        .child(
            Element::label("全局热键")
                .font_size(fs)
                .font_weight(700)
                .width_match()
                .height(20),
        )
        .child(
            Element::row()
                .width_match()
                .cross(Align::Center)
                .child(Element::label("呼出/隐藏").width(120).font_size(fs))
                .child(Element::dropdown(hotkeys, st.hotkey_idx).weight(1.0)),
        )
        .child(Element::divider())
        .child(
            Element::label("窗口")
                .font_size(fs)
                .font_weight(700)
                .width_match()
                .height(20),
        )
        .child(
            Element::row()
                .width_match()
                .cross(Align::Center)
                .child(Element::label("窗口尺寸").width(120).font_size(fs))
                .child(
                    Element::button("保存当前窗口大小")
                        .small()
                        .outline()
                        .neutral()
                        .on_click(move |ctx| {
                            let sz = win_size.get();
                            win_st.save_win_size(sz.w, sz.h);
                            ctx.toast_ok(format!("已保存窗口尺寸 {} × {}", sz.w, sz.h));
                        }),
                ),
        )
        .child(
            Element::row()
                .width_match()
                .spacing(8)
                .child(Element::label("呼出位置").width(120).font_size(fs))
                .child(
                    Element::checkbox("呼出时显示在鼠标位置", st.show_at_cursor)
                        .font_size(fs),
                ),
        )
        .child(Element::divider())
        .child(
            Element::label("数据")
                .font_size(fs)
                .font_weight(700)
                .width_match()
                .height(20),
        )
        .child(
            Element::row()
                .width_match()
                .spacing(8)
                .child(
                    Element::button("导入 JSON…")
                        .small()
                        .outline()
                        .neutral()
                        .on_click(move |ctx| {
                            let s = import_st.clone();
                            ctx.request_pick_file(
                                PickDialog::new()
                                    .title("选择要导入的数据文件")
                                    .filter("JSON", &["json"]),
                                move |path| {
                                    if let Some(p) = path {
                                        s.request_import(p.to_string_lossy().as_ref());
                                    }
                                },
                            );
                        }),
                )
                .child(
                    Element::button("导出 JSON…")
                        .small()
                        .outline()
                        .neutral()
                        .on_click(move |ctx| {
                            let s = export_st.clone();
                            ctx.request_save_file(
                                PickDialog::new()
                                    .title("导出数据")
                                    .filter("JSON", &["json"])
                                    .file_name("launcher-backup.json"),
                                move |path| {
                                    if let Some(p) = path {
                                        s.export(p.to_string_lossy().as_ref());
                                    }
                                },
                            );
                        }),
                ),
        );
    Element::dialog_panel_scrollable(
        dlg,
        "设置",
        520,
        560,
        move |_| dlg.set(false),
        body,
        Element::row()
            .width_match()
            .child(Element::flex_spacer())
            .child(
                Element::button("确定")
                    .small()
                    .on_click(move |_| ok_st.dlg_settings.set(false)),
            ),
    )
}
