use core::alloc::Layout;

use bitfield::{bitfield, BitMut, BitRange};
use num_enum::{FromPrimitive, IntoPrimitive};

use crate::c_bindings;
use crate::kalloc::ALLOCATOR;
use crate::printf::{panic, printf};

bitfield! {
    /// A wrapper around a Sv39 Page Table Entry
    #[derive(PartialEq, Eq, Copy, Clone)]
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
    pub u8, from into RSW, rsw, set_rsw: 9, 8;
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

    /// Map this PTE to a physical address
    #[must_use]
    pub fn map_pa(&self) -> u64 {
        (self.0 >> 10) << 12
    }

    /// Get the flag bits in this PTE
    #[must_use]
    pub fn get_flags(&self) -> u64 {
        self.bit_range(7, 0)
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

/// Values set in the RSW field of the [`PageTableEntry`]
#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Default, Copy, Clone, IntoPrimitive, FromPrimitive)]
pub enum RSW {
    #[default]
    /// Default value of the RSW
    Default,
    /// Set if the page in question is a COWable page (Writeable, but COW'd)
    COWPage,
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
#[allow(clippy::missing_panics_doc)]
pub unsafe extern "C" fn uvmcopy(
    old_pagetable: c_bindings::pagetable_t,
    new_pagetable: c_bindings::pagetable_t,
    size: u64,
) -> core::ffi::c_int {
    for va in (0..size).step_by(c_bindings::PGSIZE as usize) {
        match unsafe {
            c_bindings::walk(old_pagetable, va, 0)
                .cast::<PageTableEntry>()
                .as_mut()
        } {
            None => {
                panic!("uvmcopy: pte should exist\0");
            }
            Some(old_pte) => {
                if !old_pte.valid() {
                    panic!("uvmcopy: page not present");
                }
                let writeable = old_pte.writeable();
                if writeable {
                    old_pte.set_rsw(RSW::COWPage);
                    old_pte.set_writeable(false);
                }
                let pa = old_pte.map_pa();
                let flags = old_pte.get_flags();
                if unsafe {
                    c_bindings::mappages(
                        new_pagetable,
                        va,
                        u64::from(c_bindings::PGSIZE),
                        pa,
                        i32::try_from(flags).unwrap(),
                    )
                } != 0
                {
                    if writeable {
                        old_pte.set_writeable(true);
                        old_pte.set_rsw(RSW::COWPage);
                    }
                    return -1;
                }

                if writeable {
                    match unsafe {
                        c_bindings::walk(new_pagetable, va, 0)
                            .cast::<PageTableEntry>()
                            .as_mut()
                    } {
                        None => return -1,
                        Some(new_pte) => {
                            new_pte.set_rsw(RSW::COWPage);
                        }
                    }
                }
                ALLOCATOR.in_place_copy(usize::try_from(pa).unwrap());
            }
        }
    }
    0
}

/// Copy from kernel to user
/// Copy len bytes from src to virtual address dstva in a given page table.
/// Return 0 on sucess, -1 on error.
/// # Safety
/// Caller ensures that the data is in bounds, as needed
#[no_mangle]
#[allow(clippy::similar_names, clippy::missing_panics_doc)]
pub unsafe extern "C" fn copyout(
    pagetable: c_bindings::pagetable_t,
    mut dstva: c_bindings::uint64,
    mut src: *const u8,
    mut len: c_bindings::uint64,
) -> core::ffi::c_int {
    while len > 0 {
        let va0 = PGROUNDDOWN!(dstva);
        if va0 >= c_bindings::MAXVA {
            return -1;
        }

        let pte = unsafe {
            c_bindings::walk(pagetable, va0, 0)
                .cast::<PageTableEntry>()
                .as_mut()
        };
        match pte {
            None => return -1,
            Some(pte) => {
                if !pte.valid() || !pte.user_accessible() {
                    return -1;
                }
                // Do we need to cow this page?
                if !pte.writeable() && pte.rsw() == RSW::COWPage {
                    let old_pa = pte.map_pa() as *const u8;
                    let page_size = c_bindings::PGSIZE as usize;
                    let layout = unsafe { Layout::from_size_align_unchecked(page_size, page_size) };
                    let new_page = unsafe { alloc::alloc::alloc(layout) };
                    // Out of memory, abort the copy
                    if new_page.is_null() {
                        return -1;
                    }
                    pte.set_writeable(true);
                    pte.set_valid(false);
                    pte.set_rsw(RSW::Default);
                    // Map the page, and copy data to the COW'd page
                    if unsafe {
                        c_bindings::mappages(
                            pagetable,
                            va0,
                            c_bindings::PGSIZE.into(),
                            new_page as u64,
                            pte.get_flags().try_into().unwrap(),
                        )
                    } == -1
                    {
                        return -1;
                    }
                    unsafe { core::ptr::copy_nonoverlapping(old_pa, new_page, page_size) };
                }

                let pa0 = pte.map_pa();
                let mut n = c_bindings::PGSIZE as usize - usize::try_from(dstva - va0).unwrap();
                if n > usize::try_from(len).unwrap() {
                    n = usize::try_from(len).unwrap();
                }
                unsafe {
                    core::ptr::copy(src, (pa0 + (dstva - va0)) as *mut u8, n);
                }

                len -= u64::try_from(n).unwrap();
                src = unsafe { src.offset(isize::try_from(n).unwrap()) };
                dstva = va0 + u64::from(c_bindings::PGSIZE);
            }
        }
    }
    0
}

macro_rules! PGROUNDUP {
    ($e:expr) => {
        ($e as u64 + $crate::c_bindings::PGSIZE as u64 - 1)
            & !($crate::c_bindings::PGSIZE as u64 - 1)
    };
}

macro_rules! PGROUNDDOWN {
    ($e:expr) => {
        $e as u64 & !($crate::c_bindings::PGSIZE as u64 - 1)
    };
}

pub(crate) use PGROUNDDOWN;
pub(crate) use PGROUNDUP;
