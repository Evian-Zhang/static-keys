//! Windows-specific implementations

use crate::{code_manipulate::CodeManipulator, JumpEntry};

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

/// Arch-specific [`CodeManipulator`]
pub struct ArchCodeManipulator {
    /// Aligned addr
    addr: *mut core::ffi::c_void,
    /// Aligned length
    length: usize,
}

impl CodeManipulator for ArchCodeManipulator {
    unsafe fn mark_code_region_writable(addr: *const core::ffi::c_void, length: usize) -> Self {
        // TODO: page_size can be initialized once
        let page_size = 1024;
        let aligned_addr_val = (addr as usize) / page_size * page_size;
        let aligned_addr = aligned_addr_val as *mut core::ffi::c_void;
        let aligned_length = if (addr as usize) + length - aligned_addr_val > page_size {
            page_size * 2
        } else {
            page_size
        };
        todo!();
        Self {
            addr: aligned_addr,
            length: aligned_length,
        }
    }

    unsafe fn restore_code_region_protect(&self) {
        todo!()
    }
}
