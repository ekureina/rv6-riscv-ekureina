use crate::{
    c_bindings,
    device_load::{CPU_COUNT, PHYSICAL_ADDRESS_STOP},
    vm::{copyout, PageTableEntry},
};
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
    let freemem = crate::kalloc::ALLOCATOR.memfree_count();
    let sysinfo = c_bindings::sysinfo {
        max_mem: unsafe { PHYSICAL_ADDRESS_STOP },
        cpu_count: unsafe { CPU_COUNT },
        freemem,
        nproc: proc_count,
    };
    let output = argaddr(0);
    let proc = unsafe { c_bindings::myproc().as_mut() };
    if let Some(proc) = proc {
        let copyout_result = unsafe {
            copyout(
                proc.pagetable,
                output,
                ptr::addr_of!(sysinfo).cast(),
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
pub extern "C" fn sys_shutdown() -> c_bindings::uint64 {
    unsafe {
        *QEMU_SHUTDOWN_ADDR = QEMU_SHUTDOWN_DATA;
        c_bindings::panic(b"\0".as_ptr().cast::<i8>().cast_mut());
    }
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn sys_pgaccess() -> c_bindings::uint64 {
    let start_va = argaddr(0);
    let page_count = argint(1);
    if page_count > 32 {
        return u64::MAX;
    }
    let out_bitmask = argaddr(2);
    if out_bitmask == 0 {
        return u64::MAX;
    }

    match unsafe { c_bindings::myproc().as_ref() } {
        None => u64::MAX,
        Some(my_process) => {
            let mut page_bitmask = 0u32;
            for (page_index, va) in (start_va
                ..(start_va + u64::from(u32::try_from(page_count).unwrap() * c_bindings::PGSIZE)))
                .step_by(c_bindings::PGSIZE as usize)
                .enumerate()
            {
                match unsafe { c_bindings::walk(my_process.pagetable, va, 0).as_mut() } {
                    None => return u64::MAX,
                    Some(pte) => {
                        let mut page_table_entry = PageTableEntry(*pte);
                        if page_table_entry.accessed() {
                            page_bitmask |= 1 << page_index;
                            page_table_entry.clear_accessed();
                            *pte = page_table_entry.into();
                        }
                    }
                }
            }
            unsafe {
                copyout(
                    my_process.pagetable,
                    out_bitmask,
                    ptr::addr_of!(page_bitmask).cast(),
                    core::mem::size_of_val(&page_bitmask) as u64,
                )
            };
            0
        }
    }
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn sys_pgdirty() -> c_bindings::uint64 {
    let start_va = argaddr(0);
    let page_count = argint(1);
    if page_count > 32 {
        return u64::MAX;
    }
    let out_bitmask = argaddr(2);
    if out_bitmask == 0 {
        return u64::MAX;
    }

    match unsafe { c_bindings::myproc().as_ref() } {
        None => u64::MAX,
        Some(my_process) => {
            let mut page_bitmask = 0u32;
            for (page_index, va) in (start_va
                ..(start_va + u64::from(u32::try_from(page_count).unwrap() * c_bindings::PGSIZE)))
                .step_by(c_bindings::PGSIZE as usize)
                .enumerate()
            {
                match unsafe { c_bindings::walk(my_process.pagetable, va, 0).as_mut() } {
                    None => return u64::MAX,
                    Some(pte) => {
                        let mut page_table_entry = PageTableEntry(*pte);
                        if page_table_entry.dirty() {
                            page_bitmask |= 1 << page_index;
                            page_table_entry.clear_dirty();
                            *pte = page_table_entry.into();
                        }
                    }
                }
            }
            unsafe {
                copyout(
                    my_process.pagetable,
                    out_bitmask,
                    ptr::addr_of!(page_bitmask).cast(),
                    core::mem::size_of_val(&page_bitmask) as u64,
                );
            }
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn sys_sigalarm() -> c_bindings::uint64 {
    let my_proc = unsafe { c_bindings::myproc().as_mut() };
    if let Some(my_proc) = my_proc {
        let interval = argint(0);
        match interval.cmp(&0) {
            core::cmp::Ordering::Greater => {
                let alarm_handler = argaddr(1) as *const ();
                if alarm_handler.is_null() {
                    u64::MAX
                } else {
                    my_proc.alarm_interval = interval;
                    my_proc.alarm_handler = Some(unsafe { core::mem::transmute(alarm_handler) });
                    my_proc.ticks_since_last_alarm = 0;
                    0
                }
            }
            core::cmp::Ordering::Equal => {
                let alarm_handler_integer = argaddr(1);
                if alarm_handler_integer == 0 {
                    my_proc.alarm_interval = 0;
                    my_proc.alarm_handler = None;
                    my_proc.ticks_since_last_alarm = 0;
                    0
                } else {
                    u64::MAX
                }
            }
            core::cmp::Ordering::Less => u64::MAX,
        }
    } else {
        u64::MAX
    }
}

#[no_mangle]
pub extern "C" fn sys_sigreturn() -> c_bindings::uint64 {
    let my_proc = unsafe { c_bindings::myproc().as_mut() };
    if let Some(my_proc) = my_proc {
        unsafe {
            ptr::copy_nonoverlapping(ptr::addr_of!(my_proc.alarm_trapframe), my_proc.trapframe, 1);
        }
        my_proc.in_alarm_handler = 0;
        if let Some(trapframe) = unsafe { my_proc.trapframe.as_ref() } {
            trapframe.a0
        } else {
            u64::MAX
        }
    } else {
        u64::MAX
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
