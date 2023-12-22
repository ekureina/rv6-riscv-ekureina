use crate::c_bindings;
use crate::printf::{panic, printf};
use crate::riscv_asm::{intr_on, r_scause, r_sepc, r_sstatus, r_stval, w_stvec, SSTATUS_SPP};

extern "C" {
    pub fn kernelvec();
    pub fn devintr() -> core::ffi::c_int;
}

/// handle an interrupt, exception, or system call from user space.
/// called from trampoline.S
/// # Panics
/// Panics if no process exists
#[no_mangle]
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
                    unsafe {
                        core::ptr::copy(
                            proc.trapframe.cast_const(),
                            core::ptr::addr_of_mut!(proc.alarm_trapframe),
                            1,
                        );
                    }
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
