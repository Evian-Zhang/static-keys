//! Utilities to manipulate memory protection.
//!
//! Since we need to make the code region writable and restore it during jump entry update,
//! we need to provide utility functions here.

/// Manipulate memory protection in code region.
pub trait CodeManipulator {
    /// Write `data` as code instruction to `addr`.
    ///
    /// The `addr` is not aligned, you need to align it you self. The length is not too long, usually
    /// 5 bytes.
    ///
    /// # Safety
    ///
    /// This method will do best effort to make the code region writable, and then write the data into
    /// the code region. If the code region is still not writable, the data writing will become a UB.
    /// Never call this method when there are multi-threads running. Spawn threads after this method
    /// is called. This method may manipulate code region memory protection, and if other threads are
    /// executing codes in the same code page, it may lead to unexpected behaviors.
    unsafe fn write_code<const L: usize>(addr: *mut core::ffi::c_void, data: &[u8; L]);
}

pub trait SyncCodeManipulator: CodeManipulator {
    unsafe fn write_code_sync<const L: usize>(addr: *mut core::ffi::c_void, data: &[u8; L]);
}

/// Dummy code manipulator. Do nothing. Used to declare a dummy static key which is never modified
pub(crate) struct DummyCodeManipulator;

impl CodeManipulator for DummyCodeManipulator {
    unsafe fn write_code<const L: usize>(_addr: *mut core::ffi::c_void, _data: &[u8; L]) {}
}
