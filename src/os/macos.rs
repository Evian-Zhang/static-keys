//! macOS-specific implementations

use crate::JumpEntry;

// See https://developer.apple.com/library/archive/documentation/DeveloperTools/Reference/Assembler/040-Assembler_Directives/asm_directives.html#//apple_ref/doc/uid/TP30000823-CJBIFBJG
/// Name and attribute of section storing jump entries
#[doc(hidden)]
#[macro_export]
macro_rules! os_static_key_sec_name_attr {
    () => {
        "__DATA,__static_keys,regular,no_dead_strip"
    };
}

// See https://stackoverflow.com/q/17669593/10005095 and https://github.com/apple-opensource-mirror/ld64/blob/master/unit-tests/test-cases/section-labels/main.c
extern "Rust" {
    /// Address of this static is the start address of __static_keys section
    #[link_name = "\x01section$start$__DATA$__static_keys"]
    pub static mut JUMP_ENTRY_START: JumpEntry;
    /// Address of this static is the end address of __static_keys section (excluded)
    #[link_name = "\x01section$end$__DATA$__static_keys"]
    pub static mut JUMP_ENTRY_STOP: JumpEntry;
}
