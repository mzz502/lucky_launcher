use std::rc::Rc;

use windui::core::{EventCtx, Widget};
use windui::event::{Event, Key, KeyEvent, MouseButton, PointerEvent, PointerKind};
use windui::geometry::{Rect, Size};
use windui::prelude::{Align, Color, Element, MenuItem};
use windui::render::image::VisualState;
use windui::render::{Canvas, Paint};
use windui::style::{Role, Style};
use windui::text::{TextEngine, TextStyle};
use windui::theme;

use crate::model::Activation;
use crate::state::{ItemView, State};
use crate::launch;

/// 虚拟键码：F2（编辑属性）。
const KEY_F2: u32 = 0x71;

/// 卡片绘制常量：选中/按下背景的透明度（基于 accent 派生，随主题联动）。
const SELECTED_ALPHA: u8 = 46;
const PRESSED_ALPHA: u8 = 26;
/// 图标相对卡片顶部的纵向间距。
const ICON_GAP_Y: i32 = 8;
/// 名称文字相对图标底部与卡片底部的间距。
const NAME_TOP: i32 = 10;
const NAME_BOTTOM: i32 = 14;

/// 图标卡片：自绘图标 + 名称 + 选中/悬停高亮，单击选中 / 双击或单击启动（按设置）。
pub struct ItemCard {
    item: ItemView,
    st: Rc<State>,
    hovered: bool,
    pressed: bool,
}

impl ItemCard {
    pub fn new(item: ItemView, st: Rc<State>) -> Self {
        Self {
            item,
            st,
            hovered: false,
            pressed: false,
        }
    }

    fn launch(&self, ctx: &mut EventCtx, id: &str) {
        let Some(item) = self.st.find_item(id) else {
            return;
        };
        let hide = self.st.settings_current().hide_after_launch;
        match launch::launch_item(&item) {
            Ok(()) => {
                if hide {
                    ctx.hide_window();
                }
            }
            Err(e) => ctx.toast_err(&e),
        }
    }
}

impl Widget for ItemCard {
    fn measure(&self, _avail: Size, _style: &Style, _text: &mut dyn TextEngine) -> Size {
        let cell = self.st.settings_current().icon_size.cell_size();
        Size::new(cell as i32, cell as i32)
    }

    fn paint(
        &self,
        bounds: Rect,
        content: Rect,
        focused: bool,
        _enabled: bool,
        canvas: &mut dyn Canvas,
        style: &Style,
    ) {
        let th = theme::current();
        let p = &th.palette;
        let selected = self.st.selected_item.get().as_deref() == Some(self.item.id.as_str());
        let (x, y, w, h) = (
            bounds.x as f32,
            bounds.y as f32,
            bounds.w as f32,
            bounds.h as f32,
        );
        let radius = th.metrics.corner_md;

        // 背景层：选中 > 按下 > 悬停
        if selected {
            let bg = Color::rgba(p.accent.r, p.accent.g, p.accent.b, SELECTED_ALPHA);
            canvas.fill_round_rect(x, y, w, h, radius, &Paint::fill(bg));
        } else if self.pressed {
            let bg = Color::rgba(p.accent.r, p.accent.g, p.accent.b, PRESSED_ALPHA);
            canvas.fill_round_rect(x, y, w, h, radius, &Paint::fill(bg));
        } else if self.hovered {
            canvas.fill_round_rect(
                x,
                y,
                w,
                h,
                radius,
                &Paint::fill(Role::SurfaceAlt.resolve(&th)),
            );
        }

        // 图标：顶部居中
        let icon_size = self.st.settings_current().icon_size.icon_size() as i32;
        let cx = content.x + content.w / 2;
        let cy = content.y + icon_size / 2 + ICON_GAP_Y;
        let icon_rect = Rect::new(cx - icon_size / 2, cy - icon_size / 2, icon_size, icon_size);
        self.item
            .icon
            .paint_into(icon_rect, canvas, style, VisualState::Normal);

        // 名称：底部居中（显示标签时）
        if self.st.settings_current().show_labels {
            let name_color = if selected {
                p.accent
            } else {
                Role::Text.resolve(&th)
            };
            let name_rect = Rect::new(
                content.x,
                content.y + icon_size + NAME_TOP,
                content.w,
                content.h - icon_size - NAME_BOTTOM,
            );
            let mut ts = TextStyle::of(style);
            ts.size = th.metrics.font_sm - 1.0;
            canvas.save();
            canvas.clip_rect(name_rect);
            canvas.draw_text(&self.item.name, name_rect, name_color, Align::Center, &ts);
            canvas.restore();
        }

        // 聚焦边框
        if focused {
            canvas.stroke_round_rect(
                x + 0.5,
                y + 0.5,
                w - 1.0,
                h - 1.0,
                radius,
                1.0,
                &Paint::fill(p.accent),
            );
        }
    }

    fn on_event(&mut self, ctx: &mut EventCtx, ev: &Event) -> bool {
        match ev {
            Event::Pointer(PointerEvent {
                kind,
                button,
                click_count,
                ..
            }) => match kind {
                PointerKind::Down => match button {
                    MouseButton::Left => {
                        self.pressed = true;
                        ctx.mark_dirty();
                        self.st.select_item(&self.item.id);
                        let start = match self.st.settings_current().activation {
                            // 单击模式：快速双击的第二个 Down 不再启动，防重复启动。
                            Activation::Single => *click_count < 2,
                            Activation::Double => *click_count >= 2,
                        };
                        if start {
                            self.launch(ctx, &self.item.id);
                        }
                        true
                    }
                    MouseButton::Right => {
                        // 右键先选中当前卡片，让容器菜单作用于它；不消费，冒泡给父容器菜单。
                        self.st.select_item(&self.item.id);
                        ctx.mark_dirty();
                        false
                    }
                    _ => false,
                },
                PointerKind::Up => {
                    self.pressed = false;
                    ctx.mark_dirty();
                    true
                }
                PointerKind::Enter => {
                    self.hovered = true;
                    ctx.mark_dirty();
                    true
                }
                PointerKind::Leave => {
                    self.hovered = false;
                    self.pressed = false;
                    ctx.mark_dirty();
                    true
                }
                _ => false,
            },
            Event::Key(KeyEvent {
                key,
                pressed: true,
                ..
            }) => match key {
                Key::Enter => {
                    if let Some(id) = self.st.selected_item.get() {
                        self.launch(ctx, &id);
                    }
                    true
                }
                Key::Delete => {
                    self.st.delete_selected();
                    true
                }
                Key::Other(vk) if *vk == KEY_F2 => {
                    // F2：编辑属性（作用于当前选中项）。
                    if let Some(id) = self.st.selected_item.get() {
                        self.st.open_item_props(&id);
                    }
                    true
                }
                Key::Left | Key::Right | Key::Up | Key::Down => {
                    let cols = self.st.settings_current().icon_size.cols();
                    let (dx, dy) = match key {
                        Key::Left => (-1, 0),
                        Key::Right => (1, 0),
                        Key::Up => (0, -1),
                        _ => (0, 1),
                    };
                    self.st.move_selection(cols, dx, dy);
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn wants_right_click(&self) -> bool {
        true
    }

    fn cursor(&self) -> windui::event::CursorShape {
        windui::event::CursorShape::Hand
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

fn card_element(item: ItemView, st: &Rc<State>) -> Element {
    let card = ItemCard::new(item, st.clone());
    Element::leaf().widget(card).weight(1.0)
}

/// 图标网格：数据源 signal 变化自动重建，行内卡片 weight 均分列宽。
pub fn grid_element(st: &Rc<State>) -> Element {
    let st_rows = st.clone();
    let list = Element::list_signal(
        st.grid,
        |row: &Vec<ItemView>| row.first().map(|v| v.id.clone()).unwrap_or_default(),
        move |row: Vec<ItemView>| {
            let cell = st_rows.settings_current().icon_size.cell_size();
            let mut r = Element::row().fill().spacing(12).cross(Align::Start);
            for item in row {
                r = r.child(card_element(item, &st_rows).height(cell as i32));
            }
            r
        },
    );
    let menu_st = st.clone();
    list.on_context_menu(move || build_item_menu(&menu_st))
}

/// 图标区右键菜单：作用于当前选中项。
pub fn build_item_menu(st: &Rc<State>) -> Vec<MenuItem> {
    let Some(id) = st.selected_item.get() else {
        let s = st.clone();
        return vec![MenuItem::run(
            "新增快捷方式",
            move || s.open_add_item_dialog(),
            false,
        )];
    };
    let Some(item) = st.find_item(&id) else {
        return vec![];
    };
    let s_launch = st.clone();
    let item_for_launch = item.clone();
    let s_props = st.clone();
    let id_props = id.clone();
    let s_rename = st.clone();
    let id_rename = id.clone();
    let s_up = st.clone();
    let s_down = st.clone();
    let s_del = st.clone();
    let s_add = st.clone();
    let s_move = st.clone();
    let id_move = id.clone();
    // 「移动到」子菜单：列出除当前集合外的所有集合。
    let move_targets: Vec<MenuItem> = st
        .locate(&id)
        .and_then(|(ci, _)| {
            st.data()
                .borrow()
                .collections
                .get(ci)
                .map(|c| c.id.clone())
        })
        .map(|scid| {
            st.collections
                .get()
                .into_iter()
                .filter(|cv| cv.id != scid)
                .map(|cv| {
                    let s = s_move.clone();
                    let cid = cv.id.clone();
                    let iid = id_move.clone();
                    MenuItem::run(
                        cv.name,
                        move || s.move_item_to_collection(&cid, &iid),
                        false,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let mut v = vec![
        MenuItem::run(
            "启动",
            move || {
                if let Err(e) = launch::launch_item(&item_for_launch) {
                    s_launch.set_error(&e);
                }
            },
            false,
        ),
        MenuItem::separator(),
        MenuItem::run("重命名", move || s_rename.open_item_props(&id_rename), false),
        MenuItem::run("属性", move || s_props.open_item_props(&id_props), false),
    ];
    if !move_targets.is_empty() {
        v.push(MenuItem::submenu("移动到", move_targets));
    }
    v.push(MenuItem::run("上移", move || s_up.move_selected(true), false));
    v.push(MenuItem::run("下移", move || s_down.move_selected(false), false));
    v.push(MenuItem::separator());
    v.push(MenuItem::run("删除", move || s_del.delete_selected(), false));
    v.push(MenuItem::separator());
    v.push(MenuItem::run(
        "新增快捷方式",
        move || s_add.open_add_item_dialog(),
        false,
    ));
    v
}

/// 空状态：无图标时展示提示（搜索场景显示"无匹配结果"）。
fn empty_state_element(st: &Rc<State>) -> Element {
    let s = st.clone();
    let s_empty = st.clone();
    let s_search = st.clone();
    Element::col()
        .fill()
        .cross(Align::Center)
        .spacing(10)
        .child(Element::label("🎯").font_size(40.0).height(48))
        .child(
            Element::stack()
                .visible_when(move || s_empty.search.get().trim().is_empty())
                .child(Element::label("暂无快捷方式").fg_role(Role::TextMuted)),
        )
        .child(
            Element::label("或把文件拖到这里添加")
                .font_size(theme::current().metrics.font_sm - 1.0)
                .fg_role(Role::TextDisabled),
        )
        .child(
            Element::stack()
                .visible_when(move || !s_search.search.get().trim().is_empty())
                .child(Element::label("没有匹配的快捷方式").fg_role(Role::TextMuted)),
        )
        .child(
            Element::button("新增快捷方式")
                .small()
                .on_click(move |_ctx| s.open_add_item_dialog()),
        )
}

/// 图标网格面板：空集合显示空状态，否则显示网格。
/// 整个右栏区域接收文件拖放，把拖入的路径添加为当前集合的快捷方式。
pub fn item_panel(st: &Rc<State>) -> Element {
    let s_empty = st.clone();
    let s_grid = st.clone();
    let s_drop = st.clone();
    Element::stack()
        .fill()
        .weight(1.0)
        .on_drop_files(move |ctx, paths| {
            let (added, skipped) = s_drop.add_dropped_paths(paths);
            if added > 0 {
                ctx.toast_ok(format!("已添加 {added} 个快捷方式"));
            } else if skipped > 0 {
                ctx.toast_err("没有可添加的项");
            }
        })
        .child(
            Element::stack()
                .fill()
                .visible_when(move || {
                    let rows = s_empty.grid.get();
                    rows.iter().all(|r| r.is_empty())
                })
                .child(empty_state_element(st)),
        )
        .child(
            Element::stack()
                .fill()
                .visible_when(move || s_grid.grid.get().iter().any(|r| !r.is_empty()))
                .child(grid_element(st)),
        )
}
