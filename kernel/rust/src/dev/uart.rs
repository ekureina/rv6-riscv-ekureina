use core::ptr::NonNull;

use bitflags::bitflags;

use crate::{
    c_bindings,
    interrupts::{pop_off, push_off},
    proc::sleep_rust,
    sync::spinlock::Spintex,
};

use super::console::CONSOLE;

macro_rules! bitflags_to_primitive {
    ($flag_struct:ident, $primative:ty$(;)?) => {
        impl From<$primative> for $flag_struct {
            fn from(value: $primative) -> Self {
                $flag_struct::from_bits_retain(value)
            }
        }

        impl From<$flag_struct> for $primative {
            fn from(value: $flag_struct) -> Self {
                value.bits()
            }
        }
    };
    ($flag_struct:ident, $primative:ty; $($flag_structs:ident, $primatives:ty);+$(;)?) => {
        bitflags_to_primitive!($flag_struct, $primative);
        bitflags_to_primitive!($($flag_structs, $primatives);+);
    };
}

bitflags! {
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct InterruptEnableRegister: u8 {
        const RECIEVE_HOLDING_REGISTER_INTERRUPT = 1;
        const TRANSMIT_HOLDING_REGISTER_INTERRUPT = 1 << 1;
        const RECIEVE_LINE_STATUS_INTERRUP = 1 << 2;
        const MODEM_STATUS_INTERRUPT = 1 << 3;

        const _ = 0xf0;
    }

    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct FIFOControlRegister: u8 {
        const FIFO_ENABLE = 1;
        const RECIEVER_FIFO_RESET = 1 << 1;
        const TRANSMIT_FIFO_RESET = 1 << 2;
        const FIFO_RESET = 3 << 1;
        const DMA_MODE_SELECT = 1 << 3;
        const RECIEVER_TRIGGER_LSB = 1 << 6;
        const RECIEVER_TRIGGER_MSB = 1 << 7;

        const _ = 0x30;
    }

    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct InterruptStatusRegister: u8 {
        const INTERRUPT_STATUS = 1;
        const INTERRUPT_PRIOR_BIT_0 = 1 << 1;
        const INTERRUPT_PRIOR_BIT_1 = 1 << 2;
        const INTERRUPT_PRIOR_BIT_2 = 1 << 3;

        const _ = 0xf0;
    }

    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct LineControlRegister: u8 {
        const WORD_LENGTH_BIT_0 = 1;
        const WORD_LENGTH_BIT_1 = 1 << 1;
        const STOP_BITS = 1 << 2;
        const PARITY_ENABLE = 1 << 3;
        const EVEN_PARITY = 1 << 4;
        const SET_PARITY = 1 << 5;
        const SET_BREAK = 1 << 6;
        const DIVISOR_LATCH_ENABLE = 1 << 7;
        const EIGHT_BITS = 3;
    }

    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    struct LineStatusRegister: u8 {
        const RECIEVE_DATA_READY = 1;
        const OVERRUN_ERROR = 1 << 1;
        const PARITY_ERROR = 1 << 2;
        const FRAMING_ERROR = 1 << 3;
        const BREAK_INTERRUPT = 1 << 4;
        const TRANSMIT_HOLDING_EMPTY = 1 << 5;
        const TRANSMIT_EMPTY = 1 << 6;
        const FIFO_ERROR = 1 << 7;
    }
}

struct BaudRate {
    pub two_mhz_clock: u8,
    pub seven_mhz_clock: u8,
}

bitflags_to_primitive!(
    InterruptEnableRegister, u8;
    FIFOControlRegister, u8;
    InterruptStatusRegister, u8;
    LineControlRegister, u8;
    LineStatusRegister, u8;
);

macro_rules! read_write_reg {
    ($read:ident, $write: ident, $reg_type:ident, $reg_num:literal$(;)?) => {
        fn $read() -> $reg_type {
            unsafe { Self::read_reg($reg_num) }
        }

        fn $write(data: $reg_type) {
            unsafe { Self::write_reg($reg_num, data) }
        }
    };
    ($read:ident, $write:ident, $reg_type:ident, $reg_num:literal; $($reads:ident, $writes:ident, $reg_types:ident, $reg_nums:literal);+$(;)?) => {
        read_write_reg!($read, $write, $reg_type, $reg_num);
        read_write_reg!($($reads, $writes, $reg_types, $reg_nums);+);
    };
}

#[derive(Debug, Default, Copy, Clone)]
#[allow(clippy::struct_field_names)]
struct UartBuffer {
    pub tx_buffer: [u8; 32],
    pub tx_w: usize, // write next to tx_buf[tx_w % tx_buffer.len()]
    pub tx_r: usize, // read next from tx_buf[tx_r % tx_buffer.len()]
}

impl UartBuffer {
    const fn new() -> Self {
        Self {
            tx_buffer: [0; 32],
            tx_w: 0,
            tx_r: 0,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct UartDev<'a> {
    tx_buf: Spintex<'a, UartBuffer>,
}

impl UartDev<'_> {
    /// Base of UART0 address space
    const UART0: *mut u8 = 0x1000_0000 as *mut u8;
    /// Baud Rate: 38.4K <http://byterunner.com/16550.html>
    const BAUD_RATE: BaudRate = BaudRate {
        two_mhz_clock: 3,
        seven_mhz_clock: 12,
    };

    pub(super) const fn new() -> Self {
        Self {
            tx_buf: Spintex::new(UartBuffer::new(), "uart"),
        }
    }

    pub(crate) fn init() {
        // disable interrupts
        Self::write_ier(InterruptEnableRegister::empty());
        // Set the Baud rate
        Self::set_baud_rate(&Self::BAUD_RATE);
        // Leave set-baud mode,
        // and set word length to 8 bits, no parity.
        Self::write_lcr(LineControlRegister::EIGHT_BITS);
        // Reset and enable FIFOs.
        Self::write_fcr(FIFOControlRegister::FIFO_ENABLE | FIFOControlRegister::FIFO_RESET);
        // enable transmit and recieve interrupts
        Self::write_ier(
            InterruptEnableRegister::RECIEVE_HOLDING_REGISTER_INTERRUPT
                | InterruptEnableRegister::TRANSMIT_HOLDING_REGISTER_INTERRUPT,
        );
    }

    pub(crate) fn putc_sync(character: u8) {
        push_off();

        // Wait for Transmit Holding Empty to be set in LSR.
        while !Self::read_lsr().contains(LineStatusRegister::TRANSMIT_HOLDING_EMPTY) {
            core::hint::spin_loop();
        }

        Self::write_thr(character);

        pop_off();
    }

    pub(crate) fn putc(&self, character: u8) {
        let mut tx_buf = self.tx_buf.lock();

        while tx_buf.tx_w == tx_buf.tx_r + tx_buf.tx_buffer.len() {
            let tx_r_ptr = NonNull::from(&tx_buf.tx_r);
            sleep_rust(tx_r_ptr, tx_buf);
            tx_buf = self.tx_buf.lock();
        }
        let index = tx_buf.tx_w % tx_buf.tx_buffer.len();
        tx_buf.tx_buffer[index] = character;
        tx_buf.tx_w = tx_buf.tx_w.wrapping_add(1);
        Self::start(&mut tx_buf);
    }

    // if the UART is idle, and a character is waiting
    // in the transmit buffer, send it.
    // caller must hold uart_tx_lock.
    // called from both the top- and bottom-half.
    fn start(buf: &mut UartBuffer) {
        loop {
            if buf.tx_w == buf.tx_r {
                // Transmit buffer is empty.
                return;
            }

            if !Self::read_lsr().contains(LineStatusRegister::TRANSMIT_HOLDING_EMPTY) {
                // the UART transmit holding register is full,
                // so we cannot give it another byte.
                // it will interrupt when it's ready for a new byte.
                return;
            }

            let index = buf.tx_r % buf.tx_buffer.len();
            let character = buf.tx_buffer[index];
            buf.tx_r = buf.tx_r.wrapping_add(1);

            // maybe uartputc() is waiting for space in the buffer.
            unsafe {
                c_bindings::wakeup(core::ptr::addr_of_mut!(buf.tx_r).cast());
            }

            Self::write_thr(character);
        }
    }

    /// Reads a character from the UART, if ready
    pub(crate) fn getc() -> Option<u8> {
        if Self::read_lsr().contains(LineStatusRegister::RECIEVE_DATA_READY) {
            Some(Self::read_rhr())
        } else {
            None
        }
    }

    /// handle a uart interrupt, raised because input has arrived
    /// or the uart is ready for more output, or both. called from [`devintr`].
    pub(crate) fn intr(&self) {
        // read and process incoming characters
        loop {
            match UartDev::getc() {
                None => break,
                Some(character) => CONSOLE.intr(character.into()),
            }
        }

        let mut tx_buf = self.tx_buf.lock();
        Self::start(&mut tx_buf);
    }

    fn set_baud_rate(rate: &BaudRate) {
        Self::write_lcr(LineControlRegister::DIVISOR_LATCH_ENABLE);
        unsafe {
            Self::write_reg(0, rate.two_mhz_clock);
            Self::write_reg(1, rate.seven_mhz_clock);
        }
    }

    read_write_reg!(
        // the UART control registers.
        // some have different meanings for read vs write.
        // see http://byterunner.com/16550.html
        // transmit holding register (for output bytes)
        read_thr, write_thr, u8, 0;
        // Recieve holding register (for input bytes)
        read_rhr, write_hrh, u8, 0;
        // Interrupt enable register
        read_ier, write_ier, InterruptEnableRegister, 1;
        read_fcr, write_fcr, FIFOControlRegister, 2;
        read_isr, write_isr, InterruptStatusRegister, 2;
        read_lcr, write_lcr, LineControlRegister, 3;
        read_lsr, write_lsr, LineStatusRegister, 5;
    );

    unsafe fn write_reg(reg_num: usize, data: impl Into<u8>) {
        unsafe {
            core::ptr::write_volatile(Self::UART0.add(reg_num), data.into());
        }
    }

    unsafe fn read_reg<T: From<u8>>(reg_num: usize) -> T {
        unsafe { core::ptr::read_volatile(Self::UART0.add(reg_num)).into() }
    }
}

impl core::fmt::Write for UartDev<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.as_bytes() {
            self.putc(*byte);
        }
        Ok(())
    }
}

#[no_mangle]
pub extern "C" fn uartintr() {
    CONSOLE.uart.intr();
}
