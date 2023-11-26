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
