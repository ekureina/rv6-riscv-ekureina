pub(crate) static SSTATUS_SIE: u64 = 1 << 1; // Supervisor Interrupt Enable
pub(crate) static SSTATUS_SPP: u64 = 1 << 8; // Previous mode, 1=Supervisor, 0=User

macro_rules! r_fp {
    () => {{
        let fp: u64;
        unsafe {
            core::arch::asm!("mv {0}, s0", options(nomem, nostack), out(reg) fp);
        }
        fp
    }}
}

macro_rules! r_sstatus {
    () => {{
        let sstatus: u64;
        unsafe {
            core::arch::asm!("csrr {0}, sstatus", options(nomem, nostack), out(reg) sstatus);
        }
        sstatus
    }}
}

macro_rules! w_sstatus {
    ($val:expr) => {{
        unsafe {
            let evaluated = $val;
            core::arch::asm!("csrw sstatus, {0}", options(nomem, nostack), in(reg) evaluated);
        }
    }}
}

macro_rules! w_stvec {
    ($val:expr) => {{
        unsafe {
            let evaluated = $val;
            core::arch::asm!("csrw stvec, {0}", options(nomem, nostack), in(reg) evaluated);
        }
    }}
}

macro_rules! r_sepc {
    () => {{
        let sepc: u64;
        unsafe {
            core::arch::asm!("csrr {0}, sepc", options(nomem, nostack), out(reg) sepc);
        }
        sepc
    }}
}

macro_rules! r_scause {
    () => {{
        let scause: u64;
        unsafe {
            core::arch::asm!("csrr {0}, scause", options(nomem, nostack), out(reg) scause);
        }
        scause
    }}
}

macro_rules! r_stval {
    () => {{
        let stval: u64;
        unsafe {
            core::arch::asm!("csrr {0}, stval", options(nomem, nostack), out(reg) stval);
        }
        stval
    }};
}

macro_rules! intr_on {
    () => {
        #[allow(unused_unsafe)]
        {
            $crate::riscv_asm::w_sstatus!(
                $crate::riscv_asm::r_sstatus!() | $crate::riscv_asm::SSTATUS_SIE
            );
        }
    };
}

macro_rules! intr_off {
    () => {
        #[allow(unused_unsafe)]
        {
            $crate::riscv_asm::w_sstatus!(
                $crate::riscv_asm::r_sstatus!() & !$crate::riscv_asm::SSTATUS_SIE
            );
        }
    };
}

macro_rules! intr_get {
    () => {{
        $crate::riscv_asm::r_sstatus!() & $crate::riscv_asm::SSTATUS_SIE != 0
    }};
}

macro_rules! page_round_down {
    ($address:expr) => {
        $address & !($crate::c_bindings::PGSIZE as u64 - 1)
    };
}

pub(crate) use intr_get;
pub(crate) use intr_off;
pub(crate) use intr_on;
pub(crate) use page_round_down;
pub(crate) use r_fp;
pub(crate) use r_scause;
pub(crate) use r_sepc;
pub(crate) use r_sstatus;
pub(crate) use r_stval;
pub(crate) use w_sstatus;
pub(crate) use w_stvec;
