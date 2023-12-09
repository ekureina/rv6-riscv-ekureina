use crate::c_bindings;

static DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Prints out a bactrace for the current stack
/// Relies on the existence of frame pointers!
#[no_mangle]
pub extern "C" fn backtrace() {
    let mut fp = crate::riscv_asm::r_fp!();
    let max_stack_addr = crate::riscv_asm::page_round_down!(fp) + u64::from(c_bindings::PGSIZE);
    while fp < max_stack_addr {
        let return_addr = unsafe { *(fp as *const u64).offset(-1) };
        printf!(b"%p\n\0", return_addr);
        fp = unsafe { *(fp as *const u64).offset(-2) };
    }
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn printint(int: i32, base: i32) {
    let mut buf: [u8; 16] = Default::default();
    let is_positive = int.is_positive();
    let mut x = int.abs();
    let mut i = 0usize;
    loop {
        buf[i] = DIGITS[usize::try_from(x % base).unwrap()];
        i += 1;
        x /= base;
        if x == 0 {
            break;
        }
    }

    if !is_positive {
        buf[i] = b'-';
    }

    for index in (0..=i).rev() {
        unsafe { c_bindings::consputc(i32::from(buf[index])) };
    }
}

#[no_mangle]
pub extern "C" fn printptr(mut pointer: u64) {
    unsafe {
        c_bindings::consputc(i32::from(b'0'));
        c_bindings::consputc(i32::from(b'x'));
    }

    for _ in 0..(core::mem::size_of::<u64>() * 2) {
        unsafe {
            c_bindings::consputc(i32::from(DIGITS[(pointer >> (u64::BITS - 4)) as usize]));
        }
        pointer <<= 4;
    }
}

macro_rules! printf {
    ($fmt:literal, $($args:expr),+) => {
        unsafe { $crate::c_bindings::printf($fmt.as_ptr().cast::<i8>().cast_mut(), $($args),+) }
    };
    ($lit:literal) => {
        unsafe { $crate::c_bindings::printf($lit.as_ptr().cast::<i8>().cast_mut()) }
    };
}

pub(crate) use printf;
