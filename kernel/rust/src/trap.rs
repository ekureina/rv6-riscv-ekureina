use core::alloc::Layout;

use crate::c_bindings;
use crate::printf::{panic, printf};
use crate::riscv_asm::{intr_on, r_scause, r_sepc, r_sstatus, r_stval, w_stvec, SSTATUS_SPP};
use crate::vm::{PageTableEntry, PGROUNDDOWN, RSW};

extern "C" {
    pub fn kernelvec();
    pub fn devintr() -> core::ffi::c_int;
}

/// handle an interrupt, exception, or system call from user space.
/// called from trampoline.S
/// # Panics
/// Panics if no process exists
#[no_mangle]
#[allow(clippy::too_many_lines)]
pub extern "C" fn usertrap() {
    let mut which_dev = 0;

    if r_sstatus!() & SSTATUS_SPP != 0 {
        panic!("usertrap: not from user mode\0");
    }
    w_stvec!(kernelvec);

    let proc = unsafe { c_bindings::myproc().as_mut() }.unwrap();
    let trapframe = unsafe { proc.trapframe.as_mut() }.unwrap();
    trapframe.epc = r_sepc!();
    if r_scause!() == 8 {
        if unsafe { c_bindings::killed(proc) } != 0 {
            unsafe {
                c_bindings::exit(-1);
            }
        }

        // sepc points to the ecall instruction,
        // but we want to return to the next instruction.
        trapframe.epc += 4;

        // an interrput will change sepc, scause, and sstatuts,
        // so enable only now that we're done with those registers.
        intr_on!();

        unsafe {
            c_bindings::syscall();
        }
    } else {
        which_dev = unsafe { devintr() };
        match which_dev {
            #[allow(unused_unsafe)]
            0 => {
                printf!(
                    "usertrap(): unexpected scause %p pid=%d\n\0",
                    r_scause!(),
                    proc.pid
                );
                printf!("            sepc=%p stval=%p\n\0", r_sepc!(), r_stval!());
                unsafe {
                    c_bindings::setkilled(proc);
                }
            }
            2 => {
                if proc.alarm_interval > 0 {
                    proc.ticks_since_last_alarm += 1;
                }

                if proc.ticks_since_last_alarm == proc.alarm_interval && proc.in_alarm_handler != 1
                {
                    proc.alarm_trapframe = *trapframe;
                    proc.in_alarm_handler = 1;
                    proc.ticks_since_last_alarm = 0;
                    unsafe {
                        c_bindings::usertrapret(
                            u64::try_from(proc.alarm_handler.map_or(0usize, |ptr| ptr as usize))
                                .unwrap(),
                        );
                    }
                }
            }
            3 => {
                let va_write_fault_page = PGROUNDDOWN!(r_stval!());
                match unsafe {
                    c_bindings::walk(proc.pagetable, va_write_fault_page, 0)
                        .cast::<PageTableEntry>()
                        .as_mut()
                } {
                    None => unsafe { c_bindings::setkilled(proc) },
                    Some(va_pte) => {
                        if va_pte.rsw() == RSW::COWPage {
                            va_pte.set_rsw(RSW::Default);
                            let page_size = c_bindings::PGSIZE as usize;
                            let layout =
                                unsafe { Layout::from_size_align_unchecked(page_size, page_size) };
                            let new_page = unsafe { alloc::alloc::alloc(layout) };
                            if new_page.is_null() {
                                unsafe { c_bindings::setkilled(proc) };
                            } else {
                                let pa = va_pte.pa_mut().as_mut_ptr();
                                unsafe { core::ptr::copy_nonoverlapping(pa, new_page, page_size) };
                                va_pte.set_writeable(true);
                                va_pte.set_valid(false);
                                if unsafe {
                                    c_bindings::mappages(
                                        proc.pagetable,
                                        va_write_fault_page,
                                        c_bindings::PGSIZE.into(),
                                        new_page as u64,
                                        va_pte.get_flags().try_into().unwrap(),
                                    )
                                } == -1
                                {
                                    unsafe { c_bindings::setkilled(proc) };
                                } else {
                                    let layout = unsafe {
                                        Layout::from_size_align_unchecked(
                                            c_bindings::PGSIZE as usize,
                                            c_bindings::PGSIZE as usize,
                                        )
                                    };
                                    unsafe { alloc::alloc::dealloc(pa, layout) };
                                }
                            }
                        } else {
                            unsafe { c_bindings::setkilled(proc) }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if unsafe { c_bindings::killed(proc) } != 0 {
        unsafe {
            c_bindings::exit(-1);
        }
    }

    if which_dev == 2 {
        unsafe { c_bindings::yield_() };
    }

    unsafe { c_bindings::usertrapret(0) };
}
