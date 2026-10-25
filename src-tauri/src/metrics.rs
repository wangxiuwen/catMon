use serde::Serialize;
use sysinfo::{Components, Disks, Networks, ProcessesToUpdate, System};

#[derive(Clone, Serialize)]
pub struct DiskInfo {
    pub mount: String,
    pub total: u64,
    pub avail: u64,
}

#[derive(Clone, Serialize)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub mem: u64,
}

#[derive(Clone, Serialize)]
pub struct TempInfo {
    pub label: String,
    pub celsius: f32,
}

#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub cpu: f32,
    pub cores: Vec<f32>,
    /// 每逻辑核实时频率（MHz）；平台不支持则为空
    pub core_mhz: Vec<u32>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    /// 字节/秒
    pub rx: f64,
    pub tx: f64,
    pub rx_total: u64,
    pub tx_total: u64,
    pub disks: Vec<DiskInfo>,
    pub procs_cpu: Vec<ProcInfo>,
    pub procs_mem: Vec<ProcInfo>,
    pub temps: Vec<TempInfo>,
    /// 开机至今秒数
    pub uptime: u64,
}

pub struct Sampler {
    sys: System,
    nets: Networks,
    disks: Disks,
    comps: Components,
    clocks: Option<crate::clocks::CoreClocks>,
    tick: u64,
    last: std::time::Instant,
}

impl Sampler {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let clocks = crate::clocks::CoreClocks::new(sys.cpus().len());
        Self {
            sys,
            nets: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            comps: Components::new_with_refreshed_list(),
            clocks,
            tick: 0,
            last: std::time::Instant::now(),
        }
    }

    pub fn tick(&mut self) -> Snapshot {
        self.tick += 1;
        let dt = self.last.elapsed().as_secs_f64().max(0.05);
        self.last = std::time::Instant::now();

        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.nets.refresh(true);
        // 进程 CPU 占用需要两次间隔刷新才有意义，每 3 秒刷一次即可
        if self.tick % 3 == 0 {
            self.sys.refresh_processes(ProcessesToUpdate::All, true);
        }
        if self.tick % 5 == 0 {
            self.disks.refresh(true);
            self.comps.refresh(true);
        }

        let mut rx = 0f64;
        let mut tx = 0f64;
        let mut rx_total = 0u64;
        let mut tx_total = 0u64;
        for (_name, data) in &self.nets {
            rx += data.received() as f64 / dt;
            tx += data.transmitted() as f64 / dt;
            rx_total += data.total_received();
            tx_total += data.total_transmitted();
        }

        let disks: Vec<DiskInfo> = self
            .disks
            .list()
            .iter()
            .map(|d| DiskInfo {
                mount: d.mount_point().to_string_lossy().to_string(),
                total: d.total_space(),
                avail: d.available_space(),
            })
            .filter(|d| d.total > 512 * 1024 * 1024)
            .collect();

        let mut procs: Vec<ProcInfo> = self
            .sys
            .processes()
            .values()
            .map(|p| ProcInfo {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu: p.cpu_usage(),
                mem: p.memory(),
            })
            .collect();
        procs.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));
        let procs_cpu: Vec<ProcInfo> = procs.iter().take(10).cloned().collect();
        procs.sort_by(|a, b| b.mem.cmp(&a.mem));
        let procs_mem: Vec<ProcInfo> = procs.iter().take(10).cloned().collect();

        let temps: Vec<TempInfo> = self
            .comps
            .list()
            .iter()
            .filter_map(|c| {
                let t = c.temperature()?;
                (t > 0.0 && t < 150.0).then(|| TempInfo {
                    label: c.label().to_string(),
                    celsius: t,
                })
            })
            .take(8)
            .collect();

        Snapshot {
            cpu: self.sys.global_cpu_usage(),
            cores: self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
            core_mhz: match self.clocks.as_mut() {
                Some(c) => c.read(self.sys.cpus().len()),
                None => crate::clocks::core_mhz(self.sys.cpus().len()),
            },
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            swap_used: self.sys.used_swap(),
            swap_total: self.sys.total_swap(),
            rx,
            tx,
            rx_total,
            tx_total,
            disks,
            procs_cpu,
            procs_mem,
            temps,
            uptime: System::uptime(),
        }
    }
}

/// `catmon --selftest`：采样三次输出 JSON 后退出，用于验证指标链路
pub fn selftest() {
    let mut s = Sampler::new();
    for _ in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let snap = s.tick();
        println!("{}", serde_json::to_string_pretty(&snap).unwrap());
    }
}
