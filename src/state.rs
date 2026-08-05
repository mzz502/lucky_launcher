use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use windui::prelude::{signal, Image, ImageContent, Sender, Signal};

use crate::model::{
    Activation, AppData, Collection, IconSize, Item, ItemKind, Settings,
};
use crate::{search, storage, win_utils};

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 解析 Windows 快捷方式文件（Internet Shortcut）：读取 `URL=` 行。
/// 兼容 ANSI / UTF-8 / UTF-16LE（带 BOM）三种编码。
fn parse_url_file(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let text = if bytes.starts_with(&[0xFF, 0xFE]) {
        let words: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&words)
    } else {
        String::from_utf8_lossy(&bytes).to_string()
    };
    text.lines()
        .find_map(|l| l.trim().strip_prefix("URL=").map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
}

pub const HOTKEYS: [&str; 3] = ["Ctrl+Q", "Alt+Space", "Ctrl+Alt+L"];

/// 图标网格单格数据（克隆廉价：图标为共享句柄）。
#[derive(Clone)]
pub struct ItemView {
    pub id: String,
    pub name: String,
    pub target: String,
    pub desc: String,
    pub icon: Rc<ImageContent>,
}

/// 集合列表单行数据。
#[derive(Clone)]
pub struct CollectionView {
    pub id: String,
    pub name: String,
    pub index: usize,
    pub count: usize,
}

pub enum DeleteTarget {
    Collection { id: String, name: String },
    Item { collection_id: String, id: String, name: String },
}

/// 后台图标提取任务。
struct IconJob {
    path: String,
    size: u32,
}

/// 后台图标提取结果（跨线程 Send，UI 线程回填缓存）。
pub struct IconResult {
    pub path: String,
    pub rgba: Option<(u32, u32, Vec<u8>)>,
}

pub struct State {
    data: Rc<RefCell<AppData>>,
    icon_cache: Rc<RefCell<HashMap<String, Rc<ImageContent>>>>,
    pending_delete: Rc<RefCell<Option<DeleteTarget>>>,
    last_autostart: Rc<RefCell<Option<bool>>>,
    /// 数据/集合列表
    pub collections: Signal<Vec<CollectionView>>,
    /// 图标网格（按行分块）
    pub grid: Signal<Vec<Vec<ItemView>>>,
    pub selected_collection: Signal<Option<usize>>,
    pub selected_item: Signal<Option<String>>,
    pub search: Signal<String>,
    pub settings: Signal<Settings>,
    /// 错误文本（驱动错误对话框，空串=无错误）
    pub save_error: Signal<String>,
    pub dlg_error: Signal<bool>,
    /// 删除确认对话框显示文本
    pub delete_text: Signal<String>,
    /// 对话框开关
    pub dlg_add_col: Signal<bool>,
    pub dlg_add_item: Signal<bool>,
    pub dlg_item_props: Signal<bool>,
    pub dlg_delete: Signal<bool>,
    pub dlg_settings: Signal<bool>,
    /// 导入确认对话框（导入会覆盖全部数据）
    pub dlg_import_confirm: Signal<bool>,
    /// 待确认导入的文件内容
    import_pending: Rc<RefCell<Option<String>>>,
    /// 设置控件绑定（变化由 UI 层轮询提交）
    pub theme_idx: Signal<usize>,
    pub act_idx: Signal<usize>,
    pub icon_idx: Signal<usize>,
    pub show_labels: Signal<bool>,
    pub hide_after: Signal<bool>,
    pub autostart: Signal<bool>,
    pub show_at_cursor: Signal<bool>,
    pub hotkey_idx: Signal<usize>,
    /// 表单输入信号（对话框）
    pub add_col_name: Signal<String>,
    pub add_item_name: Signal<String>,
    pub add_item_target: Signal<String>,
    pub edit_name: Signal<String>,
    pub edit_target: Signal<String>,
    pub edit_args: Signal<String>,
    pub edit_workdir: Signal<String>,
    pub edit_desc: Signal<String>,
    pub edit_kind_idx: Signal<usize>,
    /// 属性对话框当前编辑的图标 id
    edit_item_id: Rc<RefCell<String>>,
    /// 重命名集合对话框
    pub dlg_rename_col: Signal<bool>,
    rename_col_idx: Rc<RefCell<usize>>,
    /// 后台图标提取通道（None=未注册，get_icon 同步降级）
    icon_sender: RefCell<Option<Sender<Vec<IconResult>>>>,
    /// 图标磁盘缓存目录（None=默认 %APPDATA% 目录；测试可注入临时目录）
    icon_cache_dir: Rc<RefCell<Option<PathBuf>>>,
    /// 正在后台提取的路径（避免重复排队）
    icon_inflight: Rc<RefCell<HashSet<String>>>,
    /// 待提取队列（rebuild 末尾一次性派发）
    pending_extract: Rc<RefCell<Vec<IconJob>>>,
    /// 名字 → (全拼, 首字母) 预计算缓存，避免每次搜索击键重算
    pinyin_cache: Rc<RefCell<HashMap<String, (String, String)>>>,
}

impl State {
    pub fn new(data: AppData) -> Self {
        let s = data.settings.clone();
        let hotkey_idx = HOTKEYS.iter().position(|&h| h == s.global_hotkey).unwrap_or(0);
        let st = Self {
            collections: signal(Vec::new()),
            grid: signal(Vec::new()),
            selected_collection: signal(if data.collections.is_empty() {
                None
            } else {
                Some(0)
            }),
            selected_item: signal(None),
            search: signal(String::new()),
            settings: signal(s.clone()),
            save_error: signal(String::new()),
            dlg_error: signal(false),
            delete_text: signal(String::new()),
            dlg_add_col: signal(false),
            dlg_add_item: signal(false),
            dlg_item_props: signal(false),
            dlg_delete: signal(false),
            dlg_settings: signal(false),
            dlg_import_confirm: signal(false),
            import_pending: Rc::new(RefCell::new(None)),
            theme_idx: signal(if s.theme == "dark" { 1 } else { 0 }),
            act_idx: signal(match s.activation {
                Activation::Double => 0,
                Activation::Single => 1,
            }),
            icon_idx: signal(match s.icon_size {
                IconSize::Small => 0,
                IconSize::Medium => 1,
                IconSize::Large => 2,
            }),
            show_labels: signal(s.show_labels),
            hide_after: signal(s.hide_after_launch),
            autostart: signal(s.autostart),
            show_at_cursor: signal(s.show_at_cursor),
            hotkey_idx: signal(hotkey_idx),
            add_col_name: signal(String::new()),
            add_item_name: signal(String::new()),
            add_item_target: signal(String::new()),
            edit_name: signal(String::new()),
            edit_target: signal(String::new()),
            edit_args: signal(String::new()),
            edit_workdir: signal(String::new()),
            edit_desc: signal(String::new()),
            edit_kind_idx: signal(0),
            edit_item_id: Rc::new(RefCell::new(String::new())),
            dlg_rename_col: signal(false),
            rename_col_idx: Rc::new(RefCell::new(0)),
            data: Rc::new(RefCell::new(data)),
            icon_cache: Rc::new(RefCell::new(HashMap::new())),
            pending_delete: Rc::new(RefCell::new(None)),
            last_autostart: Rc::new(RefCell::new(None)),
            icon_sender: RefCell::new(None),
            icon_cache_dir: Rc::new(RefCell::new(None)),
            icon_inflight: Rc::new(RefCell::new(HashSet::new())),
            pending_extract: Rc::new(RefCell::new(Vec::new())),
            pinyin_cache: Rc::new(RefCell::new(HashMap::new())),
        };
        st.rebuild_lists();
        st.apply_autostart();
        st.prune_icon_cache();
        st
    }

    pub fn data(&self) -> Rc<RefCell<AppData>> {
        self.data.clone()
    }

    pub fn settings_current(&self) -> Settings {
        self.data.borrow().settings.clone()
    }

    pub fn set_error(&self, msg: &str) {
        self.save_error.set(msg.to_string());
        self.dlg_error.set(true);
    }

    fn save_and_rebuild(&self) {
        let err = {
            let data = self.data.borrow();
            storage::save(&data).err()
        };
        if let Some(e) = err {
            self.set_error(&e);
        }
        self.rebuild_lists();
    }

    pub fn rebuild_lists(&self) {
        let data = self.data.borrow();
        let search_text = self.search.get();
        let cols: Vec<CollectionView> = data
            .collections
            .iter()
            .enumerate()
            .map(|(i, c)| CollectionView {
                id: c.id.clone(),
                name: c.name.clone(),
                index: i,
                count: c.items.len(),
            })
            .collect();
        self.collections.set(cols);

        let cols_per_row = data.settings.icon_size.cols();
        let mut flat: Vec<ItemView> = Vec::new();
        if search_text.trim().is_empty() {
            if let Some(idx) = self.selected_collection.get() {
                if let Some(c) = data.collections.get(idx) {
                    for it in &c.items {
                        flat.push(self.to_item_view(it));
                    }
                }
            }
        } else {
            for c in &data.collections {
                for it in &c.items {
                    let (full, init) = self.pinyin_key(&it.name);
                    if search::matches_prekeyed(&search_text, &it.name, &it.target, &full, &init)
                    {
                        flat.push(self.to_item_view(it));
                    }
                }
            }
        }
        let rows: Vec<Vec<ItemView>> = flat
            .chunks(cols_per_row)
            .map(|ch| ch.to_vec())
            .collect();
        self.grid.set(rows);
        self.flush_icon_extract();
    }

    fn to_item_view(&self, it: &Item) -> ItemView {
        let src = if it.icon_path.is_empty() {
            it.target.clone()
        } else {
            it.icon_path.clone()
        };
        ItemView {
            id: it.id.clone(),
            name: it.name.clone(),
            target: it.target.clone(),
            desc: it.description.clone(),
            icon: self.get_icon(&src),
        }
    }

    /// 图标提取 + 缓存（按路径去重，失败回退占位）。
    /// 注册了后台通道后，未命中路径改为异步提取（先返回占位，到位后回填）。
    pub fn get_icon(&self, path: &str) -> Rc<ImageContent> {
        if path.trim().is_empty() {
            return Rc::new(ImageContent::new(None));
        }
        if let Some(ic) = self.icon_cache.borrow().get(path) {
            return ic.clone();
        }
        // 磁盘缓存命中（源文件 mtime 未变）→ 载入内存缓存，跳过系统提取。
        if let Some(ic) = self.load_icon_from_disk(path) {
            return ic;
        }
        if self.icon_sender.borrow().is_some() {
            // 异步路径：记入 in-flight 并排队，本帧先用占位。
            if !self.icon_inflight.borrow().contains(path) {
                self.icon_inflight.borrow_mut().insert(path.to_string());
                self.pending_extract
                    .borrow_mut()
                    .push(IconJob { path: path.to_string(), size: 32 });
            }
            return Rc::new(ImageContent::new(None));
        }
        match win_utils::extract_icon(path, 32) {
            Some(img) => {
                let ic = Rc::new(ImageContent::new(Some(img)));
                self.icon_cache.borrow_mut().insert(path.to_string(), ic.clone());
                ic
            }
            None => {
                // 失败也缓存占位，避免损坏/无权限文件在每次重建时重复提取。
                let ic = Rc::new(ImageContent::new(None));
                self.icon_cache.borrow_mut().insert(path.to_string(), ic.clone());
                ic
            }
        }
    }

    /// 指定磁盘图标缓存目录（默认 `%APPDATA%\LuckyLauncher\icon_cache`；测试注入临时目录）。
    pub fn set_icon_cache_dir(&self, dir: PathBuf) {
        *self.icon_cache_dir.borrow_mut() = Some(dir);
    }

    fn disk_cache_dir(&self) -> PathBuf {
        self.icon_cache_dir
            .borrow()
            .clone()
            .unwrap_or_else(crate::icon_cache::cache_dir)
    }

    /// 尝试从磁盘缓存加载图标（校验源文件修改时间）。未命中/失效返回 `None`。
    fn load_icon_from_disk(&self, path: &str) -> Option<Rc<ImageContent>> {
        let dir = self.disk_cache_dir();
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let (w, h, buf) = crate::icon_cache::load(&dir, path, mtime)?;
        let ic = Rc::new(ImageContent::new(Image::from_rgba(w, h, &buf).ok()));
        self.icon_cache.borrow_mut().insert(path.to_string(), ic.clone());
        Some(ic)
    }

    /// 当前所有条目对应的图标源路径（图标缓存路径优先，否则目标路径）。
    fn all_icon_paths(&self) -> Vec<String> {
        let data = self.data.borrow();
        let mut out = Vec::new();
        for c in &data.collections {
            for it in &c.items {
                out.push(if it.icon_path.is_empty() {
                    it.target.clone()
                } else {
                    it.icon_path.clone()
                });
            }
        }
        out
    }

    /// 清理磁盘缓存中已不存在的条目路径对应的缓存文件。
    pub fn prune_icon_cache(&self) {
        let active: HashSet<u64> = self
            .all_icon_paths()
            .iter()
            .map(|p| crate::icon_cache::key_for(p))
            .collect();
        crate::icon_cache::prune(&self.disk_cache_dir(), &active);
    }

    /// 注册后台图标提取通道。注册后 `get_icon` 未命中路径改为异步提取。
    pub fn set_icon_sender(&self, sender: Sender<Vec<IconResult>>) {
        *self.icon_sender.borrow_mut() = Some(sender);
    }

    /// 把排队中的提取任务派发到后台线程（每轮重建末尾调用一次）。
    fn flush_icon_extract(&self) {
        if self.icon_sender.borrow().is_none() {
            return;
        }
        let jobs: Vec<IconJob> = self.pending_extract.borrow_mut().drain(..).collect();
        if jobs.is_empty() {
            return;
        }
        let tx = self.icon_sender.borrow().clone().unwrap();
        let cache_dir = self.disk_cache_dir();
        std::thread::spawn(move || {
            let results: Vec<IconResult> = jobs
                .into_iter()
                .map(|j| {
                    let rgba = win_utils::extract_icon_rgba(&j.path, j.size);
                    // 提取成功即落盘，下次启动免于重复提取（mtime 作失效校验）。
                    if let Some((w, h, ref buf)) = rgba {
                        let mtime = std::fs::metadata(&j.path).and_then(|m| m.modified()).ok();
                        crate::icon_cache::save(&cache_dir, &j.path, mtime, w, h, buf);
                    }
                    IconResult { path: j.path.clone(), rgba }
                })
                .collect();
            let _ = tx.send(results);
        });
    }

    /// 后台提取结果回填缓存并清除 in-flight 标记。
    pub fn cache_icon(&self, path: &str, rgba: Option<(u32, u32, Vec<u8>)>) {
        let img = match rgba {
            Some((w, h, buf)) => Image::from_rgba(w, h, &buf).ok(),
            None => None,
        };
        let ic = Rc::new(ImageContent::new(img));
        self.icon_cache.borrow_mut().insert(path.to_string(), ic);
        self.icon_inflight.borrow_mut().remove(path);
    }

    /// 丢弃已缓存的图标并重建网格（配合后台通道，使首次展示走异步提取）。
    pub fn invalidate_icons_and_rebuild(&self) {
        self.icon_cache.borrow_mut().clear();
        self.icon_inflight.borrow_mut().clear();
        self.rebuild_lists();
    }

    /// 名字的拼音检索键，带缓存（同名字多次重建只算一次）。
    fn pinyin_key(&self, name: &str) -> (String, String) {
        if let Some(k) = self.pinyin_cache.borrow().get(name) {
            return k.clone();
        }
        let k = search::pinyin_keys(name);
        self.pinyin_cache.borrow_mut().insert(name.to_string(), k.clone());
        k
    }

    // ---------- 集合操作 ----------

    pub fn add_collection(&self, name: &str) {
        let name = name.trim().to_string();
        let mut created = false;
        let mut idx = 0usize;
        {
            let mut data = self.data.borrow_mut();
            if !name.is_empty() && !data.collections.iter().any(|c| c.name == name) {
                let c = Collection::new(&name);
                idx = data.collections.len();
                data.collections.push(c);
                created = true;
            }
        }
        if created {
            self.selected_collection.set(Some(idx));
            self.selected_item.set(None);
            self.save_and_rebuild();
        }
    }

    pub fn rename_collection(&self, index: usize, name: &str) {
        let name = name.trim().to_string();
        {
            let mut data = self.data.borrow_mut();
            if let Some(c) = data.collections.get_mut(index) {
                if !name.is_empty() {
                    c.name = name;
                    c.updated_at = storage::now_iso();
                }
            }
        }
        self.save_and_rebuild();
    }

    /// 删除集合；最后一个集合不允许删除（至少保留一个），失败返回 false。
    pub fn remove_collection(&self, index: usize) -> bool {
        if self.data.borrow().collections.len() <= 1 {
            self.set_error("至少保留一个集合");
            return false;
        }
        {
            let mut data = self.data.borrow_mut();
            if index < data.collections.len() {
                data.collections.remove(index);
            }
        }
        // 集合减少后选中钳制到有效范围。
        let sel = self.selected_collection.get().unwrap_or(0);
        let n = self.data.borrow().collections.len();
        let new_sel = if n == 0 { None } else { Some(sel.min(n - 1)) };
        self.selected_collection.set(new_sel);
        self.selected_item.set(None);
        self.save_and_rebuild();
        true
    }

    pub fn move_collection(&self, index: usize, up: bool) {
        let mut swapped = false;
        {
            let mut data = self.data.borrow_mut();
            let n = data.collections.len();
            let target = if up {
                index.checked_sub(1)
            } else {
                (index + 1 < n).then_some(index + 1)
            };
            if let Some(t) = target {
                if t < n {
                    data.collections.swap(index, t);
                    swapped = true;
                }
            }
        }
        if swapped {
            let t = if up { index - 1 } else { index + 1 };
            self.selected_collection.set(Some(t));
            self.save_and_rebuild();
        }
    }

    // ---------- 图标操作 ----------

    /// 判断指定集合内是否存在同名快捷方式（可排除指定 id，用于改名场景）。
    fn has_item_name(&self, collection: usize, name: &str, exclude_id: Option<&str>) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let data = self.data.borrow();
        data.collections
            .get(collection)
            .map(|c| {
                c.items
                    .iter()
                    .any(|i| i.name == name && exclude_id.is_none_or(|e| i.id != e))
            })
            .unwrap_or(false)
    }

    pub fn add_item(&self, collection: usize, name: &str, target: &str) -> Option<Item> {
        let name = name.trim().to_string();
        let target = target.trim().to_string();
        if name.is_empty() || target.is_empty() {
            return None;
        }
        if self.has_item_name(collection, &name, None) {
            return None;
        }
        let mut created: Option<Item> = None;
        {
            let mut data = self.data.borrow_mut();
            if let Some(c) = data.collections.get_mut(collection) {
                let mut it = Item::new(&name, &target);
                if let Some((t, args, wd)) = win_utils::parse_lnk(&target) {
                    if !t.is_empty() {
                        it.target = t;
                        // 目标已解析为真实路径，按它重检类型（.lnk 本身会被判为 File）。
                        it.kind = ItemKind::detect(&it.target);
                        // 新建条目 args/working_dir 本就为空，直接填入解析出的参数。
                        it.args = args;
                        it.working_dir = wd;
                    }
                }
                if let Some(kind) = detect_folder(&it.target) {
                    it.kind = kind;
                }
                let now = storage::now_iso();
                c.items.push(it.clone());
                c.updated_at = now;
                created = Some(it);
            }
        }
        if let Some(it) = created {
            self.selected_item.set(Some(it.id.clone()));
            self.save_and_rebuild();
            Some(it)
        } else {
            None
        }
    }

    /// 拖放文件添加：把拖入的路径逐个转成快捷方式，加入当前选中集合。
    /// `.lnk` 由 `add_item` 内部解析目标；`.url` 解析出真实网址；
    /// 目录识别为文件夹类型。返回（成功数, 跳过数）。
    pub fn add_dropped_paths(&self, paths: &[PathBuf]) -> (usize, usize) {
        if paths.is_empty() {
            return (0, 0);
        }
        let Some(collection) = self.selected_collection.get() else {
            return (0, 0);
        };
        let mut added = 0;
        let mut skipped = 0;
        for p in paths {
            let name = if p.is_dir() {
                p.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            } else {
                p.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            };
            let is_url_file = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("url"))
                .unwrap_or(false);
            let target = if is_url_file {
                parse_url_file(p).unwrap_or_else(|| p.to_string_lossy().to_string())
            } else {
                p.to_string_lossy().to_string()
            };
            if self.add_item(collection, &name, &target).is_some() {
                added += 1;
            } else {
                skipped += 1;
            }
        }
        (added, skipped)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_item(
        &self,
        collection: usize,
        id: &str,
        name: &str,
        target: &str,
        args: &str,
        working_dir: &str,
        desc: &str,
        kind: ItemKind,
    ) {
        {
            let mut data = self.data.borrow_mut();
            if let Some(c) = data.collections.get_mut(collection) {
                if let Some(it) = c.items.iter_mut().find(|it| it.id == id) {
                    it.name = name.trim().to_string();
                    it.target = target.trim().to_string();
                    it.args = args.to_string();
                    it.working_dir = working_dir.to_string();
                    it.description = desc.to_string();
                    it.kind = kind;
                    it.updated_at = storage::now_iso();
                }
            }
        }
        self.save_and_rebuild();
    }

    pub fn remove_item(&self, collection: usize, id: &str) {
        {
            let mut data = self.data.borrow_mut();
            if let Some(c) = data.collections.get_mut(collection) {
                c.items.retain(|it| it.id != id);
            }
        }
        if self.selected_item.get().as_deref() == Some(id) {
            self.selected_item.set(None);
        }
        self.save_and_rebuild();
    }

    pub fn move_item(&self, collection: usize, id: &str, up: bool) {
        let mut moved = false;
        {
            let mut data = self.data.borrow_mut();
            if let Some(c) = data.collections.get_mut(collection) {
                if let Some(pos) = c.items.iter().position(|it| it.id == id) {
                    let target = if up {
                        pos.checked_sub(1)
                    } else {
                        (pos + 1 < c.items.len()).then_some(pos + 1)
                    };
                    if let Some(t) = target {
                        c.items.swap(pos, t);
                        moved = true;
                    }
                }
            }
        }
        if moved {
            self.save_and_rebuild();
        }
    }

    /// 把快捷方式移动到其他集合：从源集合移除，追加到目标集合末尾，
    /// 选中跳到目标集合并高亮该快捷方式。
    pub fn move_item_to_collection(&self, target_collection: &str, item_id: &str) {
        let target_ci = {
            let mut data = self.data.borrow_mut();
            let Some(source_ci) = data
                .collections
                .iter()
                .position(|c| c.items.iter().any(|i| i.id == item_id))
            else {
                return;
            };
            if data.collections[source_ci].id == target_collection {
                return;
            }
            let Some(ti) = data
                .collections
                .iter()
                .position(|c| c.id == target_collection)
            else {
                return;
            };
            let Some(ii) = data.collections[source_ci]
                .items
                .iter()
                .position(|i| i.id == item_id)
            else {
                return;
            };
            let mut item = data.collections[source_ci].items.remove(ii);
            item.updated_at = storage::now_iso();
            data.collections[ti].items.push(item);
            Some(ti)
        };
        if let Some(ti) = target_ci {
            self.select_collection(ti);
        }
        self.selected_item.set(Some(item_id.to_string()));
        self.save_and_rebuild();
    }

    // ---------- 选择 ----------

    pub fn select_collection(&self, index: usize) {
        if self.selected_collection.get() != Some(index) {
            self.selected_collection.set(Some(index));
            self.selected_item.set(None);
            self.rebuild_lists();
        }
    }

    pub fn select_item(&self, id: &str) {
        self.selected_item.set(Some(id.to_string()));
    }

    /// 定位条目所在的 (集合索引, 条目索引)。
    pub fn locate(&self, id: &str) -> Option<(usize, usize)> {
        let data = self.data.borrow();
        for (ci, c) in data.collections.iter().enumerate() {
            if let Some(ii) = c.items.iter().position(|it| it.id == id) {
                return Some((ci, ii));
            }
        }
        None
    }

    pub fn find_item(&self, id: &str) -> Option<Item> {
        let data = self.data.borrow();
        for c in &data.collections {
            if let Some(it) = c.items.iter().find(|it| it.id == id) {
                return Some(it.clone());
            }
        }
        None
    }

    /// 方向键移动选中：dx 横向（±1）、dy 纵向（±cols）。
    pub fn move_selection(&self, cols: usize, dx: isize, dy: isize) {
        let Some(id) = self.selected_item.get() else { return };
        let Some((ci, ii)) = self.locate(&id) else { return };
        let data = self.data.borrow();
        let n = data.collections[ci].items.len();
        let next = if dy != 0 {
            let target = ii as isize + dy * cols as isize;
            (target >= 0 && target < n as isize).then_some(target as usize)
        } else {
            let target = ii as isize + dx;
            (target >= 0 && target < n as isize).then_some(target as usize)
        };
        drop(data);
        if let Some(t) = next {
            let new_id = self
                .data
                .borrow()
                .collections
                .get(ci)
                .and_then(|c| c.items.get(t))
                .map(|it| it.id.clone());
            if let Some(nid) = new_id {
                self.selected_item.set(Some(nid));
            }
        }
    }

    /// 请求删除当前选中的图标（走确认对话框）。
    pub fn delete_selected(&self) {
        let Some(id) = self.selected_item.get() else { return };
        let Some((ci, _)) = self.locate(&id) else { return };
        let collection_id = self
            .data
            .borrow()
            .collections
            .get(ci)
            .map(|c| c.id.clone())
            .unwrap_or_default();
        let name = self.find_item(&id).map(|i| i.name).unwrap_or_default();
        self.request_delete(DeleteTarget::Item {
            collection_id,
            id,
            name,
        });
    }

    /// 上移/下移当前选中的图标。
    pub fn move_selected(&self, up: bool) {
        let Some(id) = self.selected_item.get() else { return };
        let Some((collection, _)) = self.locate(&id) else { return };
        self.move_item(collection, &id, up);
    }

    // ---------- 对话框 ----------

    pub fn open_add_col_dialog(&self) {
        self.add_col_name.set(String::new());
        self.dlg_add_col.set(true);
    }

    pub fn commit_add_col(&self) {
        let name = self.add_col_name.get();
        if name.trim().is_empty() {
            self.set_error("集合名称不能为空");
            return; // 不关闭对话框，让用户重新输入
        }
        self.dlg_add_col.set(false);
        self.add_collection(&name);
    }

    pub fn open_rename_col_dialog(&self, index: usize) {
        let name = self
            .data
            .borrow()
            .collections
            .get(index)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        self.add_col_name.set(name);
        *self.rename_col_idx.borrow_mut() = index;
        self.dlg_rename_col.set(true);
    }

    pub fn commit_rename_col(&self) {
        let index = *self.rename_col_idx.borrow();
        let name = self.add_col_name.get();
        self.dlg_rename_col.set(false);
        self.rename_collection(index, &name);
    }

    pub fn open_add_item_dialog(&self) {
        self.add_item_name.set(String::new());
        self.add_item_target.set(String::new());
        self.dlg_add_item.set(true);
    }

    pub fn commit_add_item(&self) -> bool {
        let collection = self.selected_collection.get().unwrap_or(0);
        let mut name = self.add_item_name.get();
        let target = self.add_item_target.get().trim().to_string();
        if name.trim().is_empty() {
            if let Some(f) = std::path::Path::new(&target).file_name() {
                name = f.to_string_lossy().to_string();
            }
        }
        if self.has_item_name(collection, &name, None) {
            self.set_error("集合中已存在同名快捷方式");
            return false;
        }
        if self.add_item(collection, &name, &target).is_none() {
            self.set_error("无法添加：名称或路径不能为空");
            return false;
        }
        self.dlg_add_item.set(false);
        true
    }

    pub fn open_item_props(&self, id: &str) {
        let Some(item) = self.find_item(id) else { return };
        *self.edit_item_id.borrow_mut() = id.to_string();
        self.edit_name.set(item.name.clone());
        self.edit_target.set(item.target.clone());
        self.edit_args.set(item.args.clone());
        self.edit_workdir.set(item.working_dir.clone());
        self.edit_desc.set(item.description.clone());
        self.edit_kind_idx
            .set(item.kind.index());
        self.dlg_item_props.set(true);
    }

    pub fn commit_item_props(&self) {
        let id = self.edit_item_id.borrow().clone();
        let Some((collection, _)) = self.locate(&id) else {
            self.dlg_item_props.set(false);
            return;
        };
        let kind = ItemKind::from_index(self.edit_kind_idx.get());
        let name = self.edit_name.get();
        let target = self.edit_target.get();
        if name.trim().is_empty() || target.trim().is_empty() {
            self.set_error("名称与目标路径不能为空");
            return;
        }
        if self.has_item_name(collection, &name, Some(&id)) {
            self.set_error("集合中已存在同名快捷方式");
            return;
        }
        self.dlg_item_props.set(false);
        self.update_item(
            collection,
            &id,
            &name,
            &target,
            &self.edit_args.get(),
            &self.edit_workdir.get(),
            &self.edit_desc.get(),
            kind,
        );
    }

    // ---------- 删除确认 ----------

    pub fn request_delete(&self, target: DeleteTarget) {
        let text = match &target {
            DeleteTarget::Collection { name, .. } => format!("确定删除集合「{name}」及其中的全部快捷方式吗？"),
            DeleteTarget::Item { name, .. } => format!("确定删除快捷方式「{name}」吗？"),
        };
        self.delete_text.set(text);
        *self.pending_delete.borrow_mut() = Some(target);
        self.dlg_delete.set(true);
    }

    pub fn confirm_delete(&self) {
        let pending = self.pending_delete.borrow_mut().take();
        match pending {
            Some(DeleteTarget::Collection { id, .. }) => {
                // 按 id 定位，确认前发生移动/增删也不会误删。
                // 两段式借用：先在块内定位取 index 并释放借用，再执行删除——
                // if-let 条件里的临时借用会覆盖整个块体，块体内再 borrow_mut 会 panic。
                let index = {
                    let data = self.data.borrow();
                    data.collections.iter().position(|c| c.id == id)
                };
                if let Some(index) = index {
                    // 最后一个集合删除失败时错误对话框已弹出，此处忽略返回值。
                    let _ = self.remove_collection(index);
                }
            }
            Some(DeleteTarget::Item {
                collection_id,
                id,
                ..
            }) => {
                let collection = {
                    let data = self.data.borrow();
                    data.collections.iter().position(|c| c.id == collection_id)
                };
                if let Some(collection) = collection {
                    self.remove_item(collection, &id);
                }
            }
            None => {}
        }
        self.dlg_delete.set(false);
    }

    // ---------- 设置 ----------

    /// 读取设置控件绑定信号，若有变化则写入模型并保存。返回新设置（UI 层据此切主题/改热键）。
    pub fn sync_settings(&self) -> Option<Settings> {
        let mut data = self.data.borrow_mut();
        let s = &mut data.settings;
        let new_act = if self.act_idx.get() == 1 {
            Activation::Single
        } else {
            Activation::Double
        };
        let new_icon = match self.icon_idx.get() {
            0 => IconSize::Small,
            1 => IconSize::Medium,
            _ => IconSize::Large,
        };
        let new_theme = if self.theme_idx.get() == 1 { "dark" } else { "light" };
        let hotkey = HOTKEYS
            .get(self.hotkey_idx.get())
            .copied()
            .unwrap_or("Ctrl+Q");
        let changed = s.activation != new_act
            || s.icon_size != new_icon
            || s.theme != new_theme
            || s.hide_after_launch != self.hide_after.get()
            || s.show_labels != self.show_labels.get()
            || s.autostart != self.autostart.get()
            || s.show_at_cursor != self.show_at_cursor.get()
            || s.global_hotkey != hotkey;
        if !changed {
            return None;
        }
        s.activation = new_act;
        s.icon_size = new_icon;
        s.theme = new_theme.to_string();
        s.hide_after_launch = self.hide_after.get();
        s.show_labels = self.show_labels.get();
        s.autostart = self.autostart.get();
        s.show_at_cursor = self.show_at_cursor.get();
        s.global_hotkey = hotkey.to_string();
        drop(data);
        self.settings.set(self.data.borrow().settings.clone());
        self.save_and_rebuild();
        self.apply_autostart();
        Some(self.data.borrow().settings.clone())
    }

    /// 持久化窗口尺寸（设置页「保存当前窗口大小」触发）。尺寸未变化不写盘。
    pub fn save_win_size(&self, w: i32, h: i32) {
        let changed = {
            let mut data = self.data.borrow_mut();
            if data.settings.win_w == w && data.settings.win_h == h {
                false
            } else {
                data.settings.win_w = w;
                data.settings.win_h = h;
                true
            }
        };
        if changed {
            let err = {
                let data = self.data.borrow();
                storage::save(&data).err()
            };
            if let Some(e) = err {
                self.set_error(&e);
            }
        }
    }

    fn apply_autostart(&self) {
        let want = self.data.borrow().settings.autostart;
        let last = *self.last_autostart.borrow();
        if last == Some(want) {
            return;
        }
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if exe.is_empty() {
            *self.last_autostart.borrow_mut() = Some(want);
            return;
        }
        if let Err(e) = win_utils::set_autostart(want, &exe) {
            self.set_error(&e);
        } else {
            // 仅成功才记录，瞬态失败下次轮询会重试。
            *self.last_autostart.borrow_mut() = Some(want);
        }
    }

    // ---------- 导入 / 导出 ----------

    /// 导入前先弹出确认框（导入会覆盖全部数据）。读取文件内容暂存，
    /// 待用户确认后才真正解析并应用。
    pub fn request_import(&self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                *self.import_pending.borrow_mut() = Some(content);
                self.dlg_import_confirm.set(true);
            }
            Err(e) => self.set_error(&format!("无法读取文件：{e}")),
        }
    }

    pub fn confirm_import(&self) {
        let Some(content) = self.import_pending.borrow_mut().take() else {
            return;
        };
        match serde_json::from_str::<AppData>(&content) {
            Ok(mut appdata) => {
                appdata.sanitize();
                if appdata.collections.is_empty() {
                    self.set_error("导入文件中没有有效的集合数据");
                    return;
                }
                self.apply_import(appdata);
                self.dlg_import_confirm.set(false);
            }
            Err(e) => self.set_error(&format!("文件不是有效的 JSON：{e}")),
        }
    }

    /// 用导入的数据整体替换当前数据并同步所有 UI 信号。
    fn apply_import(&self, appdata: AppData) {
        let s = appdata.settings.clone();
        *self.data.borrow_mut() = appdata;
        self.icon_cache.borrow_mut().clear();
        self.search.set(String::new());
        self.selected_collection.set(Some(0));
        self.selected_item.set(None);
        self.theme_idx
            .set(if s.theme == "dark" { 1 } else { 0 });
        self.act_idx.set(match s.activation {
            Activation::Double => 0,
            Activation::Single => 1,
        });
        self.icon_idx.set(match s.icon_size {
            IconSize::Small => 0,
            IconSize::Medium => 1,
            IconSize::Large => 2,
        });
        self.show_labels.set(s.show_labels);
        self.hide_after.set(s.hide_after_launch);
        self.autostart.set(s.autostart);
        self.hotkey_idx
            .set(HOTKEYS.iter().position(|&h| h == s.global_hotkey).unwrap_or(0));
        self.save_and_rebuild();
        self.apply_autostart();
    }

    pub fn export(&self, path: &str) {
        let data = self.data.borrow();
        if let Err(e) = storage::export_data(Path::new(path), &data) {
            self.set_error(&e);
        }
    }
}

fn detect_folder(target: &str) -> Option<ItemKind> {
    let p = std::path::Path::new(target);
    if p.is_dir() {
        Some(ItemKind::Folder)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_data() -> AppData {
        let mut data = AppData::default();
        let mut c = Collection::new("开发");
        c.items.push(Item::new("Code", "C:/dev/code.exe"));
        data.collections.push(c);
        data
    }

    #[test]
    fn add_remove_collection() {
        let st = State::new(sample_data());
        assert_eq!(st.collections.get().len(), 1);
        st.add_collection("办公");
        assert_eq!(st.collections.get().len(), 2);
        assert_eq!(st.selected_collection.get(), Some(1));
        st.remove_collection(1);
        assert_eq!(st.collections.get().len(), 1);
    }

    #[test]
    fn grid_rows_by_icon_size() {
        let mut data = sample_data();
        data.settings.icon_size = IconSize::Small;
        for i in 0..12 {
            data.collections[0]
                .items
                .push(Item::new(format!("T{i}"), "C:/t.exe"));
        }
        let st = State::new(data);
        let rows = st.grid.get();
        assert!(rows.len() >= 2);
        assert!(rows[0].len() <= 8);
        // 全量条目被展示
        let total: usize = rows.iter().map(|r| r.len()).sum();
        assert_eq!(total, 13);
    }

    #[test]
    fn search_filters_global() {
        let mut data = sample_data();
        data.collections[0]
            .items
            .push(Item::new("微信", "C:/wechat.exe"));
        let st = State::new(data);
        st.search.set("wx".to_string());
        st.rebuild_lists();
        let rows = st.grid.get();
        let flat: Vec<_> = rows.into_iter().flatten().collect();
        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].name, "微信");
    }

    #[test]
    fn move_selection_wraps_boundary() {
        let st = State::new(sample_data());
        let id = st.data.borrow().collections[0].items[0].id.clone();
        st.selected_item.set(Some(id));
        // 边界外不动
        st.move_selection(6, 0, 1);
        let n = st.data.borrow().collections[0].items.len();
        assert_eq!(st.locate(&st.selected_item.get().unwrap()).unwrap().1, n - 1);
        st.move_selection(6, 0, -1);
        assert_eq!(st.locate(&st.selected_item.get().unwrap()).unwrap().1, 0);
    }

    #[test]
    fn add_dropped_paths_adds_items() {
        let dir = std::env::temp_dir().join(format!("lucky_drop_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("notepad.exe");
        std::fs::write(&exe, b"").unwrap();
        let folder = dir.join("MyFolder");
        std::fs::create_dir_all(&folder).unwrap();
        let url = dir.join("GitHub.url");
        std::fs::write(&url, "[InternetShortcut]\nURL=https://github.com\n").unwrap();

        let st = State::new(sample_data());
        let (added, skipped) = st.add_dropped_paths(&[exe.clone(), folder.clone(), url.clone()]);
        assert_eq!(added, 3);
        assert_eq!(skipped, 0);

        let items = &st.data.borrow().collections[0].items;
        assert_eq!(items.len(), 4);
        assert_eq!(items[1].name, "notepad");
        assert_eq!(items[1].kind, ItemKind::Application);
        assert_eq!(items[2].name, "MyFolder");
        assert_eq!(items[2].kind, ItemKind::Folder);
        assert_eq!(items[3].name, "GitHub");
        assert_eq!(items[3].target, "https://github.com");
        assert_eq!(items[3].kind, ItemKind::Url);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn click_collection_row_switches_selection() {
        use windui::core::Tree;
        use windui::event::{MouseButton, PointerEvent, PointerKind};
        use windui::geometry::{Point, Size};
        use windui::text::NullTextEngine;

        let mut data = AppData::default();
        data.collections.push(Collection::new("集合A"));
        data.collections.push(Collection::new("集合B"));
        let st = Rc::new(State::new(data));
        st.select_collection(1);
        assert_eq!(st.selected_collection.get(), Some(1));

        let ui = crate::ui::collection_panel::collection_panel(&st);
        let mut tree = Tree::new();
        let root = ui.build(&mut tree);
        tree.root = Some(root);
        let mut engine = NullTextEngine;
        tree.layout_root(Size::new(880, 600), &mut engine);

        let (mut hover, mut capture) = (None, None);
        let p = Point::new(100, 42);
        tree.dispatch_pointer(
            PointerEvent::single(PointerKind::Down, p, MouseButton::Left),
            &mut hover,
            &mut capture,
        );
        tree.dispatch_pointer(
            PointerEvent::single(PointerKind::Up, p, MouseButton::Left),
            &mut hover,
            &mut capture,
        );
        assert_eq!(st.selected_collection.get(), Some(0));
    }

    #[test]
    fn confirm_delete_item() {
        let mut data = sample_data();
        data.collections[0]
            .items
            .push(Item::new("微信", "C:/wechat.exe"));
        let st = State::new(data);
        let (col_id, id, name) = {
            let d = st.data.borrow();
            let it = &d.collections[0].items[1];
            (d.collections[0].id.clone(), it.id.clone(), it.name.clone())
        };
        st.request_delete(DeleteTarget::Item {
            collection_id: col_id,
            id: id.clone(),
            name,
        });
        st.confirm_delete();
        assert!(!st.dlg_delete.get());
        assert_eq!(st.data.borrow().collections[0].items.len(), 1);
        assert_eq!(st.data.borrow().collections[0].items[0].name, "Code");
    }

    #[test]
    fn confirm_delete_collection() {
        let mut data = sample_data();
        data.collections.push(Collection::new("办公"));
        let st = State::new(data);
        let col_id = st.data.borrow().collections[0].id.clone();
        st.request_delete(DeleteTarget::Collection {
            id: col_id,
            name: "开发".to_string(),
        });
        st.confirm_delete();
        assert_eq!(st.data.borrow().collections.len(), 1);
        assert_eq!(st.data.borrow().collections[0].name, "办公");
    }

    #[test]
    fn add_item_rejects_duplicate_name() {
        let st = State::new(sample_data());
        assert!(st.add_item(0, "Code", "C:/dev/code.exe").is_none());
        assert_eq!(st.data.borrow().collections[0].items.len(), 1);
    }

    #[test]
    fn add_dropped_paths_skips_duplicates() {
        let dir = std::env::temp_dir().join(format!("lucky_drop_dup_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let notepad = dir.join("Code.exe");
        std::fs::write(&notepad, b"").unwrap();
        let other = dir.join("other.exe");
        std::fs::write(&other, b"").unwrap();

        let st = State::new(sample_data());
        let (added, skipped) = st.add_dropped_paths(&[notepad.clone(), other.clone()]);
        assert_eq!(added, 1);
        assert_eq!(skipped, 1);
        assert_eq!(st.data.borrow().collections[0].items.len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn commit_item_props_rejects_rename_to_duplicate() {
        let mut data = sample_data();
        data.collections[0]
            .items
            .push(Item::new("微信", "C:/wechat.exe"));
        let st = State::new(data);
        let code_id = st.data.borrow().collections[0].items[0].id.clone();
        st.open_item_props(&code_id);
        st.edit_name.set("微信".to_string());
        st.commit_item_props();
        assert!(st.dlg_item_props.get());
        assert_eq!(st.data.borrow().collections[0].items[0].name, "Code");
    }

    #[test]
    fn lnk_target_kind_is_application() {
        use windows::core::{Interface, PCWSTR};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
            IPersistFile,
        };
        use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

        let dir = std::env::temp_dir().join(format!("lucky_lnk_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("demo.exe");
        std::fs::write(&exe, b"").unwrap();
        let lnk = dir.join("Demo.lnk");

        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            let exe_wide = crate::win_utils::to_wide(&exe.to_string_lossy());
            link.SetPath(PCWSTR(exe_wide.as_ptr())).unwrap();
            let args_wide = crate::win_utils::to_wide("--quick --max");
            link.SetArguments(PCWSTR(args_wide.as_ptr())).unwrap();
            let wd_wide = crate::win_utils::to_wide(&dir.to_string_lossy());
            link.SetWorkingDirectory(PCWSTR(wd_wide.as_ptr())).unwrap();
            let lnk_wide = crate::win_utils::to_wide(&lnk.to_string_lossy());
            link.cast::<IPersistFile>()
                .unwrap()
                .Save(PCWSTR(lnk_wide.as_ptr()), false)
                .unwrap();
        }

        let st = State::new(sample_data());
        let it = st.add_item(0, "Demo", &lnk.to_string_lossy()).unwrap();
        assert_eq!(it.kind, ItemKind::Application);
        assert!(it.target.ends_with("demo.exe"));
        assert_eq!(it.args, "--quick --max");
        assert_eq!(it.working_dir, dir.to_string_lossy());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_collection_keeps_last() {
        let st = State::new(sample_data());
        // 最后一个集合不可删。
        assert!(!st.remove_collection(0));
        assert_eq!(st.collections.get().len(), 1);
        // 有两个集合时可以删。
        st.add_collection("办公");
        assert!(st.remove_collection(1));
        assert_eq!(st.collections.get().len(), 1);
    }

    #[test]
    fn import_requires_confirm_and_applies() {
        let st = State::new(sample_data());
        let mut data = AppData::default();
        data.collections.push(Collection::new("导入集"));
        data.collections[0].items.push(Item::new("X", "C:/x.exe"));
        let json = serde_json::to_string(&data).unwrap();
        let path = std::env::temp_dir().join(format!("lucky_imp_{}.json", std::process::id()));
        std::fs::write(&path, &json).unwrap();

        st.request_import(&path.to_string_lossy());
        assert!(st.dlg_import_confirm.get());
        assert_eq!(st.collections.get()[0].name, "开发");

        st.confirm_import();
        assert!(!st.dlg_import_confirm.get());
        assert_eq!(st.collections.get().len(), 1);
        assert_eq!(st.collections.get()[0].name, "导入集");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn move_item_to_collection_moves_item() {
        let mut data = sample_data();
        data.collections.push(Collection::new("办公"));
        let st = State::new(data);
        let (code_id, office_id) = {
            let d = st.data.borrow();
            (d.collections[0].items[0].id.clone(), d.collections[1].id.clone())
        };
        st.move_item_to_collection(&office_id, &code_id);
        let d = st.data.borrow();
        assert_eq!(d.collections[0].items.len(), 0);
        assert_eq!(d.collections[1].items.len(), 1);
        assert_eq!(d.collections[1].items[0].id, code_id);
        drop(d);
        assert_eq!(st.selected_collection.get(), Some(1));
        assert_eq!(st.selected_item.get().as_deref(), Some(code_id.as_str()));
    }

    #[test]
    fn cache_icon_populates_and_clears_inflight() {
        let st = State::new(sample_data());
        st.icon_inflight.borrow_mut().insert("C:/x.exe".to_string());
        st.cache_icon("C:/x.exe", Some((2, 2, vec![0u8; 16])));
        assert!(st.icon_cache.borrow().contains_key("C:/x.exe"));
        assert!(st.icon_inflight.borrow().is_empty());
    }

    #[test]
    fn get_icon_loads_from_disk_cache() {
        // 预写磁盘缓存（真实源文件 + mtime），get_icon 应命中磁盘并载入内存缓存。
        let dir = std::env::temp_dir().join(format!("ll_state_icon_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("app.exe");
        std::fs::write(&src, b"fake exe bytes").unwrap();
        let path = src.to_string_lossy().to_string();
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).unwrap();
        crate::icon_cache::save(&dir, &path, Some(mtime), 32, 32, &vec![255u8; 32 * 32 * 4]);
        let st = State::new(sample_data());
        st.set_icon_cache_dir(dir.clone());
        let ic = st.get_icon(&path);
        assert!(ic.is_loaded(), "磁盘缓存命中应加载成功");
        // 二次调用命中内存缓存，与首次返回同一实例。
        assert!(Rc::ptr_eq(&ic, &st.get_icon(&path)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pinyin_key_caches() {
        let st = State::new(sample_data());
        let k1 = st.pinyin_key("微信");
        let k2 = st.pinyin_key("微信");
        assert_eq!(k1, k2);
        assert!(st.pinyin_cache.borrow().contains_key("微信"));
        assert!(!k1.0.is_empty() && !k1.1.is_empty());
    }
}
