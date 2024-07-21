//! Utilities to manipulate memory protection.
//!
//! Since we need to make the code region writable and restore it during jump entry update,
//! we need to provide utility functions here.
//!
//! For `std` environment, we can directly use [`NixCodeManipulator`] here, which utilizes
//! [`nix`] to manipulate memory protection with `mprotect`. For `no_std` environment, there
//! are either no memory protection mechanism or complicated memory protections, so implement
//! it you self. :)

/// Manipulate memory protection in code region.
pub trait CodeManipulator {
    /// Mark the code region starting at `addr` with `length` writable.
    ///
    /// The `addr` is not aligned, you need to align it you self. The length is not too long, usually
    /// 5 bytes.
    unsafe fn mark_code_region_writable(addr: *const core::ffi::c_void, length: usize) -> Self;
    /// Restore the code region protection after the instruction has been updated.
    unsafe fn restore_code_region_protect(&self);
}

/// A conveninent [`CodeManipulator`] using [`nix`] with `mprotect`.
#[cfg(feature = "std")]
pub struct NixCodeManipulator {
    /// Aligned addr
    addr: core::ptr::NonNull<core::ffi::c_void>,
    /// Aligned length
    length: usize,
}

#[cfg(feature = "std")]
impl CodeManipulator for NixCodeManipulator {
    unsafe fn mark_code_region_writable(addr: *const core::ffi::c_void, length: usize) -> Self {
        use nix::sys::mman::ProtFlags;
        // TODO: The page size should be probed using `sysconf`.
        const PAGE_SIZE: usize = 4096;
        let aligned_addr_val = (addr as usize) / PAGE_SIZE * PAGE_SIZE;
        let aligned_addr =
            core::ptr::NonNull::new_unchecked(aligned_addr_val as *mut core::ffi::c_void);
        let aligned_length = if (addr as usize) + length - aligned_addr_val > PAGE_SIZE {
            PAGE_SIZE * 2
        } else {
            PAGE_SIZE
        };
        nix::sys::mman::mprotect(
            aligned_addr,
            aligned_length,
            ProtFlags::PROT_READ | ProtFlags::PROT_WRITE | ProtFlags::PROT_EXEC,
        )
        .expect("Unable to make code region writable");
        Self {
            addr: aligned_addr,
            length: aligned_length,
        }
    }

    /// Due to limitation of Linux, we cannot get the original memory protection flags easily
    /// without parsing `/proc/[pid]/maps`. As a result, we just make the code region non-writable.
    unsafe fn restore_code_region_protect(&self) {
        use nix::sys::mman::ProtFlags;
        nix::sys::mman::mprotect(
            self.addr,
            self.length,
            ProtFlags::PROT_READ | ProtFlags::PROT_EXEC,
        )
        .expect("Unable to restore code region to non-writable");
    }
}
