//! 清理模块（对标 CCleaner / CleanMyMac 常用能力）：
//! 文件垃圾（临时/回收站/缩略图/浏览器缓存/崩溃转储/WER/更新缓存/开发工具缓存）、
//! 注册表残留（MuiCache/卸载残留/App Paths/隐私痕迹）、启动项管理、大文件查找、DNS 刷新。
//! ★注册表删除前自动导出 .reg 备份；只清用户级（HKCU），HKLM 标注需管理员不擅动。

use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct CleanCategory {
    pub id: String,
    pub group: String, // file | reg
    pub title: String,
    pub detail: String,
    pub count: u64,
    pub bytes: u64,
}

#[derive(Clone, Serialize)]
pub struct CleanScan {
    pub categories: Vec<CleanCategory>,
}

#[derive(Clone, Serialize, Default)]
pub struct CleanReport {
    pub freed_bytes: u64,
    pub removed_files: u64,
    pub removed_reg_items: u64,
    pub backups: Vec<String>,
    pub errors: Vec<String>,
}

/// 清理互斥：并发触发（双窗口/连点）时第二次直接拒绝
static CLEAN_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct CleanGuard;

impl Drop for CleanGuard {
    fn drop(&mut self) {
        std::sync::atomic::AtomicBool::store(
            &CLEAN_RUNNING,
            false,
            std::sync::atomic::Ordering::SeqCst,
        );
    }
}

#[derive(Clone, Serialize)]
pub struct StartupItem {
    /// "hkcu-run:<名>" | "hklm-run:<名>" | "userfolder:<文件名>" | "commonfolder:<文件名>"
    pub id: String,
    pub source: String,
    pub name: String,
    pub cmd: String,
    pub admin: bool,
    /// 任务管理器里已被禁用（StartupApproved 记录）
    pub disabled: bool,
}

#[derive(Clone, Serialize)]
pub struct BigFile {
    pub path: String,
    pub size: u64,
    pub mtime: u64,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::fs;
    use std::os::windows::fs::MetadataExt;
    use std::path::{Path, PathBuf};
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    const REPARSE: u32 = 0x400; // FILE_ATTRIBUTE_REPARSE_POINT

    /// junction/symlink 目录（如 XP 兼容链）跳过，防循环
    fn is_reparse_dir(e: &std::fs::DirEntry) -> bool {
        e.metadata()
            .map(|md| md.file_attributes() & REPARSE != 0)
            .unwrap_or(false)
    }

    const MUI: &str = r"Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache";
    const UNINST: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    const APPPATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";
    const RECENTDOCS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\RecentDocs";
    const TYPEDPATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\TypedPaths";
    const OPENSAVE: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\ComDlg32\OpenSavePidlMRU";
    const DAY: u64 = 24 * 3600;

    // ===== 文件遍历 =====

    fn walk(path: &Path, min_age: u64, name_filter: Option<&str>, depth: u32) -> (u64, u64) {
        if depth == 0 {
            return (0, 0);
        }
        let Ok(rd) = fs::read_dir(path) else {
            return (0, 0);
        };
        let (mut n, mut bytes) = (0u64, 0u64);
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let p = e.path();
            if ft.is_dir() {
                if is_reparse_dir(&e) {
                    continue;
                }
                let (cn, cb) = walk(&p, min_age, name_filter, depth - 1);
                n += cn;
                bytes += cb;
            } else if ft.is_file() {
                if let Some(f) = name_filter {
                    if !p.to_string_lossy().to_lowercase().contains(f) {
                        continue;
                    }
                }
                let Ok(md) = e.metadata() else { continue };
                if min_age > 0 {
                    let Ok(m) = md.modified() else { continue };
                    if m.elapsed().map(|d| d.as_secs() < min_age).unwrap_or(true) {
                        continue;
                    }
                }
                n += 1;
                bytes += md.len();
            }
        }
        (n, bytes)
    }

    fn clean_walk(path: &Path, min_age: u64, name_filter: Option<&str>, depth: u32) -> (u64, u64) {
        if depth == 0 {
            return (0, 0);
        }
        let Ok(rd) = fs::read_dir(path) else {
            return (0, 0);
        };
        let (mut n, mut bytes) = (0u64, 0u64);
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let p = e.path();
            if ft.is_dir() {
                if is_reparse_dir(&e) {
                    continue;
                }
                let (cn, cb) = clean_walk(&p, min_age, name_filter, depth - 1);
                n += cn;
                bytes += cb;
            } else if ft.is_file() {
                if let Some(f) = name_filter {
                    if !p.to_string_lossy().to_lowercase().contains(f) {
                        continue;
                    }
                }
                let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                if min_age > 0 {
                    if let Ok(md) = e.metadata() {
                        if let Ok(m) = md.modified() {
                            if m.elapsed().map(|d| d.as_secs() < min_age).unwrap_or(true) {
                                continue;
                            }
                        }
                    }
                }
                if fs::remove_file(&p).is_ok() {
                    n += 1;
                    bytes += len;
                }
            }
        }
        (n, bytes)
    }

    fn scan_multi(dirs: &[PathBuf], min_age: u64) -> (u64, u64) {
        let (mut n, mut b) = (0u64, 0u64);
        for d in dirs {
            let (cn, cb) = walk(d, min_age, None, 6);
            n += cn;
            b += cb;
        }
        (n, b)
    }

    fn clean_multi(dirs: &[PathBuf], min_age: u64) -> (u64, u64) {
        let (mut n, mut b) = (0u64, 0u64);
        for d in dirs {
            let (cn, cb) = clean_walk(d, min_age, None, 6);
            n += cn;
            b += cb;
        }
        (n, b)
    }

    // ===== 回收站 =====

    #[repr(C)]
    struct ShQueryRbInfo {
        cb_size: u32,
        _pad: u32,
        i64_size: i64,
        i64_num_items: i64,
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SHQueryRecycleBinW(root: *const u16, info: *mut ShQueryRbInfo) -> i32;
        fn SHEmptyRecycleBinW(hwnd: usize, root: *const u16, flags: u32) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn recycle_stats() -> (u64, u64) {
        let (mut n, mut bytes) = (0u64, 0u64);
        for c in b'A'..=b'Z' {
            let root = format!("{}:\\", c as char);
            if !Path::new(&root).exists() {
                continue;
            }
            let mut info = ShQueryRbInfo {
                cb_size: std::mem::size_of::<ShQueryRbInfo>() as u32,
                _pad: 0,
                i64_size: 0,
                i64_num_items: 0,
            };
            if unsafe { SHQueryRecycleBinW(wide(&root).as_ptr(), &mut info) } == 0 {
                n += info.i64_num_items.max(0) as u64;
                bytes += info.i64_size.max(0) as u64;
            }
        }
        (n, bytes)
    }

    // ===== 注册表工具 =====

    fn expand_env(s: &str) -> String {
        let mut out = s.to_string();
        while let Some(a) = out.find('%') {
            let Some(rel) = out[a + 1..].find('%') else { break };
            let var = out[a + 1..a + 1 + rel].to_uppercase();
            match std::env::var(&var) {
                Ok(val) => {
                    out.replace_range(a..a + rel + 2, &val);
                    if val.contains('%') {
                        // 展开值里还有 %，到此为止避免死循环
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        out
    }

    /// 裸命令（如 winget/msiexec）在 PATH 上即视为有效
    fn command_on_path(cand: &str) -> bool {
        if cand.contains('\\') || cand.contains('/') {
            return false;
        }
        let Ok(path) = std::env::var("PATH") else {
            return false;
        };
        path.split(';').any(|dir| {
            !dir.is_empty()
                && (Path::new(dir).join(cand).is_file()
                    || Path::new(dir).join(format!("{cand}.exe")).is_file())
        })
    }

    /// 从命令串里抠出可执行文件并验证存在：引号路径直接查；
    /// 未加引号时逐词累加——裸命令（winget 等）查 PATH，带空格路径逐段拼。
    fn exe_from_cmd(s: &str) -> Option<PathBuf> {
        let s = expand_env(s.trim());
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let first = if s.starts_with('"') {
            s.split('"').nth(1).unwrap_or("").to_string()
        } else {
            let cut = s.find(" -").or_else(|| s.find(" /")).unwrap_or(s.len());
            s[..cut].trim().to_string()
        };
        let cand = first.split(',').next().unwrap_or("").trim();
        if cand.is_empty() {
            return None;
        }
        if cand.starts_with(r"\\") {
            // UNC/网络路径：不探测（避免卡顿），视为有效不清理
            return Some(PathBuf::from(cand));
        }
        let p = PathBuf::from(&cand);
        if p.is_file() {
            return Some(p);
        }
        let mut acc = String::new();
        for tok in cand.split_whitespace() {
            if !acc.is_empty() {
                acc.push(' ');
            }
            acc.push_str(tok);
            let pp = PathBuf::from(&acc);
            if pp.is_file() {
                return Some(pp);
            }
            if !acc.contains('\\') && !acc.contains('/') && command_on_path(&acc) {
                return Some(PathBuf::from(acc));
            }
        }
        None
    }

    fn exe_from_muicache_name(name: &str) -> Option<PathBuf> {
        let pos = name.to_lowercase().find(".exe")?;
        Some(PathBuf::from(&name[..pos + 4]))
    }

    fn reg_str(v: &RegValue) -> Option<String> {
        use winreg::enums::RegType::*;
        if !matches!(v.vtype, REG_SZ | REG_EXPAND_SZ) {
            return None;
        }
        let u16s: Vec<u16> = v
            .bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .take_while(|&c| c != 0)
            .collect();
        Some(String::from_utf16_lossy(&u16s))
    }

    fn muicache_invalid() -> Vec<String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(k) = hkcu.open_subkey(MUI) else {
            return vec![];
        };
        k.enum_values()
            .flatten()
            .map(|(name, _)| name)
            .filter(|name| match exe_from_muicache_name(name) {
                Some(p) => {
                    // UNC/网络路径不探测，视为有效
                    !p.to_string_lossy().starts_with(r"\\") && !p.is_file()
                }
                None => false,
            })
            .collect()
    }

    fn uninstall_invalid() -> Vec<String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(root) = hkcu.open_subkey(UNINST) else {
            return vec![];
        };
        let mut bad = vec![];
        for name in root.enum_keys().flatten() {
            let Ok(k) = root.open_subkey(&name) else { continue };
            let mut valid = false;
            for vn in ["UninstallString", "QuietUninstallString", "DisplayIcon"] {
                if let Some(v) = k.get_raw_value(vn).ok().and_then(|rv| reg_str(&rv)) {
                    if exe_from_cmd(&v).is_some() {
                        valid = true;
                        break;
                    }
                }
            }
            if !valid {
                bad.push(name);
            }
        }
        bad
    }

    fn apppaths_invalid() -> Vec<String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(root) = hkcu.open_subkey(APPPATHS) else {
            return vec![];
        };
        let mut bad = vec![];
        for name in root.enum_keys().flatten() {
            let Ok(k) = root.open_subkey(&name) else { continue };
            let target = k
                .get_raw_value("")
                .ok()
                .and_then(|rv| reg_str(&rv))
                .or_else(|| k.get_raw_value("Path").ok().and_then(|rv| reg_str(&rv)));
            let ok = target.and_then(|t| exe_from_cmd(&t)).is_some();
            if !ok {
                bad.push(name);
            }
        }
        bad
    }

    // ===== 注册表：备份与删除 =====

    fn hexs(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(",")
    }

    fn value_to_reg(name: &str, v: &RegValue) -> String {
        use winreg::enums::RegType::*;
        let n = if name.is_empty() {
            "@".to_string()
        } else {
            format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
        };
        match v.vtype {
            REG_SZ => format!("{n}=hex(1):{}", hexs(&v.bytes)),
            REG_EXPAND_SZ => format!("{n}=hex(2):{}", hexs(&v.bytes)),
            REG_BINARY => format!("{n}=hex:{}", hexs(&v.bytes)),
            REG_DWORD => {
                let d = u32::from_le_bytes(v.bytes.get(..4).and_then(|s| s.try_into().ok()).unwrap_or([0; 4]));
                format!("{n}=dword:{d:08x}")
            }
            REG_QWORD => {
                let d = u64::from_le_bytes(v.bytes.get(..8).and_then(|s| s.try_into().ok()).unwrap_or([0; 8]));
                format!("{n}=qword:{d:x}")
            }
            REG_MULTI_SZ => format!("{n}=hex(7):{}", hexs(&v.bytes)),
            _ => format!("{n}=hex:{}", hexs(&v.bytes)),
        }
    }

    /// 写 .reg（UTF-16LE + BOM）；sections 的第二项必须是相对 HKCU 的完整子键路径
    fn write_reg_backup(
        path: &Path,
        hkey_prefix: &str,
        sections: &[(String, Vec<(String, RegValue)>)],
    ) -> std::io::Result<()> {
        let mut text = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
        for (sub, values) in sections {
            text.push_str(&format!("[{}\\{}]\r\n", hkey_prefix, sub));
            for (n, v) in values {
                text.push_str(&value_to_reg(n, v));
                text.push_str("\r\n");
            }
            text.push_str("\r\n");
        }
        let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
        bytes.extend(text.encode_utf16().flat_map(|c| c.to_le_bytes()));
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(path, bytes)
    }

    fn delete_key_recursive(parent: &RegKey, name: &str) -> std::io::Result<()> {
        if let Ok(sub) = parent.open_subkey_with_flags(name, KEY_READ | KEY_WRITE) {
            for kid in sub.enum_keys().flatten().collect::<Vec<_>>() {
                let _ = delete_key_recursive(&sub, &kid);
            }
        }
        parent.delete_subkey(name)
    }

    fn backup_file(backup_dir: &Path, tag: &str) -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        backup_dir.join(format!("backup-{ts}-{tag}.reg"))
    }

    fn clean_values_in_key(
        rep: &mut CleanReport,
        backup_dir: &Path,
        tag: &str,
        rel: &str,
        names: Vec<String>,
    ) {
        if names.is_empty() {
            return;
        }
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(k) = hkcu.open_subkey_with_flags(rel, KEY_QUERY_VALUE | KEY_SET_VALUE) else {
            rep.errors.push(format!("{tag}: 无法以写权限打开"));
            return;
        };
        let mut sections = vec![];
        let mut deletable = vec![];
        for n in names {
            if let Ok(v) = k.get_raw_value(&n) {
                sections.push((n.clone(), v));
                deletable.push(n);
            }
        }
        if sections.is_empty() {
            return;
        }
        let file = backup_file(backup_dir, tag);
        if write_reg_backup(&file, "HKEY_CURRENT_USER", &[(rel.to_string(), sections)]).is_ok() {
            rep.backups.push(file.display().to_string());
        }
        for n in deletable {
            if k.delete_value(&n).is_ok() {
                rep.removed_reg_items += 1;
            } else {
                rep.errors.push(format!("{tag}: {n} 删除失败"));
            }
        }
    }

    /// 备份整棵子键的值 → 删除子键（父键 rel 下的若干直接子键）
    fn clean_subkeys(
        rep: &mut CleanReport,
        backup_dir: &Path,
        tag: &str,
        rel: &str,
        keys: Vec<String>,
    ) {
        if keys.is_empty() {
            return;
        }
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(root) = hkcu.open_subkey(rel) else {
            return;
        };
        let mut sections = vec![];
        let mut deletable = vec![];
        for kn in &keys {
            if let Ok(k) = root.open_subkey(kn) {
                sections.push((format!("{rel}\\{kn}"), k.enum_values().flatten().collect()));
                deletable.push(kn.clone());
            }
        }
        let file = backup_file(backup_dir, tag);
        if write_reg_backup(&file, "HKEY_CURRENT_USER", &sections).is_ok() {
            rep.backups.push(file.display().to_string());
        }
        if let Ok(root) = hkcu.open_subkey_with_flags(rel, KEY_READ | KEY_WRITE) {
            for kn in deletable {
                if delete_key_recursive(&root, &kn).is_ok() {
                    rep.removed_reg_items += 1;
                } else {
                    rep.errors.push(format!("{tag}\\{kn} 删除失败"));
                }
            }
        }
    }

    fn clean_muicache(backup_dir: &Path, rep: &mut CleanReport) {
        let bad = muicache_invalid();
        clean_values_in_key(rep, backup_dir, "muicache", MUI, bad);
    }

    fn clean_uninstall(backup_dir: &Path, rep: &mut CleanReport) {
        clean_subkeys(rep, backup_dir, "uninstall", UNINST, uninstall_invalid());
    }

    fn clean_apppaths(backup_dir: &Path, rep: &mut CleanReport) {
        clean_subkeys(rep, backup_dir, "apppaths", APPPATHS, apppaths_invalid());
    }

    // ===== 隐私痕迹 =====

    fn privacy_scan() -> u64 {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let mut n = 0u64;
        if let Ok(k) = hkcu.open_subkey(RECENTDOCS) {
            n += k.enum_values().count() as u64 + k.enum_keys().count() as u64;
            for sk in k.enum_keys().flatten() {
                if let Ok(s) = k.open_subkey(&sk) {
                    n += s.enum_values().count() as u64;
                }
            }
        }
        if let Ok(k) = hkcu.open_subkey(TYPEDPATHS) {
            n += k.enum_values().count() as u64;
        }
        if let Ok(k) = hkcu.open_subkey(OPENSAVE) {
            n += k.enum_keys().count() as u64;
        }
        n
    }

    fn clean_privacy(backup_dir: &Path, rep: &mut CleanReport) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        // RecentDocs：值 + 子键
        if let Ok(k) = hkcu.open_subkey(RECENTDOCS) {
            let values: Vec<String> = k.enum_values().flatten().map(|(n, _)| n).collect();
            let subs: Vec<String> = k.enum_keys().flatten().collect();
            let mut sections = vec![];
            if let Ok(k2) = hkcu.open_subkey(RECENTDOCS) {
                sections.push((RECENTDOCS.to_string(), k2.enum_values().flatten().collect()));
            }
            for sk in &subs {
                if let Ok(s) = hkcu.open_subkey(format!("{RECENTDOCS}\\{sk}")) {
                    sections.push((format!("{RECENTDOCS}\\{sk}"), s.enum_values().flatten().collect()));
                }
            }
            if !sections.is_empty() {
                let file = backup_file(backup_dir, "recentdocs");
                if write_reg_backup(&file, "HKEY_CURRENT_USER", &sections).is_ok() {
                    rep.backups.push(file.display().to_string());
                }
                if let Ok(kw) = hkcu.open_subkey_with_flags(RECENTDOCS, KEY_READ | KEY_WRITE) {
                    for v in values {
                        if kw.delete_value(&v).is_ok() {
                            rep.removed_reg_items += 1;
                        }
                    }
                    for sk in subs {
                        if delete_key_recursive(&kw, &sk).is_ok() {
                            rep.removed_reg_items += 1;
                        }
                    }
                }
            }
        }
        // TypedPaths：值
        if let Ok(k) = hkcu.open_subkey(TYPEDPATHS) {
            let values: Vec<String> = k.enum_values().flatten().map(|(n, _)| n).collect();
            clean_values_in_key(rep, backup_dir, "typedpaths", TYPEDPATHS, values);
        }
        // OpenSavePidlMRU：子键
        if let Ok(k) = hkcu.open_subkey(OPENSAVE) {
            let subs: Vec<String> = k.enum_keys().flatten().collect();
            clean_subkeys(rep, backup_dir, "opensave", OPENSAVE, subs);
        }
    }

    // ===== 浏览器 / 系统垃圾 / 开发缓存目录 =====

    /// 各浏览器（Chrome/Edge/Brave/Firefox）的缓存目录，按浏览器分组
    fn browser_profiles() -> Vec<(&'static str, Vec<PathBuf>)> {
        let mut v: Vec<(&'static str, Vec<PathBuf>)> = vec![];
        let Ok(la) = std::env::var("LOCALAPPDATA") else {
            return v;
        };
        let la = PathBuf::from(la);
        for (name, rel) in [
            ("Chrome", r"Google\Chrome"),
            ("Edge", r"Microsoft\Edge"),
            ("Brave", r"BraveSoftware\Brave-Browser"),
        ] {
            let ud = la.join(format!("{rel}\\User Data"));
            let mut dirs = vec![];
            if let Ok(rd) = fs::read_dir(&ud) {
                for prof in rd.flatten() {
                    if !prof.path().is_dir() {
                        continue;
                    }
                    for sub in [
                        "Cache",
                        "Code Cache",
                        "GPUCache",
                        "DawnCache",
                        r"Service Worker\CacheStorage",
                    ] {
                        let d = prof.path().join(sub);
                        if d.is_dir() {
                            dirs.push(d);
                        }
                    }
                }
            }
            if !dirs.is_empty() {
                v.push((name, dirs));
            }
        }
        let ff = la.join(r"Mozilla\Firefox\Profiles");
        if let Ok(rd) = fs::read_dir(&ff) {
            let mut dirs = vec![];
            for prof in rd.flatten() {
                let d = prof.path().join("cache2");
                if d.is_dir() {
                    dirs.push(d);
                }
            }
            if !dirs.is_empty() {
                v.push(("Firefox", dirs));
            }
        }
        v
    }

    fn dev_cache_dirs() -> Vec<PathBuf> {
        let mut v = vec![];
        let Ok(la) = std::env::var("LOCALAPPDATA") else {
            return v;
        };
        for d in [
            r"pip\cache",
            r"npm-cache",
            r"Yarn\Cache",
            r"pnpm\cache",
        ] {
            let p = PathBuf::from(&la).join(d);
            if p.is_dir() {
                v.push(p);
            }
        }
        if let Ok(ap) = std::env::var("APPDATA") {
            let p = PathBuf::from(ap).join("npm-cache");
            if p.is_dir() {
                v.push(p);
            }
        }
        if let Ok(up) = std::env::var("USERPROFILE") {
            for d in [r".cargo\registry\cache", r".cargo\registry\src"] {
                let p = PathBuf::from(&up).join(d);
                if p.is_dir() {
                    v.push(p);
                }
            }
        }
        v
    }

    fn system_junk_dirs() -> Vec<PathBuf> {
        let mut v = vec![];
        if let Ok(la) = std::env::var("LOCALAPPDATA") {
            let d = PathBuf::from(&la).join("CrashDumps");
            if d.is_dir() {
                v.push(d);
            }
        }
        for d in [r"C:\Windows\Minidump", r"C:\Windows\SoftwareDistribution\Download"] {
            let p = PathBuf::from(d);
            if p.is_dir() {
                v.push(p);
            }
        }
        for d in [
            r"C:\ProgramData\Microsoft\Windows\WER\ReportQueue",
            r"C:\ProgramData\Microsoft\Windows\WER\ReportArchive",
        ] {
            let p = PathBuf::from(d);
            if p.is_dir() {
                v.push(p);
            }
        }
        v
    }

    fn recent_lnk_dir() -> Option<PathBuf> {
        std::env::var("APPDATA").ok().map(|d| {
            PathBuf::from(d).join(r"Microsoft\Windows\Recent")
        })
    }

    // ===== 对外：扫描 / 执行 =====

    pub fn scan() -> CleanScan {
        let mut cats = vec![];
        let mut cat = |id: &str, group: &str, title: &str, detail: String, count: u64, bytes: u64| {
            cats.push(CleanCategory {
                id: id.into(),
                group: group.into(),
                title: title.into(),
                detail,
                count,
                bytes,
            });
        };

        let temp = std::env::var("TEMP").map(PathBuf::from).ok();
        let (n, b) = temp
            .as_deref()
            .map(|p| walk(p, DAY, None, 8))
            .unwrap_or((0, 0));
        cat("temp", "file", "用户临时文件", format!("{}（超过 24 小时）", std::env::var("TEMP").unwrap_or_default()), n, b);

        let (n, b) = walk(Path::new(r"C:\Windows\Temp"), DAY, None, 8);
        cat("wintemp", "file", "系统临时文件", "C:\\Windows\\Temp（无管理员权限时尽力而为）".into(), n, b);

        for (name, dirs) in browser_profiles() {
            let (n, b) = scan_multi(&dirs, 0);
            cat(
                &format!("browser:{name}"),
                "file",
                &format!("浏览器缓存：{name}"),
                "浏览器开着时部分文件跳过".into(),
                n,
                b,
            );
        }

        let dirs = system_junk_dirs();
        let (n, b) = scan_multi(&dirs, 0);
        cat("junksys", "file", "系统垃圾（转储/错误报告/更新缓存）", "CrashDumps · Minidump · WER · Windows Update 下载缓存".into(), n, b);

        let dirs = dev_cache_dirs();
        let (n, b) = scan_multi(&dirs, 0);
        cat("devcache", "file", "开发工具缓存", "pip · npm · yarn · pnpm · cargo（下次安装/构建会重新下载）".into(), n, b);

        if let Some(d) = recent_lnk_dir() {
            let (n, b) = walk(&d, 0, Some(".lnk"), 1);
            cat("recentlnk", "file", "隐私：最近文档快捷方式", "%APPDATA%\\Microsoft\\Windows\\Recent".into(), n, b);
        }

        let (n, b) = recycle_stats();
        cat("recyclebin", "file", "回收站", "清空所有分区的回收站".into(), n, b);

        let mu = muicache_invalid().len() as u64;
        cat("muicache", "reg", "注册表：图标/名称缓存残留", "MuiCache 中指向已不存在程序的条目（系统会自动重建）".into(), mu, 0);

        let un = uninstall_invalid().len() as u64;
        cat("uninstall", "reg", "注册表：卸载残留", "卸载程序已丢失的条目（HKLM 系统级需管理员，不处理）".into(), un, 0);

        let ap = apppaths_invalid().len() as u64;
        cat("apppaths", "reg", "注册表：App Paths 死链", "指向已不存在程序的「应用路径」注册项".into(), ap, 0);

        let pv = privacy_scan();
        cat("privacy", "reg", "注册表：隐私痕迹", "最近文档 · 运行框历史 · 打开/保存记录（可还原备份）".into(), pv, 0);

        CleanScan { categories: cats }
    }

    /// 备份目录只保留最近 keep 份，防无限增长
    fn prune_backups(backup_dir: &Path, keep: usize) {
        let Ok(rd) = fs::read_dir(backup_dir) else { return };
        let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        if files.len() <= keep {
            return;
        }
        files.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
        for (_, p) in files.iter().skip(keep) {
            let _ = fs::remove_file(p);
        }
    }

    pub fn execute(ids: &[String], backup_dir: &Path, progress: &dyn Fn(usize, usize, &str)) -> CleanReport {
        if std::sync::atomic::AtomicBool::compare_exchange(
            &CLEAN_RUNNING,
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
        {
            return CleanReport {
                errors: vec!["已有一次清理正在执行，请稍候".into()],
                ..Default::default()
            };
        }
        let _guard = CleanGuard;
        let mut rep = CleanReport::default();
        let total = ids.len();
        for (i, id) in ids.iter().enumerate() {
            progress(i, total, id);
            match id.as_str() {
                "temp" => {
                    if let Ok(t) = std::env::var("TEMP") {
                        let (n, b) = clean_walk(Path::new(&t), DAY, None, 8);
                        rep.removed_files += n;
                        rep.freed_bytes += b;
                    }
                }
                "wintemp" => {
                    let (n, b) = clean_walk(Path::new(r"C:\Windows\Temp"), DAY, None, 8);
                    rep.removed_files += n;
                    rep.freed_bytes += b;
                }
                s if s.starts_with("browser:") => {
                    let name = &s["browser:".len()..];
                    let dirs = browser_profiles()
                        .into_iter()
                        .find(|(n, _)| *n == name)
                        .map(|(_, d)| d)
                        .unwrap_or_default();
                    let (n, b) = clean_multi(&dirs, 0);
                    rep.removed_files += n;
                    rep.freed_bytes += b;
                }
                "junksys" => {
                    let (n, b) = clean_multi(&system_junk_dirs(), 0);
                    rep.removed_files += n;
                    rep.freed_bytes += b;
                }
                "devcache" => {
                    let (n, b) = clean_multi(&dev_cache_dirs(), 0);
                    rep.removed_files += n;
                    rep.freed_bytes += b;
                }
                "recentlnk" => {
                    if let Some(d) = recent_lnk_dir() {
                        let (n, b) = clean_walk(&d, 0, Some(".lnk"), 1);
                        rep.removed_files += n;
                        rep.freed_bytes += b;
                    }
                }
                "recyclebin" => {
                    let (_, b) = recycle_stats();
                    const SHERB_QUIET: u32 = 0x1 | 0x2 | 0x4;
                    if unsafe { SHEmptyRecycleBinW(0, std::ptr::null(), SHERB_QUIET) } == 0 {
                        rep.removed_files += 1;
                        rep.freed_bytes += b;
                    } else {
                        rep.errors.push("回收站清空失败".into());
                    }
                }
                "muicache" => clean_muicache(backup_dir, &mut rep),
                "uninstall" => clean_uninstall(backup_dir, &mut rep),
                "apppaths" => clean_apppaths(backup_dir, &mut rep),
                "privacy" => clean_privacy(backup_dir, &mut rep),
                _ => {}
            }
        }
        prune_backups(backup_dir, 50);
        rep
    }

    // ===== 启动项 =====

    /// StartupApproved 键：首字节奇数 = 该启动项已被任务管理器禁用
    fn startup_disabled(map_rel: &str, name: &str) -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(k) = hkcu.open_subkey(map_rel) else {
            return false;
        };
        match k.get_raw_value(name) {
            Ok(v) => v.bytes.len() >= 4 && v.bytes[0] & 0x01 == 0x01,
            Err(_) => false,
        }
    }

    pub fn startup_scan() -> Vec<StartupItem> {
        const APPROVED_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
        const APPROVED_FOLDER: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";
        let mut out = vec![];
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        for (root, source, idpfx, admin) in [
            (&hkcu, "注册表 HKCU\\Run", "hkcu-run", false),
            (&hklm, "注册表 HKLM\\Run", "hklm-run", true),
        ] {
            if let Ok(k) = root.open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run") {
                for (name, v) in k.enum_values().flatten() {
                    let cmd = reg_str(&v).unwrap_or_else(|| format!("(二进制值 {} 字节)", v.bytes.len()));
                    if name.starts_with("OneDrive-standalone-update") || name == "IcloudServices" {
                        continue;
                    }
                    let disabled = if admin {
                        false
                    } else {
                        startup_disabled(APPROVED_RUN, &name)
                    };
                    out.push(StartupItem {
                        id: format!("{idpfx}:{name}"),
                        source: source.into(),
                        name: name.clone(),
                        cmd,
                        admin,
                        disabled,
                    });
                }
            }
        }
        for (base, source, idpfx, admin) in [
            (
                std::env::var("APPDATA").ok().map(|d| PathBuf::from(d).join(r"Microsoft\Windows\Start Menu\Programs\Startup")),
                "启动文件夹（当前用户）",
                "userfolder",
                false,
            ),
            (
                Some(PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Startup")),
                "启动文件夹（所有用户）",
                "commonfolder",
                true,
            ),
        ] {
            let Some(base) = base else { continue };
            if let Ok(rd) = fs::read_dir(&base) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_file() {
                        let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        if name.eq_ignore_ascii_case("desktop.ini") {
                            continue;
                        }
                        let disabled = if admin {
                            false
                        } else {
                            startup_disabled(APPROVED_FOLDER, &name)
                        };
                        out.push(StartupItem {
                            id: format!("{idpfx}:{name}"),
                            source: source.into(),
                            name: name.trim_end_matches(".lnk").to_string(),
                            cmd: p.display().to_string(),
                            admin,
                            disabled,
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| a.source.cmp(&b.source).then(a.name.cmp(&b.name)));
        out
    }

    /// 启动项禁用/恢复切换（改 StartupApproved 首字节标志，可逆、无删改）
    /// 支持 hkcu-run 与 userfolder；HKLM/全体用户需管理员不支持
    pub fn startup_toggle(id: &str) -> Result<bool, String> {
        let (kind, name) = id.split_once(':').ok_or_else(|| format!("无效 id: {id}"))?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        const APPROVED_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
        const APPROVED_FOLDER: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";
        let (map_rel, run_rel) = match kind {
            "hkcu-run" => (APPROVED_RUN, None),
            "userfolder" => (APPROVED_FOLDER, Some(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupFolder")),
            _ => return Err("该项不支持切换".into()),
        };
        let sa = hkcu
            .open_subkey_with_flags(map_rel, KEY_QUERY_VALUE | KEY_SET_VALUE)
            .map_err(|e| format!("无法打开 StartupApproved: {e}"))?;
        // 无记录视为启用；有记录翻首字节最低位
        let mut v = match sa.get_raw_value(name) {
            Ok(v) => v,
            Err(_) => RegValue {
                bytes: {
                    let mut b = vec![0x02u8];
                    b.resize(12, 0);
                    b[0] = 0x02;
                    b
                },
                vtype: winreg::enums::RegType::REG_BINARY,
            },
        };
        if v.bytes.len() < 12 {
            v.bytes.resize(12, 0);
        }
        let now_disabled = v.bytes[0] & 0x01 == 0x01;
        if now_disabled {
            v.bytes[0] &= 0xFE;
        } else {
            v.bytes[0] |= 0x01;
        }
        sa.set_raw_value(name, &v)
            .map_err(|e| format!("写入失败: {e}"))?;
        Ok(!now_disabled)
    }

    pub fn startup_remove(id: &str, backup_dir: &Path) -> CleanReport {
        let mut rep = CleanReport::default();
        let Some((kind, name)) = id.split_once(':') else {
            rep.errors.push(format!("无法解析启动项 id: {id}"));
            return rep;
        };
        match kind {
            "hkcu-run" => {
                let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                const RUN: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
                const APPROVED_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
                if let Ok(k) = hkcu.open_subkey_with_flags(RUN, KEY_QUERY_VALUE | KEY_SET_VALUE) {
                    if let Ok(v) = k.get_raw_value(name) {
                        // 备份 Run 值 + StartupApproved 记录
                        let mut sections = vec![(RUN.to_string(), vec![(name.to_string(), v)])];
                        if let Ok(sa) = hkcu.open_subkey_with_flags(APPROVED_RUN, KEY_QUERY_VALUE) {
                            if let Ok(sv) = sa.get_raw_value(name) {
                                sections.push((APPROVED_RUN.to_string(), vec![(name.to_string(), sv)]));
                            }
                        }
                        let file = backup_file(backup_dir, "startup");
                        if write_reg_backup(&file, "HKEY_CURRENT_USER", &sections).is_ok() {
                            rep.backups.push(file.display().to_string());
                        }
                        if k.delete_value(name).is_ok() {
                            rep.removed_reg_items += 1;
                        }
                        // 清掉孤儿禁用记录
                        if let Ok(sa) = hkcu.open_subkey_with_flags(APPROVED_RUN, KEY_SET_VALUE) {
                            let _ = sa.delete_value(name);
                        }
                    }
                }
            }
            "hklm-run" | "commonfolder" => {
                rep.errors.push("该项属于全体用户，需要管理员权限，未处理".into());
            }
            "userfolder" => {
                if let Some(base) = std::env::var("APPDATA").ok()
                    .map(|d| PathBuf::from(d).join(r"Microsoft\Windows\Start Menu\Programs\Startup"))
                {
                    let p = base.join(name);
                    if p.is_file() {
                        let bdir = backup_dir.join("startup");
                        let _ = fs::create_dir_all(&bdir);
                        let bfile = bdir.join(name);
                        if fs::copy(&p, &bfile).is_ok() && fs::remove_file(&p).is_ok() {
                            rep.removed_files += 1;
                            rep.backups.push(bfile.display().to_string());
                        } else {
                            rep.errors.push(format!("{name} 删除失败"));
                        }
                        // 连带清 StartupFolder 里的禁用记录（先备份）
                        const APPROVED_FOLDER: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";
                        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                        if let Ok(sa) = hkcu.open_subkey_with_flags(APPROVED_FOLDER, KEY_QUERY_VALUE | KEY_SET_VALUE) {
                            if let Ok(sv) = sa.get_raw_value(name) {
                                let file = backup_file(backup_dir, "startup-folder-sa");
                                if write_reg_backup(
                                    &file,
                                    "HKEY_CURRENT_USER",
                                    &[(APPROVED_FOLDER.to_string(), vec![(name.to_string(), sv)])],
                                )
                                .is_ok()
                                {
                                    rep.backups.push(file.display().to_string());
                                }
                                let _ = sa.delete_value(name);
                            }
                        }
                    }
                }
            }
            _ => rep.errors.push("未知启动项类型".into()),
        }
        rep
    }

    // ===== 大文件（只读） =====

    pub fn large_files_scan() -> Vec<BigFile> {
        #[cfg(windows)]
        use std::os::windows::fs::MetadataExt;
        let mut out: Vec<BigFile> = vec![];
        let up = std::env::var("USERPROFILE").unwrap_or_default();
        let dirs = ["Desktop", "Downloads", "Documents", "Videos", "Pictures"]
            .iter()
            .map(|d| PathBuf::from(&up).join(d))
            .collect::<Vec<_>>();
        const MIN: u64 = 100 * 1024 * 1024;
        const REPARSE: u32 = 0x400; // FILE_ATTRIBUTE_REPARSE_POINT
        fn collect(path: &Path, depth: u32, out: &mut Vec<BigFile>) {
            if depth == 0 || out.len() > 2000 {
                return;
            }
            let Ok(rd) = fs::read_dir(path) else { return };
            for e in rd.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                let p = e.path();
                if ft.is_dir() {
                    // 跳过 junction/symlink（如 XP 兼容链），防循环重复扫描
                    if let Ok(md) = e.metadata() {
                        if md.file_attributes() & REPARSE != 0 {
                            continue;
                        }
                    }
                    collect(&p, depth - 1, out);
                } else if ft.is_file() {
                    if let Ok(md) = e.metadata() {
                        if md.len() >= MIN {
                            let mtime = md
                                .modified()
                                .ok()
                                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            out.push(BigFile {
                                path: p.display().to_string(),
                                size: md.len(),
                                mtime,
                            });
                        }
                    }
                }
            }
        }
        for d in &dirs {
            collect(d, 4, &mut out);
        }
        out.sort_by_key(|f| std::cmp::Reverse(f.size));
        out.truncate(40);
        out
    }

    // ===== DNS =====

    pub fn flush_dns() -> bool {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("ipconfig")
            .args(["/flushdns"])
            .creation_flags(0x0800_0000)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn expand_env_basic() {
            std::env::set_var("CATMON_TEST_VAR", "hello");
            assert_eq!(expand_env("a%CATMON_TEST_VAR%b"), "ahellob");
            assert_eq!(expand_env("%CATMON_NO_SUCH_VAR%x"), "%CATMON_NO_SUCH_VAR%x");
            // 值里含 % 时停止展开，不死循环
            std::env::set_var("CATMON_TEST_PCT", "1%2");
            assert_eq!(expand_env("%CATMON_TEST_PCT%"), "1%2");
        }

        #[test]
        fn exe_from_cmd_cases() {
            // 不存在的文件 → 无效
            assert!(exe_from_cmd(r"C:\catmon_nonexistent_xyz\unins.exe").is_none());
            // 命中真实文件 → 有效（用系统自带文件验证）
            assert!(exe_from_cmd(r"C:\Windows\System32\notepad.exe").is_some());
            // PATH 裸命令（winget/msiexec 同类机制）
            assert!(exe_from_cmd("cmd /c echo hi").is_some());
            // UNC 不探测，视为有效
            assert!(exe_from_cmd(r"\\server\share\app.exe /x").is_some());
        }

        fn rv(t: winreg::enums::RegType, bytes: &[u8]) -> RegValue {
            RegValue {
                bytes: bytes.to_vec(),
                vtype: t,
            }
        }

        #[test]
        fn value_to_reg_formats() {
            use winreg::enums::RegType::*;
            // 默认值名 → @
            assert_eq!(value_to_reg("", &rv(REG_SZ, &[0x41, 0x00])), r#"@=hex(1):41,00"#);
            // 反斜杠与引号转义
            assert_eq!(value_to_reg(r"a\b", &rv(REG_SZ, &[])), r#""a\\b"=hex(1):"#);
            assert_eq!(value_to_reg("a\"b", &rv(REG_SZ, &[])), r#""a\"b"=hex(1):"#);
            // 各类型的 .reg 编码（非默认值名一律带引号）
            assert_eq!(
                value_to_reg("d", &rv(REG_DWORD, &[0x78, 0x56, 0x34, 0x12])),
                r#""d"=dword:12345678"#
            );
            assert_eq!(
                value_to_reg("q", &rv(REG_QWORD, &[1, 0, 0, 0, 0, 0, 0, 0])),
                r#""q"=qword:1"#
            );
            assert_eq!(value_to_reg("b", &rv(REG_BINARY, &[0xde, 0xad])), r#""b"=hex:de,ad"#);
            assert_eq!(value_to_reg("e", &rv(REG_EXPAND_SZ, &[])), r#""e"=hex(2):"#);
            assert_eq!(value_to_reg("m", &rv(REG_MULTI_SZ, &[])), r#""m"=hex(7):"#);
            // 未知类型兜底为二进制备份（执行端只删能备份的）
            assert_eq!(value_to_reg("x", &rv(REG_NONE, &[])), r#""x"=hex:"#);
        }
    }
}

#[cfg(windows)]
pub use imp::{
    execute, flush_dns, large_files_scan, scan, startup_remove, startup_scan, startup_toggle,
};

#[cfg(not(windows))]
pub fn scan() -> CleanScan {
    let mut cats = vec![];
    if let Ok(t) = std::env::var("TMPDIR").or_else(|_| std::env::var("TEMP")) {
        let dir = std::path::PathBuf::from(t);
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let (mut n, mut b) = (0u64, 0u64);
            for e in rd.flatten() {
                if let Ok(md) = e.metadata() {
                    if md.is_file() {
                        n += 1;
                        b += md.len();
                    }
                }
            }
            cats.push(CleanCategory {
                id: "temp".into(),
                group: "file".into(),
                title: "临时文件".into(),
                detail: t,
                count: n,
                bytes: b,
            });
        }
    }
    CleanScan { categories: cats }
}

#[cfg(not(windows))]
pub fn execute(
    ids: &[String],
    _backup_dir: &std::path::Path,
    _progress: &dyn Fn(usize, usize, &str),
) -> CleanReport {
    if std::sync::atomic::AtomicBool::compare_exchange(
        &CLEAN_RUNNING,
        false,
        true,
        std::sync::atomic::Ordering::SeqCst,
        std::sync::atomic::Ordering::SeqCst,
    )
    .is_err()
    {
        return CleanReport {
            errors: vec!["已有一次清理正在执行，请稍候".into()],
            ..Default::default()
        };
    }
    let _guard = CleanGuard;
    let mut rep = CleanReport::default();
    for id in ids {
        if id == "temp" {
            if let Ok(t) = std::env::var("TMPDIR").or_else(|_| std::env::var("TEMP")) {
                if let Ok(rd) = std::fs::read_dir(std::path::Path::new(&t)) {
                    for e in rd.flatten() {
                        let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                        if std::fs::remove_file(e.path()).is_ok() {
                            rep.removed_files += 1;
                            rep.freed_bytes += len;
                        }
                    }
                }
            }
        }
    }
    rep
}

#[cfg(not(windows))]
pub fn startup_scan() -> Vec<StartupItem> {
    vec![]
}

#[cfg(not(windows))]
pub fn startup_remove(_id: &str, _backup_dir: &std::path::Path) -> CleanReport {
    CleanReport::default()
}

#[cfg(not(windows))]
pub fn startup_toggle(_id: &str) -> Result<bool, String> {
    Err("仅 Windows 支持".into())
}

#[cfg(not(windows))]
pub fn large_files_scan() -> Vec<BigFile> {
    vec![]
}

#[cfg(not(windows))]
pub fn flush_dns() -> bool {
    false
}
