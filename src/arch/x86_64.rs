//! x86_64 arch-sepcific implementations

use crate::{JumpEntry, JumpLabelType};

/// Length of jump instruction to be replaced
pub const ARCH_JUMP_INS_LENGTH: usize = 5;

/// New instruction generated according to jump label type and jump entry
#[inline(always)]
pub fn arch_jump_entry_instruction(
    jump_label_type: JumpLabelType,
    jump_entry: &JumpEntry,
) -> [u8; ARCH_JUMP_INS_LENGTH] {
    match jump_label_type {
        JumpLabelType::Jmp => {
            let relative_addr =
                (jump_entry.target_addr() - (jump_entry.code_addr() + ARCH_JUMP_INS_LENGTH)) as u32;
            let [a, b, c, d] = relative_addr.to_ne_bytes();
            [0xe9, a, b, c, d]
        }
        JumpLabelType::Nop => [0x0f, 0x1f, 0x44, 0x00, 0x00],
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! arch_static_key_init_nop_asm_template {
    () => {
        ::core::concat!(
            r#"
            2:
            .byte 0x0f,0x1f,0x44,0x00,0x00
            .pushsection "#,
            $crate::os_static_key_sec_name!(),
            r#", "awR"
            .balign 8
            .quad 2b - .
            .quad {0} - .
            .quad {1} + {2} - .
            .popsection
            "#
        )
    };
}

// The `0x90,0x90,0x90` are three NOPs, which is to make sure the `jmp {0}` is at least 5 bytes long.
#[doc(hidden)]
#[macro_export]
macro_rules! arch_static_key_init_jmp_asm_template {
    () => {
        ::core::concat!(
            r#"
            2:
                jmp {0}
            .byte 0x90,0x90,0x90
            .pushsection "#,
            $crate::os_static_key_sec_name!(),
            r#", "awR"
            .balign 8
            .quad 2b - .
            .quad {0} - .
            .quad {1} + {2} - .
            .popsection
            "#
        )
    };
}
