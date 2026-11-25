//! CPU, GPU and memory usage for the TV & power screen.

use crate::util::wide;
use serde::Serialize;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::Performance::*;
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::GetSystemTimes;

#[derive(Serialize, Clone, Debug)]
pub struct Sample {
    pub cpu: f64,
    pub gpu: Option<f64>,
    pub mem: f64,
    pub mem_used_gb: f64,
    pub mem_total_gb: f64,
}

pub struct Stats {
    prev_idle: u64,
    prev_total: u64,
    gpu_query: PDH_HQUERY,
    gpu_counter: PDH_HCOUNTER,
    gpu_ok: bool,
}

// The PDH handles are only used behind the Mutex that owns `Stats`.
unsafe impl Send for Stats {}

fn ft(t: FILETIME) -> u64 {
    ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64
}

impl Stats {
    pub fn new() -> Self {
        let mut s = Stats {
            prev_idle: 0,
            prev_total: 0,
            gpu_query: unsafe { zeroed() },
            gpu_counter: unsafe { zeroed() },
            gpu_ok: false,
        };
        unsafe {
            if PdhOpenQueryW(null(), 0, &mut s.gpu_query) == 0 {
                let path = wide("\\GPU Engine(*engtype_3D)\\Utilization Percentage");
                if PdhAddEnglishCounterW(s.gpu_query, path.as_ptr(), 0, &mut s.gpu_counter) == 0 {
                    PdhCollectQueryData(s.gpu_query);
                    s.gpu_ok = true;
                }
            }
        }
        s.sample();
        s
    }

    pub fn sample(&mut self) -> Sample {
        let (mut idle, mut kernel, mut user): (FILETIME, FILETIME, FILETIME) =
            unsafe { (zeroed(), zeroed(), zeroed()) };
        let mut cpu = 0.0;
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } != 0 {
            let (i, t) = (ft(idle), ft(kernel) + ft(user));
            let (di, dt) = (i.saturating_sub(self.prev_idle), t.saturating_sub(self.prev_total));
            if self.prev_total != 0 && dt > 0 {
                cpu = (1.0 - di as f64 / dt as f64) * 100.0;
            }
            self.prev_idle = i;
            self.prev_total = t;
        }

        let mut m: MEMORYSTATUSEX = unsafe { zeroed() };
        m.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        let (mut mem, mut used, mut total) = (0.0, 0.0, 0.0);
        if unsafe { GlobalMemoryStatusEx(&mut m) } != 0 {
            total = m.ullTotalPhys as f64 / 1_073_741_824.0;
            used = (m.ullTotalPhys - m.ullAvailPhys) as f64 / 1_073_741_824.0;
            mem = m.dwMemoryLoad as f64;
        }

        Sample {
            cpu: cpu.clamp(0.0, 100.0).round(),
            gpu: self.gpu(),
            mem,
            mem_used_gb: (used * 10.0).round() / 10.0,
            mem_total_gb: (total * 10.0).round() / 10.0,
        }
    }

    /// Sums the 3D engine utilisation of every process, like Task Manager.
    fn gpu(&mut self) -> Option<f64> {
        if !self.gpu_ok {
            return None;
        }
        unsafe {
            if PdhCollectQueryData(self.gpu_query) != 0 {
                return None;
            }
            let mut bytes = 0u32;
            let mut count = 0u32;
            PdhGetFormattedCounterArrayW(self.gpu_counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, null_mut());
            if bytes == 0 {
                return Some(0.0);
            }
            let mut buf = vec![0u8; bytes as usize];
            let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            if PdhGetFormattedCounterArrayW(self.gpu_counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, items) != 0 {
                return None;
            }
            let mut total = 0.0;
            for i in 0..count as usize {
                let it = &*items.add(i);
                if it.FmtValue.CStatus == 0 {
                    total += it.FmtValue.Anonymous.doubleValue;
                }
            }
            Some(total.clamp(0.0, 100.0).round())
        }
    }
}

impl Default for Stats {
    fn default() -> Self {
        Self::new()
    }
}
