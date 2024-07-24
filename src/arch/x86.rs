//! x86 arch-sepcific implementations

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
        JumpLabelType::Nop => [0x3e, 0x8d, 0x74, 0x26, 0x00],
    }
}

/// With given branch as likely branch, initialize the instruction here as a 5-byte NOP instruction
#[doc(hidden)]
#[macro_export]
macro_rules! arch_static_key_init_nop_with_given_branch_likely {
    ($key:path, $branch:expr) => {'my_label: {
        ::core::arch::asm!(
            r#"
            2:
            .byte 0x3e,0x8d,0x74,0x26,0x00
            .pushsection __static_keys, "awR"
            .balign 4
            .long 2b - .
            .long {0} - .
            .long {1} + {2} - .
            .popsection
            "#,
            label {
                break 'my_label !$branch;
            },
            sym $key,
            const $branch as usize,
        );

        // This branch will be adjcent to the NOP/JMP instruction
        break 'my_label $branch;
    }};
}

// The `0x8d,0x76,0x00` is a 3-byte NOP, which is to make sure the `jmp {0}` is at least 5 bytes long.
/// With given branch as likely branch, initialize the instruction here as JMP instruction
#[doc(hidden)]
#[macro_export]
macro_rules! arch_static_key_init_jmp_with_given_branch_likely {
    ($key:path, $branch:expr) => {'my_label: {
        ::core::arch::asm!(
            r#"
            2: 
                jmp {0}
            .byte 0x8d,0x76,0x00
            .pushsection __static_keys, "awR"
            .balign 4
            .long 2b - .
            .long {0} - .
            .long {1} + {2} - .
            .popsection
            "#,
            label {
                break 'my_label !$branch;
            },
            sym $key,
            const $branch as usize,
        );

        // This branch will be adjcent to the NOP/JMP instruction
        break 'my_label $branch;
    }};
}
