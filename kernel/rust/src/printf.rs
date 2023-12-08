use crate::c_bindings;

/// Prints out a bactrace for the current stack
/// Relies on the existence of frame pointers!
#[no_mangle]
pub extern "C" fn backtrace() {
    let mut fp = crate::riscv_asm::r_fp!();
    let max_stack_addr = crate::riscv_asm::page_round_down!(fp) + c_bindings::PGSIZE as u64;
    while fp < max_stack_addr {
        let return_addr = unsafe { *(fp as *const u64).offset(-1) };
        unsafe { c_bindings::printf(b"%p\n\0".as_ptr().cast::<i8>().cast_mut(), return_addr) };
        fp = unsafe { *(fp as *const u64).offset(-2) };
    }
}
