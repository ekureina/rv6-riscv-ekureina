macro_rules! r_fp {
    () => {{
        let fp: u64;
        unsafe {
            core::arch::asm!("mv {0}, s0", options(readonly), out(reg) fp);
        }
        fp
    }}
}

macro_rules! page_round_down {
    ($address:expr) => {
        $address & !($crate::c_bindings::PGSIZE as u64 - 1)
    };
}

pub(crate) use page_round_down;
pub(crate) use r_fp;
