use crate::c_bindings;

use super::uart::{UartDev, UART};

pub(crate) struct Console {}

impl Console {
    const BACKSPACE: core::ffi::c_int = 0x100;
    const BACKSPACE_CHAR: u8 = 8;

    /// Send one character to the uart.
    /// called by printf(), and to echo input characters,
    /// but not from write().
    pub(crate) fn putc(character: core::ffi::c_int) {
        if character == Self::BACKSPACE {
            UartDev::putc_sync(Self::BACKSPACE_CHAR);
            UartDev::putc_sync(b' ');
            UartDev::putc_sync(Self::BACKSPACE_CHAR);
        } else {
            UartDev::putc_sync((character & 0xff).try_into().unwrap());
        }
    }

    /// user write()s to the console go here
    pub(crate) fn write(user_src: i32, src: u64, n: i32) -> i32 {
        let mut i = 0;
        loop {
            if i >= n {
                break;
            }
            let mut c: u8 = 0;
            #[allow(clippy::cast_sign_loss)]
            if unsafe {
                c_bindings::either_copyin(
                    core::ptr::addr_of_mut!(c).cast(),
                    user_src,
                    src + i as u64,
                    1,
                )
            } == -1
            {
                break;
            }
            UART.putc(c);
            i += 1;
        }
        i
    }
}

#[no_mangle]
pub extern "C" fn consputc(character: core::ffi::c_int) {
    Console::putc(character);
}

#[no_mangle]
pub extern "C" fn consolewrite(
    user_src: core::ffi::c_int,
    src: c_bindings::uint64,
    n: core::ffi::c_int,
) -> core::ffi::c_int {
    Console::write(user_src, src, n)
}
