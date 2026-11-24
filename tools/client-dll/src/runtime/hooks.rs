//! Hooks: the two ways the DLL puts its own code in the client's path.
//!
//! Ported from Py4GW Reforged Native at `e2f5664`:
//!
//! - a **detour** replaces a whole function and keeps a way to call the original. Port of
//!   `src/base/hooker.cpp` (102 lines), a thin layer over MinHook; here the `minhook` crate wraps
//!   the same library.
//! - a **redirect** rewrites one call instruction so it calls the DLL instead. Port of
//!   `MemoryPatcher::SetRedirect` and `PatchActual` (`src/base/memory_patcher.cpp:108-145`): the
//!   instruction keeps its kind and gets a new distance, written under `VirtualProtect` and
//!   followed by `FlushInstructionCache`.
//!
//! Reforged counts its initialisations and keeps each patch's original bytes so it can unhook when
//! it is unloaded. This DLL is loaded by the client's import table and is never unloaded before the
//! process ends, so nothing is restored.

use std::ffi::c_void;

use minhook::MinHook;
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{
    MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY,
    PAGE_GUARD, PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY, MEM_COMMIT,
    VirtualProtect, VirtualQuery,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::patterns::Memory;

/// Prepare a replacement of the function at `target` by `detour`, and return the way to call the
/// original. The replacement takes effect at [`enable`], so the caller can store the original
/// first.
///
/// Port of `HookBase::CreateHookRaw` (`hooker.cpp:83-91`).
///
/// # Safety
///
/// `target` must be the start of a function in this process and `detour` a function with the same
/// calling convention and arguments.
pub unsafe fn create(target: u32, detour: *const c_void) -> Result<*mut c_void, String> {
    unsafe {
        MinHook::create_hook(target as usize as *mut c_void, detour.cast_mut())
            .map_err(|status| format!("MinHook could not create the hook: {status:?}"))
    }
}

/// Put a prepared replacement in force.
///
/// Port of `HookBase::EnableHooks(target)` (`hooker.cpp:51-59`).
///
/// # Safety
///
/// `target` must have been given to [`create`].
pub unsafe fn enable(target: u32) -> Result<(), String> {
    unsafe {
        MinHook::enable_hook(target as usize as *mut c_void)
            .map_err(|status| format!("MinHook could not enable the hook: {status:?}"))
    }
}

/// Make the call instruction at `call` call `destination` instead.
///
/// Port of `MemoryPatcher::SetRedirect` and `PatchActual` (`memory_patcher.cpp:108-145`). Before it
/// writes, it checks that the instruction is a call to `expected`, as `tools/client/title_patch.py`
/// checked the file (`:133-134`): a place that is not what the offsets said is left alone.
///
/// # Safety
///
/// `call` must be an address in this process's code, and `destination` code that can stand in for
/// the function called there.
pub unsafe fn redirect(call: u32, expected: u32, destination: *const c_void) -> Result<(), String> {
    let site = call as usize as *mut u8;
    unsafe {
        let kind = site.read();
        if kind != 0xE8 && kind != 0xE9 {
            return Err(format!("{call:#x} holds {kind:#04x}, which is not a call or a jump"));
        }
        let mut distance = [0u8; 4];
        std::ptr::copy_nonoverlapping(site.add(1), distance.as_mut_ptr(), 4);
        let current = u32::from_le_bytes(distance).wrapping_add(call).wrapping_add(5);
        if current != expected {
            return Err(format!(
                "{call:#x} calls {current:#x}, and the offsets say it calls {expected:#x}"
            ));
        }

        let distance = (destination as usize as u32)
            .wrapping_sub(call)
            .wrapping_sub(5)
            .to_le_bytes();
        let mut old_protect = 0u32;
        if VirtualProtect(site.cast(), 5, PAGE_EXECUTE_READWRITE, &mut old_protect) == 0 {
            return Err(format!("{call:#x} could not be made writable"));
        }
        std::ptr::copy_nonoverlapping(distance.as_ptr(), site.add(1), 4);
        FlushInstructionCache(GetCurrentProcess(), site.cast(), 5);
        VirtualProtect(site.cast(), 5, old_protect, &mut old_protect);
    }
    Ok(())
}

/// This process's memory, read only where Windows says it can be read.
pub struct ProcessMemory;

impl Memory for ProcessMemory {
    /// Port of `CanReadMemory` (`patterns.cpp:75-102`) and the read that follows it.
    fn read_u32(&self, address: u32) -> Option<u32> {
        if address == 0 {
            return None;
        }
        let pointer = address as usize as *const c_void;
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let size = std::mem::size_of::<MEMORY_BASIC_INFORMATION>();
        // SAFETY: VirtualQuery only writes `info`, and the read happens only after it reported the
        // four bytes as committed, readable and inside one region.
        unsafe {
            if VirtualQuery(pointer, &mut info, size) != size {
                return None;
            }
            if info.State != MEM_COMMIT || info.Protect & (PAGE_NOACCESS | PAGE_GUARD) != 0 {
                return None;
            }
            let readable = PAGE_READONLY
                | PAGE_READWRITE
                | PAGE_WRITECOPY
                | PAGE_EXECUTE_READ
                | PAGE_EXECUTE_READWRITE
                | PAGE_EXECUTE_WRITECOPY;
            if info.Protect & readable == 0 {
                return None;
            }
            let start = info.BaseAddress as usize;
            let end = start + info.RegionSize;
            let address = address as usize;
            if address < start || address + 4 > end {
                return None;
            }
            Some(pointer.cast::<u32>().read_unaligned())
        }
    }
}
