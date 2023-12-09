use crate::c_bindings;
use crate::printf::printf;

/// Prints out the mapped pages of the page table
/// # Safety
/// Assumes that the page table passed in is a valid page table
#[no_mangle]
pub unsafe extern "C" fn vmprint(pagetable: c_bindings::pagetable_t) {
    printf!(b"page table  %p\n\0", pagetable);

    vmprint_subtable(pagetable, 3u16);
}

unsafe fn vmprint_subtable(pagetable: c_bindings::pagetable_t, level: u16) {
    if level == 0 {
        return;
    }

    for pte_index in 0..512isize {
        let pte: *const c_bindings::pte_t = pagetable.offset(pte_index);
        if (*pte & u64::from(c_bindings::PTE_V)) != 0 {
            let pte_va = (*pte >> 10) << 12;
            for _ in 0..(4 - level) {
                printf!(b"..\0");
            }
            printf!(b"%d: pte %p pa %p\n\0", pte_index, *pte, pte_va);
            vmprint_subtable(pte_va as c_bindings::pagetable_t, level - 1);
        }
    }
}
