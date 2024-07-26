//! Linux-specific implementations

use crate::{code_manipulate::CodeManipulator, JumpEntry};

// See https://sourceware.org/binutils/docs/as/Section.html
/// Name and attribute of section storing jump entries
#[doc(hidden)]
#[macro_export]
macro_rules! os_static_key_sec_name_attr {
    () => {
        "__static_keys, \"awR\""
    };
}

// See https://sourceware.org/binutils/docs/ld/Input-Section-Example.html, modern linkers
// will generate these two symbols indicating the start and end address of __static_keys
// section. Note that the end address is excluded.
extern "Rust" {
    /// Address of this static is the start address of __static_keys section
    #[link_name = "__start___static_keys"]
    pub static mut JUMP_ENTRY_START: JumpEntry;
    /// Address of this static is the end address of __static_keys section (excluded)
    #[link_name = "__stop___static_keys"]
    pub static mut JUMP_ENTRY_STOP: JumpEntry;
}

/// Arch-specific [`CodeManipulator`] using [`libc`] with `mprotect`.
pub struct ArchCodeManipulator;

impl CodeManipulator for ArchCodeManipulator {
    /// Due to limitation of Linux, we cannot get the original memory protection flags easily
    /// without parsing `/proc/[pid]/maps`. As a result, we just make the code region non-writable.
    unsafe fn write_code<const L: usize>(addr: *mut core::ffi::c_void, data: &[u8; L]) {
        // TODO: page_size can be initialized once
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
        let aligned_addr_val = (addr as usize) / page_size * page_size;
        let aligned_addr = aligned_addr_val as *mut core::ffi::c_void;
        let aligned_length = if (addr as usize) + L - aligned_addr_val > page_size {
            page_size * 2
        } else {
            page_size
        };
        let res = unsafe {
            libc::mprotect(
                aligned_addr,
                aligned_length,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
            )
        };
        if res != 0 {
            panic!("Unable to make code region writable");
        }
        core::ptr::copy_nonoverlapping(data.as_ptr(), addr.cast(), L);
        let res = unsafe {
            libc::mprotect(
                aligned_addr,
                aligned_length,
                libc::PROT_READ | libc::PROT_EXEC,
            )
        };
        if res != 0 {
            panic!("Unable to restore code region to non-writable");
        }
    }
}
