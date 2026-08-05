//! 图标磁盘缓存：避免每次启动重复调用 `SHGetFileInfoW` 提取图标。
//!
//! 缓存以原始 RGBA 像素落盘（无压缩、无额外依赖，进程内即用即取），
//! 文件头记录源文件修改时间（mtime）用于失效校验：mtime 未变则命中，
//! 变了或文件损坏则视为未命中，回退系统图标提取。

use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::storage::appdata_dir;

/// 缓存目录：`%APPDATA%\LuckyLauncher\icon_cache`。
pub fn cache_dir() -> PathBuf {
    appdata_dir().join("icon_cache")
}

/// 由路径计算确定性的缓存键（`DefaultHasher` 跨进程稳定，用于文件命名）。
pub fn key_for(path: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    h.finish()
}

/// 文件头：magic(4) + w(4) + h(4) + mtime_secs(8) + mtime_nanos(4)。
const MAGIC: &[u8; 4] = b"LLIC";
const HEADER_SIZE: usize = 4 + 4 + 4 + 8 + 4;

fn to_secs_nanos(t: SystemTime) -> (u64, u32) {
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => (d.as_secs(), d.subsec_nanos()),
        Err(_) => (0, 0),
    }
}

fn file_for(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("{key:016x}.rgba"))
}

/// 读取缓存。magic/长度/mtime 任一不符都返回 `None`（视为未命中，回退提取），绝不 panic。
pub fn load(dir: &Path, path: &str, want: Option<SystemTime>) -> Option<(u32, u32, Vec<u8>)> {
    let (wsecs, wnanos) = to_secs_nanos(want?);
    let bytes = fs::read(file_for(dir, key_for(path))).ok()?;
    if bytes.len() < HEADER_SIZE || &bytes[..4] != MAGIC {
        return None;
    }
    let w = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let h = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    let msecs = u64::from_le_bytes(bytes[12..20].try_into().ok()?);
    let mnanos = u32::from_le_bytes(bytes[20..24].try_into().ok()?);
    if msecs != wsecs || mnanos != wnanos {
        return None;
    }
    let data_len = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    if bytes.len() != HEADER_SIZE + data_len {
        return None;
    }
    Some((w, h, bytes[HEADER_SIZE..].to_vec()))
}

/// 写入缓存（临时文件 + rename 原子替换）。mtime 为 None（源文件已不可读）时不写。
pub fn save(
    dir: &Path,
    path: &str,
    mtime: Option<SystemTime>,
    w: u32,
    h: u32,
    rgba: &[u8],
) {
    let (secs, nanos) = match mtime {
        Some(t) => to_secs_nanos(t),
        None => return,
    };
    let Some(expected) = (w as usize).checked_mul(h as usize).and_then(|n| n.checked_mul(4)) else {
        return;
    };
    if rgba.len() != expected || fs::create_dir_all(dir).is_err() {
        return;
    }
    let mut buf = Vec::with_capacity(HEADER_SIZE + rgba.len());
    buf.extend_from_slice(MAGIC);
    buf.extend_from_slice(&w.to_le_bytes());
    buf.extend_from_slice(&h.to_le_bytes());
    buf.extend_from_slice(&secs.to_le_bytes());
    buf.extend_from_slice(&nanos.to_le_bytes());
    buf.extend_from_slice(rgba);
    let final_path = file_for(dir, key_for(path));
    let tmp = dir.join(format!("{}.tmp", final_path.file_name().unwrap_or_default().to_string_lossy()));
    if fs::write(&tmp, &buf).is_ok() {
        let _ = fs::rename(&tmp, &final_path);
    }
}

/// 清理缓存目录中不在 `active` 集合里的文件（启动时调用，防止孤儿文件堆积）。
pub fn prune(dir: &Path, active: &HashSet<u64>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(hex) = name.strip_suffix(".rgba") else {
            continue;
        };
        if let Ok(key) = u64::from_str_radix(hex, 16) {
            if !active.contains(&key) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ll_icon_cache_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_then_load_hits_same_mtime() {
        let dir = temp_dir("hit");
        let path = "C:/some/App.exe";
        let mtime = SystemTime::now();
        let rgba = vec![7u8; 16 * 16 * 4];
        save(&dir, path, Some(mtime), 16, 16, &rgba);
        let (w, h, out) = load(&dir, path, Some(mtime)).expect("mtime 一致应命中");
        assert_eq!((w, h), (16, 16));
        assert_eq!(out, rgba);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_returns_none_on_mtime_mismatch() {
        let dir = temp_dir("mismatch");
        let path = "C:/some/App.exe";
        save(&dir, path, Some(SystemTime::now()), 16, 16, &[1u8; 16 * 16 * 4]);
        // 源文件被改动后 mtime 不同 → 未命中，回退提取。
        let newer = SystemTime::now() + std::time::Duration::from_secs(60);
        assert!(load(&dir, path, Some(newer)).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_returns_none_on_corrupt_or_truncated() {
        let dir = temp_dir("corrupt");
        let path = "C:/some/App.exe";
        let key = key_for(path);
        // 非缓存内容
        fs::write(file_for(&dir, key), b"not a cache").unwrap();
        assert!(load(&dir, path, Some(SystemTime::now())).is_none());
        // 头部合法但长度不足（截断）
        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&32u32.to_le_bytes());
        buf.extend_from_slice(&32u32.to_le_bytes());
        buf.extend_from_slice(&1u64.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 4]); // 远小于 32*32*4
        fs::write(file_for(&dir, key), buf).unwrap();
        assert!(load(&dir, path, Some(SystemTime::now())).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_removes_inactive_files() {
        let dir = temp_dir("prune");
        save(&dir, "A.exe", Some(SystemTime::now()), 16, 16, &[0u8; 16 * 16 * 4]);
        save(&dir, "B.exe", Some(SystemTime::now()), 16, 16, &[0u8; 16 * 16 * 4]);
        fs::write(dir.join("junk.txt"), b"x").unwrap();
        let active: HashSet<u64> = [key_for("A.exe")].into();
        prune(&dir, &active);
        assert!(file_for(&dir, key_for("A.exe")).exists());
        assert!(!file_for(&dir, key_for("B.exe")).exists());
        assert!(dir.join("junk.txt").exists(), "非缓存文件不应被清理");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn key_is_deterministic() {
        assert_eq!(key_for("C:/x.exe"), key_for("C:/x.exe"));
        assert_ne!(key_for("C:/x.exe"), key_for("C:/y.exe"));
    }
}
