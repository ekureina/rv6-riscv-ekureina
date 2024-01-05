use core::ptr::NonNull;

use crate::{c_bindings, proc::sleep_rust, sync::spinlock::Spintex};

use super::uart::UartDev;

#[derive(Copy, Clone, Debug)]
struct ConsoleData {
    buf: [u8; 128],
    read_index: usize,
    write_index: usize,
    edit_index: usize,
}

impl ConsoleData {
    const fn new() -> Self {
        Self {
            buf: [0; 128],
            read_index: 0,
            write_index: 0,
            edit_index: 0,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Console<'a> {
    cons: Spintex<'a, ConsoleData>,
    pub(super) uart: UartDev<'a>,
}

pub(super) static CONSOLE: Console = Console::new();

/// Console input and output, to the uart.
/// Reads are line at a time.
/// Implements special input characters:
///   newline -- end of line
///   control-h -- backspace
///   control-u -- kill line
///   control-d -- end of file
///   control-p -- print process list
impl Console<'_> {
    const BACKSPACE: core::ffi::c_int = 0x100;
    const BACKSPACE_CHAR: u8 = 8;
    const BACKSPACE_CHAR_INT: i32 = 8;
    const CTRL_D: u8 = 4;
    const CTRL_P: i32 = 16;
    const CTRL_U: i32 = 21;

    const fn new() -> Self {
        Self {
            cons: Spintex::new(ConsoleData::new(), "cons"),
            uart: UartDev::new(),
        }
    }

    pub(crate) fn init(devsw: &mut [c_bindings::devsw]) {
        UartDev::init();
        devsw[c_bindings::CONSOLE as usize].read = Some(consoleread);
        devsw[c_bindings::CONSOLE as usize].write = Some(consolewrite);
    }

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
    pub(crate) fn write(&self, user_src: i32, src: u64, n: i32) -> i32 {
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
            self.uart.putc(c);
            i += 1;
        }
        i
    }

    pub(crate) fn read(&self, user_dst: i32, mut dst: u64, mut n: u32) -> i32 {
        let target = n;
        let mut cons = self.cons.lock();
        while n > 0 {
            // wait until interrupt handler has put some input into the buffer
            while cons.read_index == cons.write_index {
                if unsafe { c_bindings::killed(c_bindings::myproc()) } != 0 {
                    // Unlocked on return from function
                    return -1;
                }
                let cons_read_ptr = NonNull::from(&cons.read_index);
                // Sleep and restore spinlock
                sleep_rust(cons_read_ptr, cons);
                cons = self.cons.lock();
            }
            let c = cons.buf[cons.read_index % cons.buf.len()];
            cons.read_index = cons.read_index.wrapping_add(1);
            if c == Self::CTRL_D {
                // EOF
                if n < target {
                    // Save ^D for next time, to make sure caller gets a 0-byte result.
                    cons.read_index = cons.read_index.wrapping_sub(1);
                    break;
                }
            }

            // copy the input byte to the user-space buffer.
            let mut cbuf = c;
            if unsafe {
                c_bindings::either_copyout(user_dst, dst, core::ptr::addr_of_mut!(cbuf).cast(), 1)
            } == -1
            {
                break;
            }

            dst += 1;
            n -= 1;

            if c == b'\n' {
                // a whole line has arrived, return to
                // the user-level read().
                break;
            }
        }
        (target - n).try_into().unwrap()
    }

    pub(crate) fn intr(&self, c: i32) {
        let mut cons = self.cons.lock();
        match c {
            Self::CTRL_P => unsafe {
                c_bindings::procdump();
            },
            Self::CTRL_U => {
                while cons.edit_index != cons.write_index
                    && cons.buf[(cons.edit_index - 1) % cons.buf.len()] != b'\n'
                {
                    cons.edit_index -= 1;
                    Console::putc(Self::BACKSPACE);
                }
            }
            Self::BACKSPACE_CHAR_INT | 0x7F => {
                if cons.edit_index != cons.write_index {
                    cons.edit_index -= 1;
                    Console::putc(Self::BACKSPACE);
                }
            }
            _ => {
                if c != 0 && cons.edit_index - cons.read_index < cons.buf.len() {
                    let c = if c == b'\r'.into() { b'\n'.into() } else { c };
                    Console::putc(c);
                    let index = cons.edit_index % cons.buf.len();
                    cons.buf[index] = c.try_into().unwrap();
                    cons.edit_index += 1;
                    if c == b'\n'.into()
                        || c == Self::CTRL_D.into()
                        || cons.edit_index - cons.read_index == cons.buf.len()
                    {
                        let cons_read_ptr = NonNull::from(&mut cons.read_index);
                        // wake up consoleread() if a whole line (or end-of-file)
                        // has arrived
                        cons.write_index = cons.edit_index;
                        unsafe {
                            c_bindings::wakeup(cons_read_ptr.as_ptr().cast());
                        }
                    }
                }
            }
        }
    }
}

impl core::fmt::Write for Console<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.as_bytes() {
            Console::putc(i32::from(*byte));
        }
        Ok(())
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
    CONSOLE.write(user_src, src, n)
}

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn consoleread(
    user_dst: core::ffi::c_int,
    dst: c_bindings::uint64,
    n: core::ffi::c_int,
) -> core::ffi::c_int {
    CONSOLE.read(user_dst, dst, n.try_into().unwrap())
}

#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn consoleinit(devsw: *mut core::ffi::c_void) {
    let devsw = unsafe { core::slice::from_raw_parts_mut(devsw.cast(), c_bindings::NDEV as usize) };
    Console::init(devsw);
}
