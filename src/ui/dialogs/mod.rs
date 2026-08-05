pub mod item_edit;
pub mod settings;

pub use item_edit::{add_item_dialog, item_props_dialog};
pub use settings::settings_dialog;

use std::rc::Rc;

use windui::core::EventCtx;
use windui::prelude::Element;
use windui::style::Role;
use windui::theme;

use crate::state::State;

/// 对话框底部按钮行：取消 + 主操作（danger 时红色）。
pub(crate) fn dialog_footer(
    cancel_label: &str,
    ok_label: &str,
    on_cancel: impl FnMut(&mut EventCtx) + 'static,
    on_ok: impl FnMut(&mut EventCtx) + 'static,
    ok_danger: bool,
) -> Element {
    let ok = if ok_danger {
        Element::button(ok_label).small().danger().on_click(on_ok)
    } else {
        Element::button(ok_label).small().on_click(on_ok)
    };
    Element::row()
        .width_match()
        .child(Element::flex_spacer())
        .child(
            Element::button(cancel_label)
                .small()
                .outline()
                .neutral()
                .on_click(on_cancel),
        )
        .child(ok)
}

/// 表单字段标签。
pub(crate) fn field_label(text: impl Into<String>, fs: f32) -> Element {
    Element::label(text)
        .font_size(fs - 0.5)
        .fg_role(Role::TextMuted)
        .width_match()
        .height(18)
}

/// 新增集合对话框。
pub fn add_collection_dialog(st: Rc<State>) -> Element {
    let dlg = st.dlg_add_col;
    let cancel_st = st.clone();
    let commit_st = st.clone();
    Element::dialog_panel(
        dlg,
        "新增集合",
        360,
        move |_| dlg.set(false),
        Element::text_input(st.add_col_name, "输入集合名称…").width_match(),
        dialog_footer(
            "取消",
            "创建",
            move |_| cancel_st.dlg_add_col.set(false),
            move |_| commit_st.commit_add_col(),
            false,
        ),
    )
}

/// 重命名集合对话框。
pub fn rename_collection_dialog(st: Rc<State>) -> Element {
    let dlg = st.dlg_rename_col;
    let cancel_st = st.clone();
    let commit_st = st.clone();
    Element::dialog_panel(
        dlg,
        "重命名集合",
        360,
        move |_| dlg.set(false),
        Element::text_input(st.add_col_name, "输入新名称…").width_match(),
        dialog_footer(
            "取消",
            "重命名",
            move |_| cancel_st.dlg_rename_col.set(false),
            move |_| commit_st.commit_rename_col(),
            false,
        ),
    )
}

/// 删除确认对话框。
pub fn delete_dialog(st: Rc<State>) -> Element {
    let fs = theme::current().metrics.font_sm;
    let dlg = st.dlg_delete;
    let cancel_st = st.clone();
    let confirm_st = st.clone();
    Element::dialog_panel(
        dlg,
        "确认删除",
        380,
        move |_| dlg.set(false),
        Element::label_rc(st.delete_text)
            .font_size(fs + 0.5)
            .width_match(),
        dialog_footer(
            "取消",
            "删除",
            move |_| cancel_st.dlg_delete.set(false),
            move |_| confirm_st.confirm_delete(),
            true,
        ),
    )
}

/// 导入确认对话框：导入会覆盖当前全部数据，需用户确认。
pub fn import_confirm_dialog(st: Rc<State>) -> Element {
    let fs = theme::current().metrics.font_sm;
    let dlg = st.dlg_import_confirm;
    let cancel_st = st.clone();
    let confirm_st = st.clone();
    Element::dialog_panel(
        dlg,
        "导入数据",
        380,
        move |_| dlg.set(false),
        Element::label("导入将覆盖当前全部集合与设置，且立即保存。确定继续吗？")
            .font_size(fs + 0.5)
            .width_match(),
        dialog_footer(
            "取消",
            "导入",
            move |_| cancel_st.dlg_import_confirm.set(false),
            move |_| confirm_st.confirm_import(),
            true,
        ),
    )
}

/// 错误提示对话框。
pub fn error_dialog(st: Rc<State>) -> Element {
    let fs = theme::current().metrics.font_sm;
    let dlg = st.dlg_error;
    let ok_st = st.clone();
    Element::dialog_panel(
        dlg,
        "出错",
        380,
        move |_| dlg.set(false),
        Element::label_rc(st.save_error)
            .font_size(fs + 0.5)
            .width_match(),
        Element::row()
            .width_match()
            .child(Element::flex_spacer())
            .child(
                Element::button("知道了")
                    .small()
                    .on_click(move |_| ok_st.dlg_error.set(false)),
            ),
    )
}
