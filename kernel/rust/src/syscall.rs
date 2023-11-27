use crate::c_bindings;
use core::ptr;

/// The address to write shudown commands for QEMU
pub const QEMU_SHUTDOWN_ADDR: *mut u16 = 0x10_0000 as *mut u16;
/// The data to write to `QEMU_SHUTDOWN_ADDR` to shut down QEMU
const QEMU_SHUTDOWN_DATA: u16 = 0x5555;

/// Provides a means of enabling syscall traces based on a "Trace Mask"
/// Each bit of the mask corresponds to a given syscall
#[no_mangle]
pub extern "C" fn sys_trace() -> c_bindings::uint64 {
    let proc = unsafe { c_bindings::myproc().as_mut() };
    if let Some(proc) = proc {
        let trace_mask = argint(0);
        proc.tracing_mask = trace_mask;
        0
    } else {
        // -1
        u64::MAX
    }
}

/// Gets the current physical memory of the system, along with the number of processes on the system
#[no_mangle]
pub extern "C" fn sys_sysinfo() -> c_bindings::uint64 {
    let proc_count = unsafe { c_bindings::count_proc_not_in_state(c_bindings::procstate::UNUSED) };
    let freemem = unsafe { c_bindings::pfree_count() };
    let mut sysinfo = c_bindings::sysinfo {
        freemem,
        nproc: proc_count,
    };
    let output = argaddr(0);
    let proc = unsafe { c_bindings::myproc().as_mut() };
    if let Some(proc) = proc {
        let copyout_result = unsafe {
            c_bindings::copyout(
                proc.pagetable,
                output,
                ptr::addr_of_mut!(sysinfo).cast::<i8>(),
                core::mem::size_of::<c_bindings::sysinfo>() as u64,
            )
        };
        if copyout_result < 0 {
            u64::MAX
        } else {
            0u64
        }
    } else {
        u64::MAX
    }
}

/// Syscall to shutdown the system from QEMU's perspective
#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn sys_shutdown() -> c_bindings::uint64 {
    unsafe {
        *QEMU_SHUTDOWN_ADDR = QEMU_SHUTDOWN_DATA;
        c_bindings::panic(
            core::ffi::CStr::from_bytes_until_nul(b"\0".as_slice())
                .unwrap()
                .as_ptr()
                .cast_mut(),
        );
    }
}

/// Get the syscall argument at index `index` as a signed, 32-bit int
fn argint(index: i32) -> i32 {
    /* Asserts not panicing correctly
    assert!(
        (0..=5).contains(&index),
        "Tried to access a bad syscall index: {index}"
    );*/
    let mut ret_val = 0i32;
    unsafe {
        c_bindings::argint(index, ptr::addr_of_mut!(ret_val));
    }
    ret_val
}

/// Get the syscall argument at index `index` as an unsigned, 64-bit int (an address)
fn argaddr(index: i32) -> u64 {
    let mut ret_val = 0u64;
    unsafe {
        c_bindings::argaddr(index, ptr::addr_of_mut!(ret_val));
    }
    ret_val
}
