//! A fault inside the client is written to `Kryta.<pid>.log` before the client dies.
//!
//! A hook that is wrong takes the whole client down, and the client's own crash report says nothing
//! about a DLL it does not know. So the DLL watches for the faults a bad hook causes - an access
//! violation, an illegal or privileged instruction, a stack overflow - and writes where each one
//! happened: the address, which module it is in, the registers, and the top of the stack. It only
//! watches: the fault is passed on untouched, to the client's own handler.
//!
//! Reforged keeps a crash handler for the same reason (`src/base/CrashHandler.cpp`); this is the
//! small part of that idea a log line needs, not a port of it.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, EXCEPTION_POINTERS,
};
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleFileNameW, GetModuleHandleExW,
};

/// The faults worth a line: access violation, illegal instruction, privileged instruction, stack
/// overflow (`STATUS_ACCESS_VIOLATION` and its neighbours in `ntstatus.h`).
const FAULTS: [u32; 4] = [0xC000_0005, 0xC000_001D, 0xC000_0096, 0xC000_00FD];

/// How many faults are written. A client that handles its own faults may raise many.
const WRITTEN_AT_MOST: u32 = 4;
static WRITTEN: AtomicU32 = AtomicU32::new(0);

/// Where this DLL is loaded, so a fault inside it is named as its own.
static OWN_BASE: AtomicUsize = AtomicUsize::new(0);

/// Start watching. `own_base` is this DLL's module handle.
pub fn install(own_base: usize) {
    OWN_BASE.store(own_base, Ordering::Release);
    // SAFETY: the handler is a function of this DLL, which is never unloaded.
    unsafe {
        AddVectoredExceptionHandler(1, Some(on_exception));
    }
}

/// The module an address is in, as `name+offset`.
fn place(address: usize) -> String {
    let mut module = std::ptr::null_mut();
    let mut name = [0u16; 260];
    // SAFETY: the flags ask for the module holding `address` without taking a reference to it.
    let length = unsafe {
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            address as *const u16,
            &mut module,
        ) == 0
        {
            return format!("{address:#010x} (no module)");
        }
        GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) as usize
    };
    let path = String::from_utf16_lossy(&name[..length]);
    let file = path.rsplit(['\\', '/']).next().unwrap_or(&path);
    format!("{address:#010x} ({file}+{:#x})", address - module as usize)
}

unsafe extern "system" fn on_exception(info: *mut EXCEPTION_POINTERS) -> i32 {
    // EXCEPTION_CONTINUE_SEARCH: this handler never handles anything.
    const CONTINUE_SEARCH: i32 = 0;
    // SAFETY: Windows passes a valid record and context for the faulting thread.
    unsafe {
        let Some(info) = info.as_ref() else {
            return CONTINUE_SEARCH;
        };
        let (Some(record), Some(context)) = (info.ExceptionRecord.as_ref(), info.ContextRecord.as_ref())
        else {
            return CONTINUE_SEARCH;
        };
        let code = record.ExceptionCode as u32;
        if !FAULTS.contains(&code) || WRITTEN.fetch_add(1, Ordering::Relaxed) >= WRITTEN_AT_MOST {
            return CONTINUE_SEARCH;
        }
        let detail = if code == 0xC000_0005 && record.NumberParameters >= 2 {
            let how = match record.ExceptionInformation[0] {
                0 => "reading",
                1 => "writing",
                _ => "executing",
            };
            format!(" {how} {:#010x}", record.ExceptionInformation[1])
        } else {
            String::new()
        };
        super::log::line(format!(
            "FAULT {code:#010x}{detail} at {}",
            place(record.ExceptionAddress as usize)
        ));
        super::log::line(format!(
            "FAULT registers eax={:08x} ebx={:08x} ecx={:08x} edx={:08x} esi={:08x} edi={:08x} ebp={:08x} esp={:08x}",
            context.Eax, context.Ebx, context.Ecx, context.Edx, context.Esi, context.Edi,
            context.Ebp, context.Esp
        ));
        // The top of the stack, with each value that points into a module named: the return
        // addresses among them say how the client got here.
        let stack = context.Esp as usize as *const u32;
        let memory = super::hooks::ProcessMemory;
        let mut callers = Vec::new();
        for slot in 0..48usize {
            let address = stack.add(slot) as usize as u32;
            let Some(value) = crate::patterns::Memory::read_u32(&memory, address) else {
                break;
            };
            let named = place(value as usize);
            if !named.ends_with("(no module)") {
                callers.push(format!("[esp+{:#x}] {named}", slot * 4));
            }
            if callers.len() == 10 {
                break;
            }
        }
        for caller in callers {
            super::log::line(format!("FAULT stack {caller}"));
        }
        let own = OWN_BASE.load(Ordering::Acquire);
        super::log::line(format!("FAULT Kryta.dll is loaded at {own:#010x}"));
    }
    CONTINUE_SEARCH
}
