#![doc = include_str!("../README.md")]
#![no_std]
#![feature(asm_goto)]
#![feature(asm_const)]

#[cfg(feature = "std")]
extern crate std;

pub mod code_manipulate;

use code_manipulate::CodeManipulator;

/// Entries in the __static_keys section, used for record addresses to modify JMP/NOP.
///
/// The fields of this struct are all **relative address** instead of absolute address considering ASLR.
/// Specifically, it is the relative address between target address and the address of field that record it.
///
/// The relative addresses will be updated to absolute address after calling [`global_init`]. This
/// is because we want to sort the jump entries in place.
struct JumpEntry {
    /// Address of the JMP/NOP instruction to be modified.
    code: usize,
    /// Address of the JMP destination
    target: usize,
    /// Address of associated static key.
    ///
    /// Since the static key has at least 8-byte alignment, the LSB bit of this address is used
    /// to record whether the likely branch is true branch or false branch in order to get right instruction
    /// to replace old one.
    key: usize,
}

impl JumpEntry {
    /// Update fields to be absolute address
    fn make_relative_address_absolute(&mut self) {
        self.code = (core::ptr::addr_of!(self.code) as usize).wrapping_add(self.code);
        self.target = (core::ptr::addr_of!(self.target) as usize).wrapping_add(self.target);
        self.key = (core::ptr::addr_of!(self.key) as usize).wrapping_add(self.key);
    }

    /// Absolute address of the JMP/NOP instruction to be modified
    fn code_addr(&self) -> usize {
        self.code
    }

    /// Absolute address of the JMP destination
    fn target_addr(&self) -> usize {
        self.target
    }

    /// Absolute address of the associated static key
    fn key_addr(&self) -> usize {
        self.key & !1usize
    }

    /// Return `true` if the likely branch is true branch.
    fn likely_branch_is_true(&self) -> bool {
        (self.key & 1usize) != 0
    }

    /// Unique reference to associated key
    fn key_mut<M: CodeManipulator, const S: bool>(&self) -> &mut NoStdStaticKey<M, S> {
        unsafe { &mut *(self.key_addr() as usize as *mut NoStdStaticKey<M, S>) }
    }

    /// Whether this jump entry is dummy
    fn is_dummy(&self) -> bool {
        self.code == 0
    }
}

// Insert a dummy jump entry here to avoid linker failure when there is no
// jump entries, and thus the __static_keys section is never defined.
core::arch::global_asm!(
    r#"
    .pushsection __static_keys, "awR"
    .balign 8
    .quad 0
    .quad 0
    .quad 0
    .popsection
    "#
);

// See https://sourceware.org/binutils/docs/ld/Input-Section-Example.html, modern linkers
// will generate these two symbols indicating the start and end address of __static_keys
// section. Note that the end address is excluded.
extern "Rust" {
    /// Address of this static is the start address of __static_keys section
    #[link_name = "__start___static_keys"]
    static mut JUMP_ENTRY_START: JumpEntry;
    /// Address of this static is the end address of __styatic_keys section (excluded)
    #[link_name = "__stop___static_keys"]
    static mut JUMP_ENTRY_STOP: JumpEntry;
}

/// Static key to hold data about current status and which jump entries are associated with this key.
///
/// For now, it is not encouraged to modify static key in a multi-thread application (which I don't think
/// is a common situation).
///
/// The `M: CodeManipulator` is required since when toggling the static key, the instructions recorded
/// at associated jump entries need to be modified, which reside in `.text` section, which is a normally
/// non-writable memory region. As a result, we need to change the protection of such memory region.
///
/// If you are in a std environment, just use [`StaticKey`], which is a convenient alias, utilizing
/// [`nix`] to modify memory protection.
///
/// The `const S: bool` indicates the initial status of this key. This value is determined
/// at compile time, and only affect the initial generation of branch layout. All subsequent
/// manually disabling and enabling will not be affected by the initial status. The struct
/// layout is also consistent with different initial status. As a result, it is safe
/// to assign arbitrary status to the static key generic when using.
pub struct NoStdStaticKey<M: CodeManipulator, const S: bool> {
    /// Whether current key is true or false
    enabled: bool,
    /// Start address of associated jump entries.
    ///
    /// The jump entries are sorted based on associated static key address in [`global_init`][Self::global_init]
    /// function. As a result, all jump entries associated with this static key are adjcent to each other.
    ///
    /// This value is 0 at static. After calling [`global_init`][Self::global_init], the value will be assigned
    /// correctly.
    entries: usize,
    /// Phantom data to hold `M`
    phantom: core::marker::PhantomData<M>,
}

/// A convenient alias for [`NoStdStaticKey`], utilizing [`nix`] for memory protection manipulation.
#[cfg(feature = "std")]
pub type StaticKey<const S: bool> = NoStdStaticKey<crate::code_manipulate::NixCodeManipulator, S>;
/// A [`StaticKey`] with initial status `true`.
#[cfg(feature = "std")]
pub type StaticTrueKey = StaticKey<true>;
/// A [`StaticKey`] with initial status `false`.
#[cfg(feature = "std")]
pub type StaticFalseKey = StaticKey<false>;

impl<M: CodeManipulator, const S: bool> NoStdStaticKey<M, S> {
    /// Whether initial status is `true`
    #[inline(always)]
    pub const fn initial_enabled(&self) -> bool {
        S
    }

    /// Create a new static key with given default value.
    const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            entries: 0,
            phantom: core::marker::PhantomData,
        }
    }

    /// Get pointer to the start of jump entries which associated with current static key
    fn entries(&self) -> *const JumpEntry {
        self.entries as *const _
    }

    /// Enable this static key (make the value to be `true`). Do nothing if current static key is already enabled.
    pub fn enable(&mut self) {
        unsafe { static_key_update(self, true) }
    }

    /// Disable this static key (make the value to be `false`). Do nothing if current static key is already disabled.
    pub fn disable(&mut self) {
        unsafe { static_key_update(self, false) }
    }

    /// Initialize the static keys data. Always call this method at beginning of application, before using any static key related
    /// functionalities. Users in `std` environment should use [`global_init`] as convenience.
    pub fn global_init() {
        let jump_entry_start_addr = unsafe { core::ptr::addr_of_mut!(JUMP_ENTRY_START) };
        let jump_entry_stop_addr = unsafe { core::ptr::addr_of_mut!(JUMP_ENTRY_STOP) };
        let jump_entry_len =
            unsafe { jump_entry_stop_addr.offset_from(jump_entry_start_addr) as usize };
        let jump_entries =
            unsafe { core::slice::from_raw_parts_mut(jump_entry_start_addr, jump_entry_len) };
        // Update jump entries to be absolute address
        for jump_entry in jump_entries.iter_mut() {
            if jump_entry.is_dummy() {
                continue;
            }
            jump_entry.make_relative_address_absolute();
        }
        // The jump entries are sorted by key address and code address
        jump_entries
            .sort_unstable_by_key(|jump_entry| (jump_entry.key_addr(), jump_entry.code_addr()));
        // Update associated static keys
        let mut last_key_addr = 0;
        for jump_entry in jump_entries {
            if jump_entry.is_dummy() {
                continue;
            }
            let key_addr = jump_entry.key_addr();
            if key_addr == last_key_addr {
                continue;
            }
            let entries_start_addr = jump_entry as *mut _ as usize;
            // The S generic is useless here
            let key = jump_entry.key_mut::<M, true>();
            // Here we assign associated static key with the start address of jump entries
            key.entries = entries_start_addr;
            last_key_addr = key_addr;
        }
    }
}

/// Count of jump entries in __static_keys section. Note that
/// there will be an additional dummy jump entry inside this section.
pub fn jump_entries_count() {
    let jump_entry_start_addr = unsafe { core::ptr::addr_of_mut!(JUMP_ENTRY_START) };
    let jump_entry_stop_addr = unsafe { core::ptr::addr_of_mut!(JUMP_ENTRY_STOP) };
    unsafe { jump_entry_stop_addr.offset_from(jump_entry_start_addr) as usize };
}

// ---------------------------- Create ----------------------------
/// Initialize the static keys data. Always call this method at beginning of application, before using any static key related
/// functionalities.
#[cfg(feature = "std")]
pub fn global_init() {
    StaticTrueKey::global_init();
}

/// Create a new static key with `false` as initial value.
///
/// This method should be called to initialize a static mut static key. It is UB to use this method
/// to create a static key on stack or heap, and use this static key to control branches.
///
/// Use [`define_static_key_false`] for short.
#[cfg(feature = "std")]
pub const fn new_static_false_key() -> StaticFalseKey {
    StaticFalseKey::new(false)
}

/// Create a new static key with `true` as initial value.
///
/// This method should be called to initialize a static mut static key. It is UB to use this method
/// to create a static key on stack or heap, and use this static key to control branches.
///
/// Use [`define_static_key_true`] for short.
#[cfg(feature = "std")]
pub const fn new_static_true_key() -> StaticTrueKey {
    StaticTrueKey::new(true)
}

/// Define a static key with `false` as initial value.
///
/// This macro will define a static mut variable without documentations and visibility modifiers.
/// Use [`new_static_false_key`] for customization.
///
/// # Usage
///
/// ```rust
/// use static_keys::define_static_key_false;
///
/// define_static_key_false!(MY_FALSE_STATIC_KEY);
/// ```
#[cfg(feature = "std")]
#[macro_export]
macro_rules! define_static_key_false {
    ($key: ident) => {
        #[used]
        static mut $key: $crate::StaticFalseKey = $crate::new_static_false_key();
    };
}

/// Define a static key with `true` as initial value.
///
/// This macro will define a static mut variable without documentations and visibility modifiers.
/// Use [`new_static_true_key`] for customization.
///
/// # Usage
///
/// ```rust
/// use static_keys::define_static_key_true;
///
/// define_static_key_true!(MY_TRUE_STATIC_KEY);
/// ```
#[cfg(feature = "std")]
#[macro_export]
macro_rules! define_static_key_true {
    ($key: ident) => {
        #[used]
        static mut $key: $crate::StaticTrueKey = $crate::new_static_true_key();
    };
}

// ---------------------------- Update ----------------------------

/// The internal method used for [`NoStdStaticKey::enable`] and [`NoStdStaticKey::disable`].
///
/// This method will update instructions recorded in each jump entries that associated with thie static key
unsafe fn static_key_update<M: CodeManipulator, const S: bool>(
    key: &mut NoStdStaticKey<M, S>,
    enabled: bool,
) {
    if key.enabled == enabled {
        return;
    }
    key.enabled = enabled;
    let jump_entry_stop_addr = core::ptr::addr_of!(JUMP_ENTRY_STOP);
    let mut jump_entry_addr = key.entries();
    loop {
        if jump_entry_addr >= jump_entry_stop_addr {
            break;
        }
        let jump_entry = &*jump_entry_addr;
        // Not the same key
        if key as *mut _ as usize != jump_entry.key_addr() {
            break;
        }

        jump_entry_update::<M>(jump_entry, enabled);

        jump_entry_addr = jump_entry_addr.add(1);
    }
}

/// Type of the instructions to be modified
enum JumpLabelType {
    /// 5 byte NOP
    Nop = 0,
    /// 5 byte JMP
    Jmp = 1,
}

/// Update instruction recorded in a single jump entry. This is where magic happens
unsafe fn jump_entry_update<M: CodeManipulator>(jump_entry: &JumpEntry, enabled: bool) {
    let jump_label_type = if enabled ^ jump_entry.likely_branch_is_true() {
        JumpLabelType::Jmp
    } else {
        JumpLabelType::Nop
    };
    let code_bytes = match jump_label_type {
        JumpLabelType::Jmp => {
            let relative_addr = (jump_entry.target_addr() - (jump_entry.code_addr() + 5)) as u32;
            let [a, b, c, d] = relative_addr.to_ne_bytes();
            [0xe9, a, b, c, d]
        }
        JumpLabelType::Nop => [0x0f, 0x1f, 0x44, 0x00, 0x00],
    };

    let manipulator = M::mark_code_region_writable(jump_entry.code_addr() as *const _, 5);
    core::ptr::copy_nonoverlapping(
        code_bytes.as_ptr(),
        jump_entry.code_addr() as usize as *mut u8,
        5,
    );
    manipulator.restore_code_region_protect();
}

// ---------------------------- Use ----------------------------
#[doc(hidden)]
#[macro_export]
macro_rules! static_key_init_nop_with_given_branch_likely {
    ($key:path, $branch:expr) => {'my_label: {
        ::core::arch::asm!(
            r#"
            2:
            .byte 0x0f,0x1f,0x44,0x00,0x00
            .pushsection __static_keys, "awR"
            .balign 8
            .quad 2b - .
            .quad {0} - .
            .quad {1} + {2} - .
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

// The three `0x90` here is to make sure the `jmp {0}` is at least 5 bytes long.
#[doc(hidden)]
#[macro_export]
macro_rules! static_key_init_jmp_with_given_branch_likely {
    ($key:path, $branch:expr) => {'my_label: {
        ::core::arch::asm!(
            r#"
            2: 
                jmp {0}
            .byte 0x90,0x90,0x90
            .pushsection __static_keys, "awR"
            .balign 8
            .quad 2b - .
            .quad {0} - .
            .quad {1} + {2} - .
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

/// Use this in a `if` condition, just like the common [`likely`][core::intrinsics::likely]
/// and [`unlikely`][core::intrinsics::unlikely] intrinsics
#[macro_export]
macro_rules! static_branch_unlikely {
    ($key:path) => {{
        unsafe {
            if $key.initial_enabled() {
                $crate::static_key_init_jmp_with_given_branch_likely! { $key, false }
            } else {
                $crate::static_key_init_nop_with_given_branch_likely! { $key, false }
            }
        }
    }};
}

/// Use this in a `if` condition, just like the common [`likely`][core::intrinsics::likely]
/// and [`unlikely`][core::intrinsics::unlikely] intrinsics
#[macro_export]
macro_rules! static_branch_likely {
    ($key:path) => {{
        unsafe {
            if $key.initial_enabled() {
                $crate::static_key_init_nop_with_given_branch_likely! { $key, true }
            } else {
                $crate::static_key_init_jmp_with_given_branch_likely! { $key, true }
            }
        }
    }};
}
