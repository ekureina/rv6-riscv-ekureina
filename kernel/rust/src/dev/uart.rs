use core::cell::Cell;

use crate::sync::spinlock::Spintex;

pub(crate) static UART: UartDev = UartDev {
    uart_tx_buf: Spintex::new(Cell::new([0; 32]), "uart"),
};

#[derive(Debug)]
pub(crate) struct UartDev<'a> {
    uart_tx_buf: Spintex<'a, Cell<[core::ffi::c_char; 32]>>,
}

impl UartDev<'_> {
    const UART0: *mut u8 = 0x10000000 as *mut u8;
    // the UART control registers.
    // some have different meanings for read vs write.
    // see http://byterunner.com/16550.html
    /// Recieve holding register (for input bytes)
    const RHR: usize = 0;
    /// Transmit holding register (for output bytes)
    const THR: usize = 0;
    /// Interrupt enable register
    const IER: usize = 1;
    const IER_RX_ENABLE: u8 = 1 << 0;
    const IER_TX_ENABLE: u8 = 1 << 1;
    /// FIFO control register
    const FCR: usize = 2;
    const FCR_FIFO_ENABLE: u8 = 1 << 0;
    // Clear the content of the two FIFOs
    const FCR_FIFO_CLEAR: u8 = 3 << 1;
    /// Interrupt Status Register
    const ISR: usize = 2;
    /// Line Control Register
    const LCR: usize = 3;
    const LCR_EIGHT_BITS: u8 = 3 << 0;
    /// Special mode to set baud rate
    const LCR_BAUD_LATCH: u8 = 1 << 7;
    /// Line Status Register
    const LSR: usize = 5;

    pub(crate) fn init() {
        unsafe {
            // disable interrupts
            Self::write_reg(Self::IER, 0x00);
            // Special mode to set baud rate.
            Self::write_reg(Self::LCR, Self::LCR_BAUD_LATCH);
            // LSB for baud rate of 38.4K
            Self::write_reg(0, 0x3);
            // MSB for baud rate of 38.4k
            Self::write_reg(1, 0x00);
            // Leave set-baud mode,
            // and set word length to 8 bits, no parity.
            Self::write_reg(Self::LCR, Self::LCR_EIGHT_BITS);
            // Reset and enable FIFOs.
            Self::write_reg(Self::FCR, Self::FCR_FIFO_ENABLE | Self::FCR_FIFO_CLEAR);
            // enable transmit and recieve interrupts
            Self::write_reg(Self::IER, Self::IER_TX_ENABLE | Self::IER_RX_ENABLE);
        }
    }

    pub(crate) unsafe fn write_reg(reg_num: usize, data: u8) {
        unsafe {
            core::ptr::write_volatile(Self::UART0.add(reg_num), data);
        }
    }
}
