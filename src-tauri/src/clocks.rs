//! 每核实时频率。
//! Windows：CallNtPowerInformation 的 CurrentMhz 在现代系统上只回报基准值，
//! 因此用 PDH 计数器 "% Processor Performance" × 标称频率（MaxMhz）换算——与任务管理器同源。
//! Linux 读 /proc/cpuinfo；其余平台无数据。

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    const MORE_DATA: i32 = 0x8000_07D2u32 as i32; // PDH_MORE_DATA
    const FMT_DOUBLE: u32 = 0x200; // PDH_FMT_DOUBLE
    const PROCESSOR_INFORMATION: i32 = 11; // CallNtPowerInformation.ProcessorInformation

    // PDH_FMT_COUNTERVALUE：CStatus 在前（offset 0），union 在 offset 8
    #[repr(C)]
    struct FmtValue {
        cstatus: u32,
        _pad: u32,
        value: f64,
    }
    #[repr(C)]
    struct ItemDouble {
        name: *const u16,
        fmt: FmtValue,
    }

    #[link(name = "pdh")]
    extern "system" {
        fn PdhOpenQueryW(source: *const u16, userdata: usize, query: *mut *mut c_void) -> i32;
        fn PdhAddEnglishCounterW(
            query: *mut c_void,
            path: *const u16,
            userdata: usize,
            counter: *mut *mut c_void,
        ) -> i32;
        fn PdhCollectQueryData(query: *mut c_void) -> i32;
        fn PdhCloseQuery(query: *mut c_void) -> i32;
        // SDK 文档里的 PdhGetFormattedCounterArrayDouble 只是本函数配 PDH_FMT_DOUBLE 的别名，
        // pdh.dll 实际只导出 A/W 两个变体
        fn PdhGetFormattedCounterArrayW(
            counter: *mut c_void,
            format: u32,
            size: *mut u32,
            count: *mut u32,
            buf: *mut u8,
        ) -> i32;
    }

    fn get_array(counter: *mut c_void, size: &mut u32, count: &mut u32, buf: *mut u8) -> i32 {
        unsafe { PdhGetFormattedCounterArrayW(counter, FMT_DOUBLE, size, count, buf) }
    }

    #[repr(C)]
    #[derive(Clone)]
    struct ProcessorPowerInformation {
        number: u32,
        max_mhz: u32,
        current_mhz: u32,
        mhz_limit: u32,
        max_idle_state: u32,
        current_idle_state: u32,
    }

    #[link(name = "powrprof")]
    extern "system" {
        fn CallNtPowerInformation(
            level: i32,
            in_buf: *mut c_void,
            in_len: u32,
            out_buf: *mut c_void,
            out_len: u32,
        ) -> i32;
    }

    fn ppi_buf(count: usize) -> Vec<ProcessorPowerInformation> {
        vec![
            ProcessorPowerInformation {
                number: 0,
                max_mhz: 0,
                current_mhz: 0,
                mhz_limit: 0,
                max_idle_state: 0,
                current_idle_state: 0,
            };
            count
        ]
    }

    /// 标称频率（MHz）：取第一个核心的 MaxMhz
    fn nominal_mhz(count: usize) -> Option<u32> {
        if count == 0 {
            return None;
        }
        let mut buf = ppi_buf(count);
        let st = unsafe {
            CallNtPowerInformation(
                PROCESSOR_INFORMATION,
                std::ptr::null_mut(),
                0,
                buf.as_mut_ptr() as *mut c_void,
                (count * std::mem::size_of::<ProcessorPowerInformation>()) as u32,
            )
        };
        (st == 0)
            .then(|| buf[0].max_mhz)
            .filter(|m| *m > 0)
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn utf16_cstr(p: *const u16) -> Option<String> {
        let mut len = 0usize;
        while *p.add(len) != 0 {
            len += 1;
            if len > 256 {
                return None;
            }
        }
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)))
    }

    /// 持久 PDH 查询；仅在创建它的线程里使用
    pub struct CoreClocks {
        query: *mut c_void,
        counter: *mut c_void,
        nominal: u32,
    }

    impl CoreClocks {
        pub fn new(count: usize) -> Option<Self> {
            let nominal = nominal_mhz(count)?;
            let mut query = std::ptr::null_mut();
            if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) } != 0 {
                return None;
            }
            let path = wide("\\Processor Information(*)\\% Processor Performance");
            let mut counter = std::ptr::null_mut();
            if unsafe { PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) } != 0 {
                unsafe { PdhCloseQuery(query) };
                return None;
            }
            // 预热一次采集
            unsafe { PdhCollectQueryData(query) };
            Some(Self {
                query,
                counter,
                nominal,
            })
        }

        /// 每逻辑核当前频率（MHz）
        pub fn read(&mut self, count: usize) -> Vec<u32> {
            if unsafe { PdhCollectQueryData(self.query) } != 0 {
                return vec![];
            }
            let mut size = 0u32;
            let mut n = 0u32;
            let st = get_array(self.counter, &mut size, &mut n, std::ptr::null_mut());
            if st != MORE_DATA || size == 0 {
                return vec![];
            }
            let mut buf = vec![0u8; size as usize];
            if get_array(self.counter, &mut size, &mut n, buf.as_mut_ptr()) != 0 {
                return vec![];
            }
            let items =
                unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const ItemDouble, n as usize) };
            let mut out = vec![0u32; count];
            for it in items {
                if it.fmt.cstatus != 0 {
                    continue;
                }
                // 实例名形如 "0,3"（节点,核号），"_Total" 跳过
                let Some(name) = (unsafe { utf16_cstr(it.name) }) else {
                    continue;
                };
                let Some((_, core)) = name.rsplit_once(',') else {
                    continue;
                };
                let Ok(idx) = core.trim().parse::<usize>() else {
                    continue;
                };
                if idx < count {
                    out[idx] = ((it.fmt.value / 100.0) * self.nominal as f64).max(0.0) as u32;
                }
            }
            out
        }
    }

    impl Drop for CoreClocks {
        fn drop(&mut self) {
            unsafe { PdhCloseQuery(self.query) };
        }
    }

    /// 回退：CallNtPowerInformation（现代 Windows 常只回报基准值）
    pub fn core_mhz(count: usize) -> Vec<u32> {
        if count == 0 {
            return vec![];
        }
        let mut buf = ppi_buf(count);
        let st = unsafe {
            CallNtPowerInformation(
                PROCESSOR_INFORMATION,
                std::ptr::null_mut(),
                0,
                buf.as_mut_ptr() as *mut c_void,
                (count * std::mem::size_of::<ProcessorPowerInformation>()) as u32,
            )
        };
        if st == 0 {
            buf.iter().map(|p| p.current_mhz).collect()
        } else {
            vec![]
        }
    }
}

#[cfg(windows)]
pub use win::{core_mhz, CoreClocks};

#[cfg(target_os = "linux")]
pub fn core_mhz(count: usize) -> Vec<u32> {
    let Ok(s) = std::fs::read_to_string("/proc/cpuinfo") else {
        return vec![];
    };
    s.lines()
        .filter_map(|l| {
            let v = l.strip_prefix("cpu MHz")?;
            let v = v.trim_start_matches(|c: char| c == ' ' || c == '\t' || c == ':');
            v.parse::<f32>().ok().map(|f| f as u32)
        })
        .take(count)
        .collect()
}

#[cfg(not(windows))]
pub struct CoreClocks;

#[cfg(not(windows))]
impl CoreClocks {
    pub fn new(_count: usize) -> Option<Self> {
        None
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn core_mhz(_count: usize) -> Vec<u32> {
    vec![]
}

#[cfg(all(test, windows))]
mod tests_pdh {
    #[test]
    fn pdh_debug() {
        let n = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(1);
        let Some(mut c) = super::win::CoreClocks::new(n) else {
            println!("RESULT: new=None");
            return;
        };
        let v = c.read(n);
        println!("RESULT: read={v:?}");
    }
}
