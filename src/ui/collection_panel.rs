use std::rc::Rc;

use windui::prelude::{Align, Element, MenuItem, Role, Truncate};
use windui::theme;

use crate::state::{CollectionView, DeleteTarget, State};

fn collection_row(st: &Rc<State>, cv: CollectionView) -> Element {
    let idx = cv.index;
    let fs = theme::current().metrics.font_sm;
    let s_sel = st.clone();
    let s_click = st.clone();
    let s_menu = st.clone();
    let s_up = st.clone();
    let s_down = st.clone();
    let s_rename = st.clone();
    let s_del = st.clone();
    let s_add = st.clone();
    let s_add_col = st.clone();
    let count_text = cv.count.to_string();
    let name = cv.name.clone();

    Element::stack()
        .fill()
        .height(38)
        .padding_xy(6, 0)
        .child(
            Element::stack()
                .fill()
                .corner(6.0)
                .bg_role_alpha(Role::Accent, 0.14)
                .visible_when(move || s_sel.selected_collection.get() == Some(idx)),
        )
        .child(
            Element::row()
                .fill()
                .padding_xy(8, 0)
                .cross(Align::Center)
                .spacing(6)
                .child(Element::label(name).weight(1.0).truncate(Truncate::End))
                .child(
                    Element::label(count_text)
                        .font_size(fs - 2.0)
                        .fg_role(Role::TextMuted),
                )
                .child(
                    Element::icon_button("\u{25B2}")
                        .size(20, 20)
                        .font_size(9.0)
                        .fg_role(Role::TextMuted)
                        .on_click(move |_ctx| s_up.move_collection(idx, true)),
                )
                .child(
                    Element::icon_button("\u{25BC}")
                        .size(20, 20)
                        .font_size(9.0)
                        .fg_role(Role::TextMuted)
                        .on_click(move |_ctx| s_down.move_collection(idx, false)),
                ),
        )
        .clickable()
        .on_click(move |_ctx| s_click.select_collection(idx))
        .on_context_menu(move || {
            let add = s_add.clone();
            let up = s_menu.clone();
            let down = s_menu.clone();
            let rename = s_rename.clone();
            let del = s_del.clone();
            let col_name = cv.name.clone();
            let col_id = cv.id.clone();
            let add_col = s_add_col.clone();
            vec![
                MenuItem::run("新增集合", move || add_col.open_add_col_dialog(), false),
                MenuItem::run(
                    "新增快捷方式",
                    move || add.open_add_item_dialog(),
                    false,
                ),
                MenuItem::separator(),
                MenuItem::run("上移", move || up.move_collection(idx, true), false),
                MenuItem::run("下移", move || down.move_collection(idx, false), false),
                MenuItem::separator(),
                MenuItem::run("重命名", move || rename.open_rename_col_dialog(idx), false),
                MenuItem::run(
                    "删除",
                    move || {
                        del.request_delete(DeleteTarget::Collection {
                            id: col_id.clone(),
                            name: col_name.clone(),
                        })
                    },
                    false,
                ),
            ]
        })
}

/// 左侧集合列表面板。
pub fn collection_panel(st: &Rc<State>) -> Element {
    let s_rows = st.clone();
    let list = Element::list_signal(
        st.collections,
        |c: &CollectionView| c.id.clone(),
        move |c: CollectionView| collection_row(&s_rows, c),
    );
    let s_add = st.clone();
    let fs = theme::current().metrics.font_sm;
    Element::col()
        .width(200)
        .height_match()
        .spacing(8)
        .child(
            Element::label("集合")
                .font_size(fs - 2.0)
                .fg_role(Role::TextMuted)
                .padding_xy(10, 0),
        )
        .child(list.weight(1.0))
        .child(
            Element::button("＋ 新增集合")
                .width_match()
                .small()
                .on_click(move |_ctx| s_add.open_add_col_dialog()),
        )
}
