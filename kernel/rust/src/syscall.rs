use crate::c_bindings;
use core::ptr;

#[no_mangle]
pub extern "C" fn sys_trace() -> c_bindings::uint64 {
    let proc = unsafe { c_bindings::myproc().as_mut() };
    if let Some(proc) = proc {
        let trace_mask = argaddr(1);
        proc.tracing_mask = trace_mask;
        0
    } else {
        // -1
        u64::MAX
    }
}

fn argaddr(index: i32) -> u64 {
    assert!(index >= 1, "Tried to access a bad syscall index");
    let mut ret_val = 0u64;
    unsafe {
        c_bindings::argaddr(index, ptr::addr_of_mut!(ret_val));
    }
    ret_val
}
