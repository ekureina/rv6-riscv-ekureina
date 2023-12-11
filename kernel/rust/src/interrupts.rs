use crate::{
    c_bindings::{mycpu, panic},
    riscv_asm::{intr_get, intr_off, intr_on},
};

/// Turns off interrupts
/// Must be matched by an equal number of [`pop_off`]s
#[no_mangle]
pub extern "C" fn push_off() {
    let old = intr_get!();
    intr_off!();
    if let Some(cpu) = unsafe { mycpu().as_mut() } {
        if cpu.noff == 0 {
            cpu.intena = i32::from(old);
        }
        cpu.noff += 1;
    }
}

/// Reverses one [`push_off`]
/// Turns on interrupts if this is matches the outermost [`push_off`]
#[no_mangle]
pub extern "C" fn pop_off() {
    if intr_get!() {
        unsafe {
            panic(
                b"pop_off - interruptible\0"
                    .as_ptr()
                    .cast::<i8>()
                    .cast_mut(),
            );
        }
    }
    if let Some(cpu) = unsafe { mycpu().as_mut() } {
        if cpu.noff < 1 {
            unsafe {
                panic(b"pop_off\0".as_ptr().cast::<i8>().cast_mut());
            }
        }
        cpu.noff -= 1;
        if cpu.noff == 0 && cpu.intena != 0 {
            intr_on!();
        }
    }
}
