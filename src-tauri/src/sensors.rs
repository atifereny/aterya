//! Hardware readings for the ring and usage widgets, and the laptop battery.
//! - CPU load: GetSystemTimes. CPU temperature/power and SSD temperature/read/write: Windows has no user-mode
//!   source for them, so they come from HWiNFO's shared memory (HWiNFO > Settings > "Shared Memory Support").
//! - GPU temperature, load, power and VRAM: NVML (nvml.dll ships with the NVIDIA driver).
//! - RAM: GlobalMemoryStatusEx. Network: byte counters of the physical adapters.
use serde::Serialize;
use std::{ffi::c_void, sync::Mutex, time::{Duration, Instant}};
use windows::{
    core::{w, PCSTR},
    Win32::{
        Foundation::{CloseHandle, FILETIME},
        NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2},
        System::{
            LibraryLoader::{GetProcAddress, LoadLibraryW},
            Memory::{MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ},
            Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
            SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
            Threading::GetSystemTimes,
        },
    },
};

#[derive(Serialize, Clone, Default)]
pub struct Chip {
    /// Percent.
    load: Option<f32>,
    /// Degrees Celsius.
    temp: Option<f32>,
    /// Watts.
    power: Option<f32>,
}

#[derive(Serialize, Clone, Default)]
pub struct Mem {
    used: u64,
    total: u64,
}

#[derive(Serialize, Clone, Default)]
pub struct Disk {
    name: String,
    temp: Option<f32>,
    /// MB/s.
    read: f32,
    write: f32,
}

#[derive(Serialize, Clone, Default)]
pub struct Sensors {
    cpu: Chip,
    gpu: Chip,
    ram: Mem,
    vram: Option<Mem>,
    /// Bytes per second.
    down: f64,
    up: f64,
    disks: Vec<Disk>,
    /// HWiNFO shared memory found (CPU temperature/power and the SSD values need it).
    hwinfo: bool,
}

static LATEST: Mutex<Option<Sensors>> = Mutex::new(None);

fn ticks(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

/// CPU load between two calls, in percent.
struct CpuLoad(Option<(u64, u64)>);
impl CpuLoad {
    fn read(&mut self) -> Option<f32> {
        let (mut i, mut k, mut u) = Default::default();
        unsafe { GetSystemTimes(Some(&mut i), Some(&mut k), Some(&mut u)) }.ok()?;
        let (idle, total) = (ticks(i), ticks(k) + ticks(u)); // kernel time includes idle
        let prev = self.0.replace((idle, total))?;
        let (di, dt) = (idle.saturating_sub(prev.0), total.saturating_sub(prev.1));
        (dt > 0).then(|| (1.0 - di as f32 / dt as f32).clamp(0.0, 1.0) * 100.0)
    }
}

fn ram() -> Mem {
    let mut m = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_ok() {
        Mem { used: m.ullTotalPhys - m.ullAvailPhys, total: m.ullTotalPhys }
    } else {
        Mem::default()
    }
}

/// Bytes received and sent so far by the physical adapters that are up (virtual ones would double count).
fn net_totals() -> Option<(u64, u64)> {
    unsafe {
        let mut t: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        GetIfTable2(&mut t).ok().ok()?;
        let rows = std::slice::from_raw_parts((*t).Table.as_ptr(), (*t).NumEntries as usize);
        let (mut rx, mut tx) = (0u64, 0u64);
        for r in rows {
            let hardware = r.InterfaceAndOperStatusFlags._bitfield & 1 != 0;
            if hardware && r.OperStatus.0 == 1 && matches!(r.Type, 6 | 71) {
                rx += r.InOctets;
                tx += r.OutOctets;
            }
        }
        FreeMibTable(t as *const c_void);
        Some((rx, tx))
    }
}

#[repr(C)]
struct NvmlUtil {
    gpu: u32,
    memory: u32,
}
#[repr(C)]
struct NvmlMem {
    total: u64,
    free: u64,
    used: u64,
}

/// NVML, loaded by hand so machines without an NVIDIA driver just report nothing.
struct Nvml {
    dev: *mut c_void,
    temp: unsafe extern "C" fn(*mut c_void, u32, *mut u32) -> i32,
    util: unsafe extern "C" fn(*mut c_void, *mut NvmlUtil) -> i32,
    power: unsafe extern "C" fn(*mut c_void, *mut u32) -> i32,
    mem: unsafe extern "C" fn(*mut c_void, *mut NvmlMem) -> i32,
}

impl Nvml {
    fn open() -> Option<Nvml> {
        unsafe {
            let lib = LoadLibraryW(w!("nvml.dll")).ok()?;
            let sym = |name: &'static [u8]| GetProcAddress(lib, PCSTR(name.as_ptr()));
            let init: unsafe extern "C" fn() -> i32 = std::mem::transmute(sym(b"nvmlInit_v2\0")?);
            let by_index: unsafe extern "C" fn(u32, *mut *mut c_void) -> i32 = std::mem::transmute(sym(b"nvmlDeviceGetHandleByIndex_v2\0")?);
            if init() != 0 {
                return None;
            }
            let mut dev = std::ptr::null_mut();
            if by_index(0, &mut dev) != 0 {
                return None;
            }
            Some(Nvml {
                dev,
                temp: std::mem::transmute(sym(b"nvmlDeviceGetTemperature\0")?),
                util: std::mem::transmute(sym(b"nvmlDeviceGetUtilizationRates\0")?),
                power: std::mem::transmute(sym(b"nvmlDeviceGetPowerUsage\0")?),
                mem: std::mem::transmute(sym(b"nvmlDeviceGetMemoryInfo\0")?),
            })
        }
    }

    fn read(&self) -> (Chip, Option<Mem>) {
        let (mut t, mut p, mut u, mut m) = (0u32, 0u32, NvmlUtil { gpu: 0, memory: 0 }, NvmlMem { total: 0, free: 0, used: 0 });
        unsafe {
            let chip = Chip {
                temp: ((self.temp)(self.dev, 0, &mut t) == 0).then_some(t as f32),
                load: ((self.util)(self.dev, &mut u) == 0).then_some(u.gpu as f32),
                power: ((self.power)(self.dev, &mut p) == 0).then_some(p as f32 / 1000.0), // mW
            };
            let vram = ((self.mem)(self.dev, &mut m) == 0).then_some(Mem { used: m.used, total: m.total });
            (chip, vram)
        }
    }
}

/// What HWiNFO's shared memory gave us.
struct Hw {
    cpu_temp: Option<f32>,
    cpu_power: Option<f32>,
    disks: Vec<Disk>,
}

/// "S.M.A.R.T.: KINGSTON SNV3S1000G (50026B76872D393D) [C:, D:]" -> "KINGSTON SNV3S1000G [C:, D:]"
fn drive_name(sensor: &str) -> String {
    let s = sensor.split_once(": ").map_or(sensor, |x| x.1);
    match (s.find(" ("), s.find(')')) {
        (Some(a), Some(b)) if b > a => format!("{}{}", &s[..a], &s[b + 1..]),
        _ => s.to_string(),
    }
}

/// Reads HWiNFO_SENSORS_SHARED_MEM2: a header, sensor elements and reading elements (offsets and sizes come from the header).
fn hwinfo() -> Option<Hw> {
    unsafe {
        let h = OpenFileMappingW(FILE_MAP_READ.0, false, w!(r"Global\HWiNFO_SENS_SM2")).ok()?;
        let view = MapViewOfFile(h, FILE_MAP_READ, 0, 0, 0);
        let base = view.Value as *const u8;
        let out = (|| {
            if base.is_null() {
                return None;
            }
            let u32_at = |o: usize| (base.add(o) as *const u32).read_unaligned();
            let (so, ss, sn) = (u32_at(20) as usize, u32_at(24) as usize, u32_at(28) as usize);
            let (ro, rs, rn) = (u32_at(32) as usize, u32_at(36) as usize, u32_at(40) as usize);
            if ss < 264 || rs < 292 || sn > 2_000 || rn > 20_000 {
                return None;
            }
            let text = |p: *const u8, o: usize, len: usize| {
                let b = std::slice::from_raw_parts(p.add(o), len);
                String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(len)]).to_string()
            };
            let sensor = |i: usize| {
                let p = base.add(so + i * ss);
                let user = text(p, 136, 128);
                if user.is_empty() { text(p, 8, 128) } else { user }
            };
            let (mut cpu_temp, mut cpu_power) = (None, None);
            let mut disks: Vec<Disk> = vec![];
            for i in 0..rn {
                let r = base.add(ro + i * rs);
                let kind = (r as *const u32).read_unaligned();
                let sidx = (r.add(4) as *const u32).read_unaligned() as usize;
                if sidx >= sn {
                    continue;
                }
                let label = {
                    let l = text(r, 140, 128);
                    if l.is_empty() { text(r, 12, 128) } else { l }
                };
                let unit = text(r, 268, 16);
                let mut value = (r.add(284) as *const f64).read_unaligned();
                let l = label.to_lowercase();
                if kind == 1 && cpu_temp.is_none() && (l.contains("tctl") || l.contains("tdie") || l.contains("cpu package") || l == "cpu") {
                    cpu_temp = Some(value as f32);
                }
                if kind == 5 && cpu_power.is_none() && l.contains("cpu package power") {
                    cpu_power = Some(value as f32);
                }
                let sname = sensor(sidx);
                let smart = sname.starts_with("S.M.A.R.T.");
                if (smart && label == "Drive Temperature" && kind == 1) || (sname.starts_with("Drive:") && (label == "Read Rate" || label == "Write Rate")) {
                    if sname.contains("Storage Space") {
                        continue;
                    }
                    let name = drive_name(&sname);
                    let d = match disks.iter().position(|d| d.name == name) {
                        Some(p) => &mut disks[p],
                        None => {
                            disks.push(Disk { name, ..Default::default() });
                            disks.last_mut().unwrap()
                        }
                    };
                    if smart {
                        d.temp = Some(value as f32);
                    } else {
                        value *= match unit.as_str() { "KB/s" => 1.0 / 1024.0, "GB/s" => 1024.0, _ => 1.0 };
                        if label == "Read Rate" { d.read = value as f32 } else { d.write = value as f32 }
                    }
                }
            }
            Some(Hw { cpu_temp, cpu_power, disks })
        })();
        if !base.is_null() {
            let _ = UnmapViewOfFile(view);
        }
        let _ = CloseHandle(h);
        out
    }
}

pub fn start() {
    std::thread::spawn(|| {
        let (mut cpu_load, nvml) = (CpuLoad(None), Nvml::open());
        let mut net = net_totals().map(|t| (t, Instant::now()));
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let hw = hwinfo();
            let (gpu, vram) = nvml.as_ref().map(Nvml::read).unwrap_or_default();
            let (mut down, mut up) = (0.0, 0.0);
            if let Some(now) = net_totals() {
                if let Some(((rx, tx), at)) = net {
                    let dt = at.elapsed().as_secs_f64().max(0.001);
                    down = now.0.saturating_sub(rx) as f64 / dt;
                    up = now.1.saturating_sub(tx) as f64 / dt;
                }
                net = Some((now, Instant::now()));
            }
            let s = Sensors {
                cpu: Chip { load: cpu_load.read(), temp: hw.as_ref().and_then(|h| h.cpu_temp), power: hw.as_ref().and_then(|h| h.cpu_power) },
                gpu,
                ram: ram(),
                vram,
                down,
                up,
                hwinfo: hw.is_some(),
                disks: hw.map(|h| h.disks).unwrap_or_default(),
            };
            *LATEST.lock().unwrap() = Some(s);
        }
    });
}

#[tauri::command]
pub fn sensors_get() -> Sensors {
    LATEST.lock().unwrap().clone().unwrap_or_default()
}

#[derive(Serialize)]
pub struct Battery {
    percent: u32,
    charging: bool,
    plugged: bool,
}

/// The laptop battery, None on a PC without one.
#[tauri::command]
pub fn battery_get() -> Option<Battery> {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s) }.ok()?;
    if s.BatteryFlag & 128 != 0 || s.BatteryLifePercent > 100 {
        return None;
    }
    Some(Battery { percent: s.BatteryLifePercent as u32, charging: s.BatteryFlag & 8 != 0, plugged: s.ACLineStatus == 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drive_names_lose_the_serial() {
        assert_eq!(drive_name("S.M.A.R.T.: KINGSTON SNV3S1000G (50026B76872D393D) [C:, D:]"), "KINGSTON SNV3S1000G [C:, D:]");
        assert_eq!(drive_name("Drive: CT1000P3SSD8 (2328E6ED0E8D)"), "CT1000P3SSD8");
    }
    #[test]
    #[ignore]
    fn live_sensors() {
        let h = hwinfo();
        println!("hwinfo cpu {:?}", h.as_ref().map(|h| (h.cpu_temp, h.cpu_power)));
        for d in h.map(|h| h.disks).unwrap_or_default() {
            println!("disk {} {:?} r{} w{}", d.name, d.temp, d.read, d.write);
        }
        println!("ram {} / {}", ram().used, ram().total);
        println!("net {:?}", net_totals());
        println!("gpu {:?}", Nvml::open().map(|n| { let (c, m) = n.read(); (c.load, c.temp, c.power, m.map(|m| m.used)) }));
    }
}
