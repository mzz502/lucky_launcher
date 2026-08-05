use std::rc::Rc;

use windui::prelude::{Element, PickDialog, Signal};
use windui::theme;

use crate::model::ItemKind;
use crate::state::State;

use super::{dialog_footer, field_label};

/// 目标路径输入 + 浏览按钮行（可选"文件夹…"按钮）。
fn browse_row(input: Element, target_sig: Signal<String>, with_folder: bool) -> Element {
    let file_sig = target_sig;
    let row = Element::row()
        .width_match()
        .spacing(8)
        .child(input.weight(1.0))
        .child(
            Element::button("文件…")
                .small()
                .outline()
                .neutral()
                .on_click(move |ctx| {
                    let sig = file_sig;
                    ctx.request_pick_file(
                        PickDialog::new().title("选择目标"),
                        move |path| {
                            if let Some(p) = path {
                                sig.set(p.to_string_lossy().to_string());
                            }
                        },
                    );
                }),
        );
    if !with_folder {
        return row;
    }
    let folder_sig = target_sig;
    row.child(
        Element::button("文件夹…")
            .small()
            .outline()
            .neutral()
            .on_click(move |ctx| {
                let sig = folder_sig;
                ctx.request_pick_folder(
                    PickDialog::new().title("选择文件夹"),
                    move |path| {
                        if let Some(p) = path {
                            sig.set(p.to_string_lossy().to_string());
                        }
                    },
                );
            }),
    )
}

/// 新增快捷方式对话框：名称 + 目标路径（含文件/文件夹浏览）。
pub fn add_item_dialog(st: Rc<State>) -> Element {
    let fs = theme::current().metrics.font_sm;
    let dlg = st.dlg_add_item;
    let cancel_st = st.clone();
    let commit_st = st.clone();
    let target_sig = st.add_item_target;
    let body = Element::col()
        .width_match()
        .spacing(12)
        .child(field_label("名称", fs))
        .child(Element::text_input(st.add_item_name, "例如：VSCode").width_match())
        .child(field_label("目标路径（文件 / 文件夹 / 网址）", fs))
        .child(browse_row(
            Element::text_input(target_sig, "C:\\...\\Code.exe 或 https://…"),
            target_sig,
            true,
        ));
    Element::dialog_panel_scrollable(
        dlg,
        "新增快捷方式",
        460,
        560,
        move |_| dlg.set(false),
        body,
        dialog_footer(
            "取消",
            "添加",
            move |_| cancel_st.dlg_add_item.set(false),
            move |ctx| {
                if commit_st.commit_add_item() {
                    ctx.toast_ok("已添加");
                }
            },
            false,
        ),
    )
}

/// 图标属性对话框：名称 / 类型 / 目标 / 参数 / 工作目录 / 描述。
pub fn item_props_dialog(st: Rc<State>) -> Element {
    let fs = theme::current().metrics.font_sm;
    let dlg = st.dlg_item_props;
    let cancel_st = st.clone();
    let commit_st = st.clone();
    let target_sig = st.edit_target;
    let kinds: Vec<&str> = ItemKind::ALL.map(|k| k.label()).to_vec();
    let body = Element::col()
        .width_match()
        .spacing(12)
        .child(field_label("名称", fs))
        .child(Element::text_input(st.edit_name, "名称…").width_match())
        .child(field_label("类型", fs))
        .child(Element::dropdown(kinds, st.edit_kind_idx).width_match())
        .child(field_label("目标", fs))
        .child(browse_row(
            Element::text_input(target_sig, "路径或网址…"),
            target_sig,
            false,
        ))
        .child(field_label("参数（可选）", fs))
        .child(Element::text_input(st.edit_args, "启动参数…").width_match())
        .child(field_label("工作目录（可选）", fs))
        .child(Element::text_input(st.edit_workdir, "工作目录…").width_match())
        .child(field_label("描述（可选）", fs))
        .child(Element::text_input(st.edit_desc, "描述…").width_match());
    Element::dialog_panel_scrollable(
        dlg,
        "属性",
        460,
        560,
        move |_| dlg.set(false),
        body,
        dialog_footer(
            "取消",
            "保存",
            move |_| cancel_st.dlg_item_props.set(false),
            move |_| commit_st.commit_item_props(),
            false,
        ),
    )
}
