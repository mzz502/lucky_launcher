use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::AppData;

pub fn appdata_dir() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("LuckyLauncher")
}

pub fn data_file() -> PathBuf {
    appdata_dir().join("data.json")
}

pub fn backup_file() -> PathBuf {
    appdata_dir().join("data.json.bak")
}

/// 载入数据：主文件损坏或读取失败时回退备份，均不可用则返回默认数据。
pub fn load() -> AppData {
    let file = data_file();
    match fs::read_to_string(&file) {
        Ok(text) => match serde_json::from_str::<AppData>(&text) {
            Ok(data) => data,
            Err(_) => recover_from_backup(),
        },
        Err(_) => load_backup(),
    }
}

fn load_backup() -> AppData {
    match fs::read_to_string(backup_file()) {
        Ok(text) => serde_json::from_str::<AppData>(&text).unwrap_or_default(),
        Err(_) => AppData::default(),
    }
}

/// 主文件损坏、备份可用时：回写备份到主文件（避免之后 save() 把损坏文件当作新备份覆盖唯一恢复源）。
fn recover_from_backup() -> AppData {
    let bak = backup_file();
    if let Ok(text) = fs::read_to_string(&bak) {
        if let Ok(data) = serde_json::from_str::<AppData>(&text) {
            let _ = fs::write(data_file(), &text);
            return data;
        }
    }
    AppData::default()
}

/// 保存前先备份旧文件，再写新文件。返回错误信息（成功为 Ok）。
pub fn save(data: &AppData) -> Result<(), String> {
    let dir = appdata_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        return Err(format!("创建目录失败：{e}"));
    }
    let file = data_file();
    let bak = backup_file();
    if file.exists() {
        // 备份失败提前报错，避免主文件随后损坏时丢失唯一恢复源
        fs::copy(&file, &bak).map_err(|e| format!("备份旧数据失败：{e}"))?;
    }
    let text = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    // 先写临时文件再原子替换（同目录同卷），避免写盘中途失败损坏主文件
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, &text).map_err(|e| format!("写入数据文件失败：{e}"))?;
    fs::rename(&tmp, &file).map_err(|e| format!("替换数据文件失败：{e}"))
}

pub fn export_data(path: &Path, data: &AppData) -> Result<(), String> {
    let text = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| format!("导出失败：{e}"))
}

pub fn import_data(path: &Path) -> Result<AppData, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("读取文件失败：{e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("数据格式不合法：{e}"))
}

/// 当前 UTC 时间（RFC3339 风格），用于 createdAt/updatedAt。
pub fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant 的 civil_from_days 算法（公历日 → 年月日）。
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_days_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(11_041), (2000, 3, 25));
    }

    #[test]
    fn now_iso_shape() {
        let s = now_iso();
        assert_eq!(s.len(), 20);
        assert!(s.ends_with('Z'));
        assert!(s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-');
    }
}
