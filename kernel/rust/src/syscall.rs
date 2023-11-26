use crate::c_bindings;
use core::ptr;

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

fn argaddr(index: i32) -> u64 {
    let mut ret_val = 0u64;
    unsafe {
        c_bindings::argaddr(index, ptr::addr_of_mut!(ret_val));
    }
    ret_val
}
