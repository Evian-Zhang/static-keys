#![doc = include_str!("../docs/en/src/README.md")]
#![no_std]
#![feature(asm_goto)]
#![feature(asm_const)]
#![allow(clippy::needless_doctest_main)]

mod arch;
pub mod code_manipulate;
mod os;

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
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    fn make_relative_address_absolute(&mut self) {
        self.code = (core::ptr::addr_of!(self.code) as usize).wrapping_add(self.code);
        self.target = (core::ptr::addr_of!(self.target) as usize).wrapping_add(self.target);
        self.key = (core::ptr::addr_of!(self.key) as usize).wrapping_add(self.key);
    }

    // For Win64, the relative address is truncated into 32bit.
    // See https://github.com/llvm/llvm-project/blob/862d837e483437b33f5588f89e62085de3a806b9/llvm/lib/Target/X86/MCTargetDesc/X86WinCOFFObjectWriter.cpp#L48-L51
    /// Update fields to be absolute address
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    fn make_relative_address_absolute(&mut self) {
        let code = (self.code as i32) as i64 as usize;
        self.code = (core::ptr::addr_of!(self.code) as usize).wrapping_add(code);
        let target = (self.target as i32) as i64 as usize;
        self.target = (core::ptr::addr_of!(self.target) as usize).wrapping_add(target);
        let key = (self.key as i32) as i64 as usize;
        self.key = (core::ptr::addr_of!(self.key) as usize).wrapping_add(key);
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
    fn key_mut<M: CodeManipulator, const S: bool>(&self) -> &'static mut GenericStaticKey<M, S> {
        unsafe { &mut *(self.key_addr() as *mut GenericStaticKey<M, S>) }
    }

    /// Whether this jump entry is dummy
    fn is_dummy(&self) -> bool {
        self.code == 0
    }

    /// Create a dummy jump entry
    #[allow(unused)]
    const fn dummy() -> Self {
        Self {
            code: 0,
            target: 0,
            key: 0,
        }
    }
}

/// Static key generic over code manipulator.
///
/// The `M: CodeManipulator` is required since when toggling the static key, the instructions recorded
/// at associated jump entries need to be modified, which reside in `.text` section, which is a normally
/// non-writable memory region. As a result, we need to change the protection of such memory region.
///
/// The `const S: bool` indicates the initial status of this key. This value is determined
/// at compile time, and only affect the initial generation of branch layout. All subsequent
/// manually disabling and enabling will not be affected by the initial status. The struct
/// layout is also consistent with different initial status. As a result, it is safe
/// to assign arbitrary status to the static key generic when using.
pub struct GenericStaticKey<M: CodeManipulator, const S: bool> {
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

/// Static key to hold data about current status and which jump entries are associated with this key.
///
/// For now, it is not encouraged to modify static key in a multi-thread application (which I don't think
/// is a common situation).
pub type StaticKey<const S: bool> = GenericStaticKey<crate::os::ArchCodeManipulator, S>;
/// A [`StaticKey`] with initial status `true`.
pub type StaticTrueKey = StaticKey<true>;
/// A [`StaticKey`] with initial status `false`.
pub type StaticFalseKey = StaticKey<false>;

// Insert a dummy static key here, and use this at global_init function. This is
// to avoid linker failure when there is no jump entries, and thus the __static_keys
// section is never defined.
//
// It should work if we just use global_asm to define a dummy jump entry in __static_keys,
// however, it seems a Rust bug to erase sections marked with "R" (retained). If we specify
// --print-gc-sections for linker options, it's strange that linker itself does not
// erase it. IT IS SO STRANGE.
static mut DUMMY_STATIC_KEY: GenericStaticKey<code_manipulate::DummyCodeManipulator, false> =
    GenericStaticKey::new(false);

impl<M: CodeManipulator, const S: bool> GenericStaticKey<M, S> {
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
    ///
    /// # Safety
    ///
    /// This method may be UB if called before [`global_init`] or called in parallel. Never call this method when
    /// there are multi-threads running. Spawn threads after this method is called. This method may manipulate
    /// code region memory protection, and if other threads are executing codes in the same code page, it may
    /// lead to unexpected behaviors.
    pub unsafe fn enable(&mut self) {
        static_key_update(self, true)
    }

    /// Disable this static key (make the value to be `false`). Do nothing if current static key is already disabled.
    ///
    /// # Safety
    ///
    /// This method may be UB if called before [`global_init`] or called in parallel. Never call this method when
    /// there are multi-threads running. Spawn threads after this method is called. This method may manipulate
    /// code region memory protection, and if other threads are executing codes in the same code page, it may
    /// lead to unexpected behaviors.
    pub unsafe fn disable(&mut self) {
        static_key_update(self, false)
    }
}

/// Count of jump entries in __static_keys section. Note that
/// there will be several dummy jump entries inside this section.
pub fn jump_entries_count()-> usize {
    let jump_entry_start_addr = core::ptr::addr_of_mut!(os::JUMP_ENTRY_START);
    let jump_entry_stop_addr = core::ptr::addr_of_mut!(os::JUMP_ENTRY_STOP);
    unsafe { jump_entry_stop_addr.offset_from(jump_entry_start_addr) as usize }
}

// ---------------------------- Create ----------------------------
/// Global state to make sure [`global_init`] is called only once
static GLOBAL_INIT_STATE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
const UNINITIALIZED: usize = 0;
const INITIALIZING: usize = 1;
const INITIALIZED: usize = 2;

/// Initialize the static keys data. Always call this method at beginning of application, before using any static key related
/// functionalities.
///
/// This function should be called only once. If calling this method multiple times in multi-threads, only the first invocation
/// will take effect.
pub fn global_init() {
    // DUMMY_STATIC_KEY will never changed, and this will always be a NOP.
    // Doing this to make sure there are at least one jump entry.
    if static_branch_unlikely!(DUMMY_STATIC_KEY) {
        return;
    }

    // This logic is taken from log::set_logger_inner
    match GLOBAL_INIT_STATE.compare_exchange(
        UNINITIALIZED,
        INITIALIZING,
        core::sync::atomic::Ordering::Acquire,
        core::sync::atomic::Ordering::Relaxed,
    ) {
        Ok(UNINITIALIZED) => {
            global_init_inner();
            GLOBAL_INIT_STATE.store(INITIALIZED, core::sync::atomic::Ordering::Release);
            // Successful init
        }
        Err(INITIALIZING) => {
            while GLOBAL_INIT_STATE.load(core::sync::atomic::Ordering::Relaxed) == INITIALIZING {
                core::hint::spin_loop();
            }
            // Other has inited
        }
        _ => {
            // Other has inited
        }
    }
}

/// Inner function to [`global_init`]
fn global_init_inner() {
    let jump_entry_start_addr = core::ptr::addr_of_mut!(os::JUMP_ENTRY_START);
    let jump_entry_stop_addr = core::ptr::addr_of_mut!(os::JUMP_ENTRY_STOP);
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
    jump_entries.sort_unstable_by_key(|jump_entry| (jump_entry.key_addr(), jump_entry.code_addr()));
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
        // The M and S generic is useless here
        let key = jump_entry.key_mut::<code_manipulate::DummyCodeManipulator, true>();
        // Here we assign associated static key with the start address of jump entries
        key.entries = entries_start_addr;
        last_key_addr = key_addr;
    }
}

/// Create a new static key with `false` as initial value.
///
/// This method should be called to initialize a static mut static key. It is UB to use this method
/// to create a static key on stack or heap, and use this static key to control branches.
///
/// Use [`define_static_key_false`] for short.
pub const fn new_static_false_key() -> StaticFalseKey {
    StaticFalseKey::new(false)
}

/// Create a new static key with `true` as initial value.
///
/// This method should be called to initialize a static mut static key. It is UB to use this method
/// to create a static key on stack or heap, and use this static key to control branches.
///
/// Use [`define_static_key_true`] for short.
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
#[macro_export]
macro_rules! define_static_key_true {
    ($key: ident) => {
        #[used]
        static mut $key: $crate::StaticTrueKey = $crate::new_static_true_key();
    };
}

// ---------------------------- Update ----------------------------
/// The internal method used for [`GenericStaticKey::enable`] and [`GenericStaticKey::disable`].
///
/// This method will update instructions recorded in each jump entries that associated with thie static key
///
/// # Safety
///
/// This method may be UB if called before [`global_init`] or called in parallel. Never call this method when
/// there are multi-threads running. Spawn threads after this method is called. This method may manipulate
/// code region memory protection, and if other threads are executing codes in the same code page, it may
/// lead to unexpected behaviors.
unsafe fn static_key_update<M: CodeManipulator, const S: bool>(
    key: &mut GenericStaticKey<M, S>,
    enabled: bool,
) {
    if key.enabled == enabled {
        return;
    }
    key.enabled = enabled;
    let jump_entry_stop_addr = core::ptr::addr_of!(os::JUMP_ENTRY_STOP);
    let mut jump_entry_addr = key.entries();
    if jump_entry_addr.is_null() {
        // This static key is never used
        return;
    }
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
///
/// # Safety
///
/// This method may be UB if called in parallel. Never call this method when
/// there are multi-threads running. Spawn threads after this method is called. This method may manipulate
/// code region memory protection, and if other threads are executing codes in the same code page, it may
/// lead to unexpected behaviors.
unsafe fn jump_entry_update<M: CodeManipulator>(jump_entry: &JumpEntry, enabled: bool) {
    let jump_label_type = if enabled ^ jump_entry.likely_branch_is_true() {
        JumpLabelType::Jmp
    } else {
        JumpLabelType::Nop
    };
    let code_bytes = arch::arch_jump_entry_instruction(jump_label_type, jump_entry);

    M::write_code(jump_entry.code_addr() as *mut _, &code_bytes);
}

// ---------------------------- Use ----------------------------
/// With given branch as likely branch, initialize the instruction here as JMP instruction
#[doc(hidden)]
#[macro_export]
macro_rules! static_key_init_jmp_with_given_branch_likely {
    ($key:path, $branch:expr) => {'my_label: {
        // This is an ugly workaround for https://github.com/rust-lang/rust/issues/128177
        #[cfg(not(all(target_os = "windows", any(target_arch = "x86", target_arch = "x86_64"))))]
        ::core::arch::asm!(
            $crate::arch_static_key_init_jmp_asm_template!(),
            label {
                break 'my_label !$branch;
            },
            sym $key,
            const $branch as usize,
        );
        #[cfg(all(target_os = "windows", any(target_arch = "x86", target_arch = "x86_64")))]
        ::core::arch::asm!(
            $crate::arch_static_key_init_jmp_asm_template!(),
            label {
                break 'my_label !$branch;
            },
            sym $key,
            const $branch as usize,
            options(att_syntax),
        );

        // This branch will be adjcent to the NOP/JMP instruction
        break 'my_label $branch;
    }};
}

/// With given branch as likely branch, initialize the instruction here as NOP instruction
#[doc(hidden)]
#[macro_export]
macro_rules! static_key_init_nop_with_given_branch_likely {
    ($key:path, $branch:expr) => {'my_label: {
        // This is an ugly workaround for https://github.com/rust-lang/rust/issues/128177
        #[cfg(not(all(target_os = "windows", any(target_arch = "x86", target_arch = "x86_64"))))]
        ::core::arch::asm!(
            $crate::arch_static_key_init_nop_asm_template!(),
            label {
                break 'my_label !$branch;
            },
            sym $key,
            const $branch as usize,
        );
        #[cfg(all(target_os = "windows", any(target_arch = "x86", target_arch = "x86_64")))]
        ::core::arch::asm!(
            $crate::arch_static_key_init_nop_asm_template!(),
            label {
                break 'my_label !$branch;
            },
            sym $key,
            const $branch as usize,
            options(att_syntax),
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
