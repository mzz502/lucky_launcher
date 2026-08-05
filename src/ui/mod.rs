use std::rc::Rc;

use windui::prelude::{Align, Element, Role, WindowButtonKind, WindowSizeHandle};

use crate::state::State;

pub mod collection_panel;
pub mod dialogs;
pub mod item_grid;
pub mod theme;

/// 根布局：标题栏 + 搜索框 + 左侧集合列表 + 右侧图标网格 + 各对话框。
pub fn build_root(st: &Rc<State>, win_size: WindowSizeHandle) -> Element {
    let st = st.clone();
    let font_md = windui::theme::current().metrics.font_md;
    // 标题栏
    let s_set = st.clone();
    let titlebar = Element::row()
        .fill()
        .height(40)
        .window_drag()
        .padding_xy(12, 0)
        .cross(Align::Center)
        .child(
            Element::label("幸运启动器")
                .font_size(font_md)
                .font_weight(600)
                .fg_role(Role::Text)
                .weight(1.0),
        )
        .child(
            Element::icon_button("\u{2699}")
                .size(28, 28)
                .font_size(font_md)
                .fg_role(Role::TextMuted)
                .tooltip("设置")
                .on_click(move |_ctx| s_set.dlg_settings.set(true)),
        )
        .child(Element::window_button(WindowButtonKind::Minimize))
        .child(Element::window_button(WindowButtonKind::Close));

    // 搜索框
    let search_bar = Element::row()
        .fill()
        .padding_xy(12, 8)
        .child(
            Element::text_input(st.search, "搜索快捷方式…")
                .width_match()
                .leading_icon('\u{1F50D}'),
        );

    // 内容区：左集合列表 + 右图标网格
    let content = Element::row()
        .fill()
        .child(collection_panel::collection_panel(&st))
        .child(
            Element::stack()
                .width(1)
                .bg_role(Role::Border)
                .height_match(),
        )
        .child(item_grid::item_panel(&st));

    let root = Element::col()
        .fill()
        .bg_role(Role::Bg)
        .child(titlebar)
        .child(search_bar)
        .child(Element::stack().fill().height(1).bg_role(Role::Divider))
        .child(content.weight(1.0));

    // 对话框叠加层
    Element::stack()
        .fill()
        .child(root)
        .child(dialogs::add_collection_dialog(st.clone()))
        .child(dialogs::rename_collection_dialog(st.clone()))
        .child(dialogs::add_item_dialog(st.clone()))
        .child(dialogs::item_props_dialog(st.clone()))
        .child(dialogs::delete_dialog(st.clone()))
        .child(dialogs::error_dialog(st.clone()))
        .child(dialogs::settings_dialog(st.clone(), win_size.clone()))
        .child(dialogs::import_confirm_dialog(st.clone()))
}
