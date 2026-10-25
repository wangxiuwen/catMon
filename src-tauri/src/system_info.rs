//! 一次性采集的系统静态信息 + CPUID 解析（缓存/指令集/家族步进，零依赖）。
//! 显卡/主板/BIOS/内存条为平台 best-effort，拿不到就整组省略。

use serde::Serialize;
use sysinfo::System;

#[derive(Clone, Serialize)]
pub struct InfoItem {
    pub k: String,
    pub v: String,
}

#[derive(Clone, Serialize)]
pub struct InfoGroup {
    pub title: String,
    pub items: Vec<InfoItem>,
}

#[derive(Clone, Serialize)]
pub struct SystemInfo {
    pub groups: Vec<InfoGroup>,
}

/// 平台附加信息：处理器组的追加项 + 追加的组
struct Extra {
    cpu_items: Vec<(&'static str, String)>,
    groups: Vec<InfoGroup>,
}

impl Extra {
    fn empty() -> Self {
        Self {
            cpu_items: vec![],
            groups: vec![],
        }
    }
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1073741824.0)
}

fn group(title: &str) -> InfoGroup {
    InfoGroup {
        title: title.into(),
        items: vec![],
    }
}

fn put(g: &mut InfoGroup, k: &str, v: Option<String>) {
    if let Some(v) = v {
        let v = v.trim();
        if !v.is_empty() {
            g.items.push(InfoItem {
                k: k.into(),
                v: v.into(),
            });
        }
    }
}

pub fn collect() -> SystemInfo {
    let sys = System::new_all();
    let extra = platform_extra();

    let mut os = group("操作系统");
    put(&mut os, "系统", System::long_os_version().or_else(System::name));
    put(&mut os, "版本", System::os_version());
    put(&mut os, "内核", System::kernel_version());
    put(&mut os, "主机名", System::host_name());
    os.items.push(InfoItem {
        k: "架构".into(),
        v: std::env::consts::ARCH.to_string(),
    });

    let mut cpu = group("处理器");
    if let Some(c) = sys.cpus().first() {
        put(&mut cpu, "型号", Some(c.brand().to_string()));
    }
    let cores = match sys.physical_core_count() {
        Some(p) => format!("物理 {p} / 逻辑 {}", sys.cpus().len()),
        None => format!("逻辑 {}", sys.cpus().len()),
    };
    cpu.items.push(InfoItem {
        k: "核心".into(),
        v: cores,
    });
    for (k, v) in &extra.cpu_items {
        cpu.items.push(InfoItem {
            k: (*k).into(),
            v: v.clone(),
        });
    }
    for (k, v) in cpuid_items() {
        cpu.items.push(InfoItem { k: k.into(), v });
    }

    let mut mem = group("内存");
    mem.items.push(InfoItem {
        k: "物理内存".into(),
        v: gb(sys.total_memory()),
    });
    if sys.total_swap() > 0 {
        mem.items.push(InfoItem {
            k: "Swap".into(),
            v: gb(sys.total_swap()),
        });
    }

    let mut groups = vec![os, cpu, mem];
    groups.extend(extra.groups);
    SystemInfo { groups }
}

// ---- CPUID（x86_64，零依赖）----

#[cfg(target_arch = "x86_64")]
fn cpuid(leaf: u32, sub: u32) -> (u32, u32, u32, u32) {
    let r = std::arch::x86_64::__cpuid_count(leaf, sub);
    (r.eax, r.ebx, r.ecx, r.edx)
}

#[cfg(target_arch = "x86_64")]
fn fmt_cache(b: u32) -> String {
    if b >= 1024 * 1024 {
        format!("{:.1} MB", b as f64 / 1048576.0)
    } else {
        format!("{} KB", (b + 511) / 1024)
    }
}

/// 厂商 / 家族型号步进 / 指令集 / 缓存层级，全部从 CPUID 指令读
#[cfg(target_arch = "x86_64")]
fn cpuid_items() -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = vec![];
    // 叶 0：厂商串按 EBX:EDX:ECX 顺序拼接
    let (max_leaf, ebx0, ecx0, edx0) = cpuid(0, 0);
    let vendor: String = String::from_utf8_lossy(
        &ebx0.to_le_bytes()
            .iter()
            .chain(edx0.to_le_bytes().iter())
            .chain(ecx0.to_le_bytes().iter())
            .copied()
            .collect::<Vec<u8>>(),
    )
    .trim_end_matches('\0')
    .to_string();
    if vendor != "GenuineIntel" && vendor != "AuthenticAMD" {
        return out;
    }
    out.push((
        "厂商",
        (if vendor == "GenuineIntel" { "Intel" } else { "AMD" }).to_string(),
    ));

    let (eax, _, _, _) = cpuid(1, 0);
    let base_family = (eax >> 8) & 0xF;
    let family = if base_family == 0xF {
        ((eax >> 20) & 0xFF) + base_family
    } else {
        base_family
    };
    let model = if base_family == 0xF || (family == 6 && base_family != 0xF) {
        (((eax >> 16) & 0xF) << 4) | ((eax >> 4) & 0xF)
    } else {
        (eax >> 4) & 0xF
    };
    let stepping = eax & 0xF;
    out.push(("家族/型号/步进", format!("{family:X} / {model:X} / {stepping}")));

    let (_, _, ecx1, edx1) = cpuid(1, 0);
    // 叶 7 的指令集位在 EBX
    let (_, b7, _, _) = if max_leaf >= 7 { cpuid(7, 0) } else { (0, 0, 0, 0) };
    let mut sets: Vec<&str> = vec![];
    if edx1 & (1 << 25) != 0 {
        sets.push("SSE");
    }
    if ecx1 & (1 << 20) != 0 {
        sets.push("SSE4.2");
    }
    if ecx1 & (1 << 28) != 0 {
        sets.push("AVX");
    }
    if ecx1 & (1 << 12) != 0 {
        sets.push("FMA3");
    }
    if ecx1 & (1 << 25) != 0 {
        sets.push("AES-NI");
    }
    if b7 & (1 << 5) != 0 {
        sets.push("AVX2");
    }
    if b7 & (1 << 16) != 0 {
        sets.push("AVX-512");
    }
    if b7 & (1 << 29) != 0 {
        sets.push("SHA");
    }
    if !sets.is_empty() {
        out.push(("指令集", sets.join(" / ")));
    }

    // 缓存层级：确定性缓存参数叶（子叶 0 起枚举，CacheType==0 结束）
    let mut l1 = 0u32;
    let mut l2 = 0u32;
    let mut l3 = 0u32;
    if max_leaf >= 4 {
        for sub in 0..16u32 {
            let (eax, ebx, ecx, _) = cpuid(4, sub);
            if eax & 0x1F == 0 {
                break;
            }
            let ways = ((ebx >> 22) & 0x3FF) + 1;
            let parts = ((ebx >> 12) & 0x3FF) + 1;
            let line = (ebx & 0xFFF) + 1;
            let sets_n = ecx + 1;
            let size = ways * parts * line * sets_n;
            match (eax >> 5) & 0x7 {
                1 => l1 += size, // 数据 + 指令两个子叶
                2 => {
                    if l2 == 0 {
                        l2 = size;
                    }
                }
                3 => {
                    if l3 == 0 {
                        l3 = size;
                    }
                }
                _ => {}
            }
        }
    }
    if l3 == 0 {
        // AMD 旧路径兜底：扩展叶 0x80000006
        let (max_ext, _, _, _) = cpuid(0x8000_0000, 0);
        if max_ext >= 0x8000_0006 {
            let (_, _, ecx6, edx6) = cpuid(0x8000_0006, 0);
            if l2 == 0 {
                l2 = (ecx6 >> 16) * 1024;
            }
            l3 = (edx6 >> 16) * 512 * 1024;
        }
    }
    if l1 > 0 {
        out.push(("L1 缓存", format!("{}（每核）", fmt_cache(l1))));
    }
    if l2 > 0 {
        out.push(("L2 缓存", format!("{}（每核）", fmt_cache(l2))));
    }
    if l3 > 0 {
        out.push(("L3 缓存", format!("{}（共享）", fmt_cache(l3))));
    }
    out
}

#[cfg(not(target_arch = "x86_64"))]
fn cpuid_items() -> Vec<(&'static str, String)> {
    vec![]
}

// ---- Windows：一条 PowerShell CIM 查询拿全显卡/主板/BIOS/插槽/内存条/驱动 ----

#[cfg(windows)]
struct Stick {
    slot: String,
    bytes: u64,
    smbios_type: u32,
    mhz: u32,
    maker: String,
    part: String,
}

#[cfg(windows)]
fn mem_type_name(t: u32) -> &'static str {
    match t {
        24 => "DDR3",
        26 => "DDR4",
        34 => "DDR5",
        _ => "",
    }
}

#[cfg(windows)]
fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// 远程/模拟器会装一堆虚拟显示适配器，有真显卡时把它们丢掉
#[cfg(windows)]
fn clean_gpus(v: String) -> String {
    let parts: Vec<&str> = v.split(" + ").collect();
    let real: Vec<&str> = parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.to_lowercase().contains("virtual"))
        .collect();
    if real.is_empty() {
        v
    } else {
        real.join(" + ")
    }
}

/// PowerShell 一次性查询（Win11 24H2 已移除 wmic，只能走 CIM）
#[cfg(windows)]
fn ps_query(script: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW，避免闪黑框
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    (!s.trim().is_empty()).then_some(s)
}

#[cfg(windows)]
fn platform_extra() -> Extra {
    let mut extra = Extra::empty();
    let Some(raw) = ps_query(concat!(
        "$ErrorActionPreference='SilentlyContinue';",
        "'G|' + ((Get-CimInstance Win32_VideoController).Name -join ' + ');",
        "'M|' + ((Get-CimInstance Win32_BaseBoard | Select-Object -First 1) | ForEach-Object { ([string]$_.Manufacturer + ' ' + [string]$_.Product).Trim() });",
        "$b = Get-CimInstance Win32_BIOS | Select-Object -First 1;",
        "'B|' + ([string]$b.Manufacturer + ' ' + [string]$b.SMBIOSBIOSVersion).Trim();",
        "'BD|' + $b.ReleaseDate.ToString('yyyy-MM-dd');",
        "$p = Get-CimInstance Win32_Processor | Select-Object -First 1;",
        "'P|' + [string]$p.SocketDesignation + '|' + [string]$p.MaxClockSpeed;",
        "Get-CimInstance Win32_PhysicalMemory | Sort-Object DeviceLocator | ForEach-Object { 'S|' + [string]$_.DeviceLocator + '|' + [string]$_.Capacity + '|' + [string]$_.SMBIOSMemoryType + '|' + [string]$_.ConfiguredClockSpeed + '|' + [string]$_.Manufacturer + '|' + [string]$_.PartNumber };",
        "$v = Get-CimInstance Win32_VideoController | Where-Object { [string]$_.Name -notmatch 'Virtual|Basic' } | Select-Object -First 1;",
        "'V|' + [string]$v.DriverVersion + '|' + [string]$v.CurrentHorizontalResolution + '|' + [string]$v.CurrentVerticalResolution"
    )) else {
        return extra;
    };

    let (mut gpu, mut board, mut bios, mut bios_date) = (None, None, None, None);
    let (mut sock, mut base_mhz) = (None, None);
    let mut sticks: Vec<Stick> = vec![];
    let mut vinfo: Option<(String, String, String)> = None;

    for line in raw.lines() {
        if let Some(v) = line.strip_prefix("G|") {
            gpu = non_empty(v);
        } else if let Some(v) = line.strip_prefix("M|") {
            board = non_empty(v);
        } else if let Some(v) = line.strip_prefix("B|") {
            bios = non_empty(v);
        } else if let Some(v) = line.strip_prefix("BD|") {
            bios_date = non_empty(v);
        } else if let Some(v) = line.strip_prefix("P|") {
            let mut it = v.splitn(2, '|');
            sock = non_empty(it.next().unwrap_or(""));
            base_mhz = non_empty(it.next().unwrap_or(""));
        } else if let Some(v) = line.strip_prefix("S|") {
            let p: Vec<&str> = v.splitn(6, '|').collect();
            if p.len() == 6 {
                sticks.push(Stick {
                    slot: p[0].trim().to_string(),
                    bytes: p[1].trim().parse().unwrap_or(0),
                    smbios_type: p[2].trim().parse().unwrap_or(0),
                    mhz: p[3].trim().parse().unwrap_or(0),
                    maker: p[4].trim().to_string(),
                    part: p[5].trim().to_string(),
                });
            }
        } else if let Some(v) = line.strip_prefix("V|") {
            let mut it = v.splitn(3, '|');
            let d = it.next().unwrap_or("").trim().to_string();
            let w = it.next().unwrap_or("").trim().to_string();
            let h = it.next().unwrap_or("").trim().to_string();
            vinfo = Some((d, w, h));
        }
    }

    if let Some(s) = sock {
        extra.cpu_items.push(("插槽", s));
    }
    if let Some(m) = base_mhz {
        if let Ok(mhz) = m.parse::<u32>() {
            if mhz > 0 {
                extra.cpu_items.push(("基准频率", format!("{mhz} MHz")));
            }
        }
    }

    if !sticks.is_empty() {
        let mut g = group("内存（插槽）");
        for (i, s) in sticks.iter().enumerate() {
            let mut v = Vec::new();
            if !s.slot.is_empty() {
                v.push(s.slot.clone());
            }
            if s.bytes > 0 {
                v.push(gb(s.bytes));
            }
            let t = mem_type_name(s.smbios_type);
            if s.mhz > 0 {
                v.push(if t.is_empty() {
                    format!("{} MHz", s.mhz)
                } else {
                    format!("{t}-{}", s.mhz)
                });
            } else if !t.is_empty() {
                v.push(t.to_string());
            }
            if !s.maker.is_empty() {
                v.push(s.maker.clone());
            }
            if !s.part.is_empty() {
                v.push(s.part.clone());
            }
            g.items.push(InfoItem {
                k: format!("条{}", i + 1),
                v: v.join(" · "),
            });
        }
        extra.groups.push(g);
    }

    if let Some(v) = gpu {
        let mut g = group("显卡");
        g.items.push(InfoItem {
            k: "显卡".into(),
            v: clean_gpus(v),
        });
        if let Some((d, w, h)) = &vinfo {
            put(&mut g, "驱动", Some(d.clone()));
            if !w.is_empty() && !h.is_empty() && w != "0" && h != "0" {
                g.items.push(InfoItem {
                    k: "分辨率".into(),
                    v: format!("{}×{}", w, h),
                });
            }
        }
        extra.groups.push(g);
    }

    if board.is_some() || bios.is_some() {
        let mut g = group("主板 / BIOS");
        put(&mut g, "主板", board);
        put(&mut g, "BIOS", bios);
        put(&mut g, "BIOS 日期", bios_date);
        extra.groups.push(g);
    }
    extra
}

#[cfg(target_os = "linux")]
fn platform_extra() -> Extra {
    let mut extra = Extra::empty();
    // lscpu：插槽 / 基准频率（缓存由 CPUID 已覆盖）
    if let Ok(out) = std::process::Command::new("lscpu").output() {
        let s = String::from_utf8_lossy(&out.stdout);
        for line in s.lines() {
            if let Some((k, v)) = line.split_once(':') {
                let v = v.trim();
                if v.is_empty() {
                    continue;
                }
                match k.trim() {
                    "Socket(s)" => extra.cpu_items.push(("插槽", v.to_string())),
                    "CPU max MHz" => extra.cpu_items.push(("基准频率", format!("{v} MHz"))),
                    _ => {}
                }
            }
        }
    }
    // GPU：lspci
    if let Ok(out) = std::process::Command::new("lspci").output() {
        let s = String::from_utf8_lossy(&out.stdout);
        let gpus: Vec<&str> = s
            .lines()
            .filter(|l| {
                l.contains("VGA controller")
                    || l.contains("3D controller")
                    || l.contains("Display controller")
            })
            .filter_map(|l| l.split_once(": ").map(|(_, d)| d.trim()))
            .collect();
        if !gpus.is_empty() {
            let mut g = group("显卡");
            g.items.push(InfoItem {
                k: "显卡".into(),
                v: gpus.join(" + "),
            });
            extra.groups.push(g);
        }
    }
    // 主板 / BIOS：DMI
    let read = |p: &str| {
        std::fs::read_to_string(p)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let board = read("/sys/class/dmi/id/board_vendor").map(|v| {
        let name = read("/sys/class/dmi/id/board_name").unwrap_or_default();
        format!("{v} {name}").trim().to_string()
    });
    let bios = read("/sys/class/dmi/id/bios_vendor").map(|v| {
        let ver = read("/sys/class/dmi/id/bios_version").unwrap_or_default();
        format!("{v} {ver}").trim().to_string()
    });
    if board.is_some() || bios.is_some() {
        let mut g = group("主板 / BIOS");
        put(&mut g, "主板", board);
        put(&mut g, "BIOS", bios);
        extra.groups.push(g);
    }
    extra
}

#[cfg(not(any(windows, target_os = "linux")))]
fn platform_extra() -> Extra {
    Extra::empty()
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    #[test]
    fn cpuid_items_not_empty() {
        let items = super::cpuid_items();
        assert!(!items.is_empty());
        assert!(items.iter().any(|(k, v)| *k == "指令集" && v.contains("AVX2")));
    }
}
