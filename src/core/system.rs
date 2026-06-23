    // ─── core/system.rs ──────────────────────────────────────────────────────────
    //
    // Platform-aware system memory detection.
    //
    // Used to warn operators when their plot file is small enough to fit entirely
    // in DRAM, which would allow a ramdisk attacker to bypass the PCIe I/O
    // bottleneck that the PoIO protocol depends on.
    //
    // Windows:  GlobalMemoryStatusEx (kernel32)
    // Linux:    /proc/meminfo
    // macOS:    sysctl hw.memsize
    //
    // ─────────────────────────────────────────────────────────────────────────────

    /// Information about the host machine's physical memory.
    #[derive(Debug, Clone, Copy)]
    pub struct SystemMemory {
        /// Total physical RAM in bytes, or `None` if detection failed.
        pub total_ram_bytes: Option<u64>,
    }

    impl SystemMemory {
        /// Detect the total physical RAM installed on this machine.
        pub fn detect() -> Self {
            Self {
                total_ram_bytes: detect_total_ram(),
            }
        }

        /// Returns `true` if the given plot size is smaller than 2× the detected
        /// system RAM — meaning the entire plot could trivially fit in a ramdisk.
        pub fn is_plot_cacheable(&self, plot_size: u64) -> bool {
            match self.total_ram_bytes {
                Some(ram) => plot_size < ram.saturating_mul(2),
                None      => false, // can't warn if we can't detect
            }
        }

        /// Human-readable RAM size string (e.g. "15.8 GiB").
        pub fn ram_display(&self) -> String {
            match self.total_ram_bytes {
                Some(bytes) => {
                    let gib = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                    format!("{:.1} GiB", gib)
                }
                None => "unknown".to_string(),
            }
        }
    }

    // ── Platform-specific RAM detection ──────────────────────────────────────────

    #[cfg(target_os = "windows")]
    fn detect_total_ram() -> Option<u64> {
        // SAFETY: MEMORYSTATUSEX is a simple POD struct.  GlobalMemoryStatusEx is
        // a well-documented Win32 API that only writes to the provided pointer.
        #[repr(C)]
        #[allow(non_snake_case, non_camel_case_types)]
        struct MEMORYSTATUSEX {
            dwLength:                u32,
            dwMemoryLoad:            u32,
            ullTotalPhys:            u64,
            ullAvailPhys:            u64,
            ullTotalPageFile:        u64,
            ullAvailPageFile:        u64,
            ullTotalVirtual:         u64,
            ullAvailVirtual:         u64,
            ullAvailExtendedVirtual: u64,
        }

        #[link(name = "kernel32")]
        extern "system" {
            fn GlobalMemoryStatusEx(lpBuffer: *mut MEMORYSTATUSEX) -> i32;
        }

        unsafe {
            let mut mem: MEMORYSTATUSEX = std::mem::zeroed();
            mem.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut mem) != 0 {
                Some(mem.ullTotalPhys)
            } else {
                None
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn detect_total_ram() -> Option<u64> {
        // Parse /proc/meminfo for "MemTotal: <kB>" line.
        let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
        for line in contents.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let kb: u64 = parts[1].parse().ok()?;
                    return Some(kb * 1024); // convert kB → bytes
                }
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    fn detect_total_ram() -> Option<u64> {
        use std::process::Command;
        let output = Command::new("sysctl")
            .arg("-n")
            .arg("hw.memsize")
            .output()
            .ok()?;
        let s = String::from_utf8(output.stdout).ok()?;
        s.trim().parse::<u64>().ok()
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    fn detect_total_ram() -> Option<u64> {
        None
    }

    // ── Platform-specific free disk space detection ──────────────────────────────

    /// Returns the number of free bytes available on the volume that contains `path`.
    /// Returns `None` if detection fails or the platform is unsupported.
    pub fn detect_free_disk_space(path: &std::path::Path) -> Option<u64> {
        // We need to query a *directory* that exists on disk.
        // The plot file may not exist yet, so try the parent directory first,
        // then fall back to the current working directory.
        let dir = if path.is_dir() {
            path.to_path_buf()
        } else if path.exists() {
            // File exists — use its parent
            path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from("."))
        } else {
            // File doesn't exist yet — try parent, then cwd
            path.parent()
                .filter(|p| p.exists())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        };

        // Canonicalize to get an absolute path
        let abs = std::fs::canonicalize(&dir).ok()?;
        platform_free_space(&abs)
    }

    #[cfg(target_os = "windows")]
    fn platform_free_space(path: &std::path::Path) -> Option<u64> {
        // Use GetDiskFreeSpaceExW to query free bytes for the volume containing `path`.
        use std::os::windows::ffi::OsStrExt;

        #[link(name = "kernel32")]
        extern "system" {
            fn GetDiskFreeSpaceExW(
                lpDirectoryName: *const u16,
                lpFreeBytesAvailableToCaller: *mut u64,
                lpTotalNumberOfBytes: *mut u64,
                lpTotalNumberOfFreeBytes: *mut u64,
            ) -> i32;
        }

        // Convert path to null-terminated wide string
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();

        unsafe {
            let mut free_to_caller: u64 = 0;
            let mut total: u64 = 0;
            let mut total_free: u64 = 0;
            if GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_to_caller, &mut total, &mut total_free) != 0 {
                Some(free_to_caller)
            } else {
                None
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn platform_free_space(path: &std::path::Path) -> Option<u64> {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        #[repr(C)]
        #[allow(non_camel_case_types)]
        struct statvfs {
            f_bsize:    u64,
            f_frsize:   u64,
            f_blocks:   u64,
            f_bfree:    u64,
            f_bavail:   u64,
            // remaining fields omitted — we only need the above
            _pad: [u64; 6],
        }

        extern "C" {
            fn statvfs(path: *const i8, buf: *mut statvfs) -> i32;
        }

        let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
        unsafe {
            let mut buf: statvfs = std::mem::zeroed();
            if statvfs(c_path.as_ptr(), &mut buf) == 0 {
                // f_bavail * f_frsize = bytes available to unprivileged users
                Some(buf.f_bavail.saturating_mul(buf.f_frsize))
            } else {
                None
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    fn platform_free_space(_path: &std::path::Path) -> Option<u64> {
        None
    }
