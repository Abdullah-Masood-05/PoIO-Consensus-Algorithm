// ─── core/disk.rs ─────────────────────────────────────────────────────────────
//
// Platform-aware direct / unbuffered I/O helpers.
//
// Goal: force every 4 KiB read to actually traverse the PCIe bus instead of
// being served from the OS page cache.  Without this, the OS would satisfy
// repeated lookups from RAM, destroying the I/O bottleneck that PoIO relies on.
//
// Windows:  FILE_FLAG_NO_BUFFERING  (requires 512-byte aligned I/O)
// Linux:    O_DIRECT                (requires 4096-byte aligned I/O)
// macOS:    F_NOCACHE fcntl()       (no alignment requirement)
//
// For safety we always read into a stack-allocated [u8; 4096] which satisfies
// the 4096-byte boundary on all platforms.
//
// ─────────────────────────────────────────────────────────────────────────────

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

// ── Open helpers ──────────────────────────────────────────────────────────────

/// Open the plot file with OS-level cache bypass where possible.
/// Falls back to a regular read-only open if the platform does not support it.
pub fn open_direct(path: &Path) -> io::Result<File> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_NO_BUFFERING = 0x20000000
        // FILE_FLAG_SEQUENTIAL_SCAN is NOT set — we want random access.
        const FILE_FLAG_NO_BUFFERING: u32 = 0x20000000;
        OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_NO_BUFFERING)
            .open(path)
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECT)
            .open(path)
    }

    #[cfg(target_os = "macos")]
    {
        // Open normally then disable caching via F_NOCACHE.
        let file = OpenOptions::new().read(true).open(path)?;
        // SAFETY: valid file descriptor, F_NOCACHE is documented.
        unsafe {
            libc::fcntl(std::os::unix::io::AsRawFd::as_raw_fd(&file), libc::F_NOCACHE, 1);
        }
        Ok(file)
    }

    // Fallback for other platforms (buffered, but still functional).
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        OpenOptions::new().read(true).open(path)
    }
}

// ── Core read primitive ───────────────────────────────────────────────────────

/// Seek to `byte_offset` and read exactly `buffer.len()` bytes into `buffer`.
///
/// `buffer` must be exactly 4096 bytes on Windows when the file was opened
/// with `FILE_FLAG_NO_BUFFERING`.  All callers in this project use the
/// stack-allocated `[u8; 4096]` from the miner, so this is always satisfied.
#[inline]
pub fn read_chunk_at_offset(
    file:        &mut File,
    byte_offset: u64,
    buffer:      &mut [u8],
) -> io::Result<()> {
    file.seek(SeekFrom::Start(byte_offset))?;
    file.read_exact(buffer)?;
    Ok(())
}
