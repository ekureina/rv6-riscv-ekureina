use bitfield::{bitfield, BitMut};
use core::alloc::Layout;

use crate::c_bindings;
use crate::printf::{panic, printf};

bitfield! {
    /// A wrapper around a Sv39 Page Table Entry
    #[derive(PartialEq, Eq)]
    #[repr(transparent)]
    pub struct PageTableEntry(u64);
    impl Debug;
    /// Find if the referenced page is valid
    pub valid, set_valid: 0;
    /// Can this page be read?
    pub readable, set_readable: 1;
    /// Can this page be written to?
    pub writeable, set_writeable: 2;
    /// Can memory in this page be executed?
    pub executable, set_executable: 3;
    /// Can user code access this page?
    pub user_accessible, set_user_accessible: 4;
    /// Has this page been accessed since the last reset?
    /// Must be cleared by [`clear_accessed`]
    pub accessed, _: 6;
    /// Has this page been written since the last reset?
    /// Must be cleared by [`clear_dirty`]
    pub dirty, _: 7;
    /// The RSW field, used by rv6 to track COWs
    pub u8, rsw, set_rsw: 9, 8;
    /// Physical Page Numbers to map to
    pub u8, into usize, ppn, set_ppn: 18, 10, 3;
}

impl PageTableEntry {
    /// Clear the accessed bit on the Page Table Entry
    /// Cannot set this bit, only read and clear
    pub fn clear_accessed(&mut self) {
        self.0.set_bit(6, false);
    }

    /// Clear the accessed bit on the Page Table Entry
    /// Cannot set this bit, only read and clear
    pub fn clear_dirty(&mut self) {
        self.0.set_bit(7, false);
    }
}

impl From<PageTableEntry> for u64 {
    fn from(value: PageTableEntry) -> Self {
        value.0
    }
}

impl From<u64> for PageTableEntry {
    fn from(value: u64) -> Self {
        PageTableEntry(value)
    }
}

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

/// Given a parent process's page table, copy its memory into
/// a child's page table. Copies both the page table and the
/// physical memory.
/// returns 0 on success, -1 on failure.
/// frees any allocated pages on failure.
/// # Safety
/// Assumes that the given page tables are valid page tables
#[no_mangle]
pub unsafe extern "C" fn uvmcopy(
    old_pagetable: c_bindings::pagetable_t,
    new_pagetable: c_bindings::pagetable_t,
    size: u64,
) -> core::ffi::c_int {
    for va in (0..size).step_by(c_bindings::PGSIZE as usize) {
        let old_pte = c_bindings::walk(old_pagetable, va, 0);
        if old_pte.is_null() {
            panic!("uvmcopy: pte should exist");
        }

        if (*old_pte & u64::from(c_bindings::PTE_V)) == 0 {
            panic!("uvmcopy: page not present");
        }

        let pa = PTE2PA!(*old_pte);
        let flags = PTE_FLAGS!(*old_pte);

        let layout = Layout::from_size_align_unchecked(
            c_bindings::PGSIZE as usize,
            c_bindings::PGSIZE as usize,
        );
        let mem = alloc::alloc::alloc(layout);
        if mem.is_null() {
            c_bindings::uvmunmap(new_pagetable, 0, va / u64::from(c_bindings::PGSIZE), 1);
            return -1;
        }

        core::ptr::copy_nonoverlapping(pa as *const u8, mem, c_bindings::PGSIZE as usize);
        if c_bindings::mappages(
            new_pagetable,
            va,
            u64::from(c_bindings::PGSIZE),
            mem as u64,
            flags as i32,
        ) != 0
        {
            alloc::alloc::dealloc(mem, layout);
            c_bindings::uvmunmap(new_pagetable, 0, va / u64::from(c_bindings::PGSIZE), 1);
            return -1;
        }
    }
    0
}

macro_rules! PTE2PA {
    ($pte:expr) => {
        (($pte) >> 10) << 12
    };
}

macro_rules! PTE_FLAGS {
    ($pte:expr) => {
        $pte & 0x3FF
    };
}

pub(crate) use PTE2PA;
pub(crate) use PTE_FLAGS;
