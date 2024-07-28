//! Windows-specific implementations

use windows::Win32::System::{
    Memory::{VirtualProtect, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS},
    SystemInformation::{GetSystemInfo, SYSTEM_INFO},
};

use crate::{code_manipulate::{CodeManipulator, SyncCodeManipulator}, JumpEntry};

// Bugs here, DO NOT USE. See https://github.com/rust-lang/rust/issues/128177
// See https://sourceware.org/binutils/docs/as/Section.html
/// Name and attribute of section storing jump entries
#[doc(hidden)]
#[macro_export]
macro_rules! os_static_key_sec_name_attr {
    () => {
        ".stks$b"
    };
}

// See https://stackoverflow.com/a/14783759 and https://devblogs.microsoft.com/oldnewthing/20181107-00/?p=100155
/// Address of this static is the start address of .stks section
#[link_section = ".stks$a"]
pub static mut JUMP_ENTRY_START: JumpEntry = JumpEntry::dummy();
/// Address of this static is the end address of .stks section
#[link_section = ".stks$c"]
pub static mut JUMP_ENTRY_STOP: JumpEntry = JumpEntry::dummy();

/// Arch-specific [`CodeManipulator`] using `VirtualProtect`.
pub struct ArchCodeManipulator;

impl ArchCodeManipulator {
    unsafe fn write_code_internal<F: Fn(*const u8, *mut u8), const L: usize>(addr: *mut core::ffi::c_void, data: &[u8; L], data_write: F) {
        // TODO: page_size can be initialized once
        let mut system_info = SYSTEM_INFO::default();
        unsafe {
            GetSystemInfo(&mut system_info);
        }
        let page_size = system_info.dwPageSize as usize;
        let aligned_addr_val = (addr as usize) / page_size * page_size;
        let aligned_addr = aligned_addr_val as *mut core::ffi::c_void;
        let aligned_length = if (addr as usize) + L - aligned_addr_val > page_size {
            page_size * 2
        } else {
            page_size
        };
        let mut origin_protect = PAGE_PROTECTION_FLAGS::default();
        let res = unsafe {
            VirtualProtect(
                aligned_addr,
                aligned_length,
                PAGE_EXECUTE_READWRITE,
                &mut origin_protect,
            )
        };
        if res.is_err() {
            panic!("Unable to make code region writable");
        }
        data_write(data.as_ptr(), addr.cast());
        let mut old_protect = PAGE_PROTECTION_FLAGS::default();
        let res = unsafe {
            VirtualProtect(
                aligned_addr,
                aligned_length,
                origin_protect,
                &mut old_protect,
            )
        };
        if res.is_err() {
            panic!("Unable to restore code region to non-writable");
        }
    }
}

impl CodeManipulator for ArchCodeManipulator {
    unsafe fn write_code<const L: usize>(addr: *mut core::ffi::c_void, data: &[u8; L]) {
        Self::write_code_internal(addr, data, |src, dst| {
            core::ptr::copy_nonoverlapping(src, dst, L);
        });
    }
}

static PAGE_MANIPULATR_MUTEX: spin::Mutex<()> = spin::Mutex::new(());

impl SyncCodeManipulator for ArchCodeManipulator {
    unsafe fn write_code_sync<const L: usize>(addr: *mut core::ffi::c_void, data: &[u8; L]) {
        let guard = PAGE_MANIPULATR_MUTEX.lock();
        Self::write_code_internal(addr, data, |src, dst| {
            crate::arch::arch_atomic_copy_nonoverlapping(src, dst);
        });
        drop(guard);
    }
}
