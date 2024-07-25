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

/// Dummy code manipulator. Do nothing. Used to declare a dummy static key which is never modified
pub(crate) struct DummyCodeManipulator;

impl CodeManipulator for DummyCodeManipulator {
    unsafe fn mark_code_region_writable(_addr: *const core::ffi::c_void, _length: usize) -> Self {
        Self
    }

    unsafe fn restore_code_region_protect(&self) {}
}
