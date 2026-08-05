use serde::{Deserialize, Serialize};

pub const DATA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppData {
    pub version: String,
    #[serde(default)]
    pub collections: Vec<Collection>,
    #[serde(default)]
    pub settings: Settings,
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            version: DATA_VERSION.to_string(),
            collections: Vec::new(),
            settings: Settings::default(),
        }
    }
}

impl AppData {
    /// 导入数据前的语义清洗：过滤空名称/空目标的条目与空集合，补齐/去重 id。
    pub fn sanitize(&mut self) {
        for c in &mut self.collections {
            c.items
                .retain(|it| !it.name.trim().is_empty() && !it.target.trim().is_empty());
        }
        self.collections.retain(|c| !c.name.trim().is_empty());
        let mut seen = std::collections::HashSet::new();
        for c in &mut self.collections {
            if c.id.is_empty() || !seen.insert(c.id.clone()) {
                c.id = crate::state::new_id();
            }
            seen.insert(c.id.clone());
            for it in &mut c.items {
                if it.id.is_empty() || !seen.insert(it.id.clone()) {
                    it.id = crate::state::new_id();
                }
                seen.insert(it.id.clone());
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

impl Collection {
    pub fn new(name: impl Into<String>) -> Self {
        let now = crate::storage::now_iso();
        Self {
            id: crate::state::new_id(),
            name: name.into(),
            items: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub name: String,
    pub target: String,
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub working_dir: String,
    #[serde(default)]
    pub icon_path: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub kind: ItemKind,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

impl Item {
    pub fn new(name: impl Into<String>, target: impl Into<String>) -> Self {
        let now = crate::storage::now_iso();
        let target = target.into();
        let kind = ItemKind::detect(&target);
        Self {
            id: crate::state::new_id(),
            name: name.into(),
            target,
            args: String::new(),
            working_dir: String::new(),
            icon_path: String::new(),
            description: String::new(),
            kind,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    #[default]
    Application,
    Folder,
    Url,
    File,
}

impl ItemKind {
    pub fn detect(target: &str) -> Self {
        let lower = target.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            return Self::Url;
        }
        if let Some(ext) = lower.rsplit('.').next() {
            if matches!(ext, "exe" | "com" | "bat" | "cmd" | "msi") {
                return Self::Application;
            }
        }
        // 目录判断放运行时（存在性），此处保守返回 File；win_utils 在添加时修正。
        Self::File
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Application => "应用程序",
            Self::Folder => "文件夹",
            Self::Url => "网址",
            Self::File => "文件",
        }
    }

    /// 全部类型，按 UI 下拉顺序排列。
    pub const ALL: [ItemKind; 4] = [Self::Application, Self::Folder, Self::Url, Self::File];

    /// 类型在 ALL 中的下标（UI 下拉索引）。
    pub fn index(self) -> usize {
        match self {
            Self::Application => 0,
            Self::Folder => 1,
            Self::Url => 2,
            Self::File => 3,
        }
    }

    /// 由 ALL 下标反查类型，越界回退 Application。
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Folder,
            2 => Self::Url,
            3 => Self::File,
            _ => Self::Application,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Activation {
    #[default]
    Double,
    Single,
}

impl Activation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Double => "双击启动",
            Self::Single => "单击启动",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconSize {
    #[default]
    Medium,
    Small,
    Large,
}

impl IconSize {
    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "小",
            Self::Medium => "中",
            Self::Large => "大",
        }
    }

    /// 卡片整体边长（逻辑像素）。
    pub fn cell_size(self) -> f32 {
        match self {
            Self::Small => 88.0,
            Self::Medium => 112.0,
            Self::Large => 140.0,
        }
    }

    /// 图标绘制边长。
    pub fn icon_size(self) -> f32 {
        match self {
            Self::Small => 36.0,
            Self::Medium => 48.0,
            Self::Large => 64.0,
        }
    }

    /// 每行列数（按网格区宽度估算）。
    pub fn cols(self) -> usize {
        match self {
            Self::Small => 8,
            Self::Medium => 6,
            Self::Large => 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub activation: Activation,
    #[serde(default)]
    pub hide_after_launch: bool,
    #[serde(default)]
    pub icon_size: IconSize,
    #[serde(default = "default_show_labels")]
    pub show_labels: bool,
    #[serde(default = "default_hotkey")]
    pub global_hotkey: String,
    #[serde(default)]
    pub autostart: bool,
    /// 上次保存的窗口客户区尺寸（逻辑 dp）。0 表示未保存，启动用默认 880×600。
    #[serde(default)]
    pub win_w: i32,
    #[serde(default)]
    pub win_h: i32,
    /// 呼出（隐藏/最小化恢复）时是否把窗口移到鼠标位置。
    #[serde(default)]
    pub show_at_cursor: bool,
}

fn default_theme() -> String {
    "light".to_string()
}
fn default_show_labels() -> bool {
    true
}
fn default_hotkey() -> String {
    "Ctrl+Q".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            activation: Activation::default(),
            hide_after_launch: false,
            icon_size: IconSize::default(),
            show_labels: default_show_labels(),
            global_hotkey: default_hotkey(),
            autostart: false,
            win_w: 0,
            win_h: 0,
            show_at_cursor: false,
        }
    }
}
