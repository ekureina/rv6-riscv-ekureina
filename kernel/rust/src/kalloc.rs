use crate::c_bindings;
use crate::device_load::PHYSICAL_ADDRESS_STOP;
use crate::printf::panic;
use crate::sync::spinlock::Spintex;
use crate::vm::{PGROUNDDOWN, PGROUNDUP};
use alloc::alloc::{GlobalAlloc, Layout};
use core::cell::{Cell, OnceCell, RefCell};
use core::ptr::{self, null_mut, NonNull};

const MEM_LOCK_NAME: &str = "kmem";
const REFCOUNTS_LOCK_NAME: &str = "page_refcounts";

#[repr(C)]
struct Run {
    pub next: Cell<Option<NonNull<Run>>>,
}

pub(crate) struct KernelPageAllocator<'a> {
    freelist: Spintex<'a, Cell<Option<NonNull<Run>>>>,
    page_refcounts: Spintex<'a, RefCell<[u8; 32640]>>,
    end: OnceCell<usize>,
}

#[global_allocator]
pub(crate) static ALLOCATOR: KernelPageAllocator = KernelPageAllocator {
    freelist: Spintex::new(Cell::new(None), MEM_LOCK_NAME),
    page_refcounts: Spintex::new(RefCell::new([1u8; 32640]), REFCOUNTS_LOCK_NAME),
    end: OnceCell::new(),
};

unsafe impl<'a> Sync for KernelPageAllocator<'a> {}
unsafe impl<'a> Send for KernelPageAllocator<'a> {}

unsafe impl<'a> GlobalAlloc for KernelPageAllocator<'a> {
    /// Allocates a page of physical memory
    /// Ignores `layout`, except to check that the request is for no more than a page of memory
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // Cannot allocate or align more than a page
        if size > c_bindings::PGSIZE as usize || align > c_bindings::PGSIZE as usize {
            return ptr::null_mut();
        }

        let freelist = self.freelist.lock();
        let return_cell = Cell::new(None);
        return_cell.swap(&freelist);
        if let Some(run_cell) = return_cell.get() {
            freelist.swap(&run_cell.as_ref().next);
        }

        match return_cell.get().map(NonNull::cast::<u8>) {
            None => null_mut(),
            Some(ptr) => {
                let final_ptr = ptr.as_ptr();
                {
                    let page_refcounts = self.page_refcounts.lock();
                    let mut refcount_data = page_refcounts.borrow_mut();
                    // The index in the refcount data to update.
                    let page_index = self.convert_physical_to_index(final_ptr as usize);
                    refcount_data[page_index] += 1;
                }
                Spintex::unlock(freelist);
                ptr::write_bytes(final_ptr, 5, c_bindings::PGSIZE as usize);
                final_ptr
            }
        }
    }

    /// Deallocate a page allocated by this allocator
    #[allow(clippy::cast_ptr_alignment)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        let align = layout.align();
        let ptr_int = ptr as usize;
        if ptr_int % c_bindings::PGSIZE as usize != 0
            || ptr_int < *self.end.get().unwrap()
            || ptr_int >= usize::try_from(PHYSICAL_ADDRESS_STOP).unwrap()
            || size > c_bindings::PGSIZE as usize
            || align > c_bindings::PGSIZE as usize
        {
            panic!("KPA_dealloc: Out of bounds\0");
        }

        // Lock any modifications to the freelist for the remainder of the execution
        // We want to make sure that we don't deadlock, and that we don't change the refcount before deallocating
        // We also want to have the same lock order as alloc
        let freelist = self.freelist.lock();

        let page_refcounts = self.page_refcounts.lock();
        let refcount = {
            let mut refcount_data = page_refcounts.borrow_mut();
            // The index in the refcount data to update. Previous checks ensure this is in bounds
            let page_index = self.convert_physical_to_index(ptr_int);
            // Panic if no references were loaned out to the Kernel
            let mut refcount = refcount_data[page_index];
            if refcount == 0 {
                panic!("KPA_dealloc: No page references\0");
            }
            // Remove a reference to this page
            refcount -= 1;
            refcount_data[page_index] = refcount;
            refcount
        };
        Spintex::unlock(page_refcounts);

        // Only actually deallocate if we have 0 references
        if refcount == 0 {
            ptr::write_bytes(ptr, 1, c_bindings::PGSIZE as usize);
            let run_ref_option: Option<&'static mut Run> = ptr.cast::<Run>().as_mut();
            if let Some(run_ref) = run_ref_option {
                run_ref.next = Cell::new(None);
                run_ref.next.swap(&freelist);
                let run_cell = Cell::new(Some(NonNull::new_unchecked(run_ref as *mut Run)));
                freelist.swap(&run_cell);
            }
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // Cannot allocate or align more than a page, otherwise, this realloc is valid in place
        if size > c_bindings::PGSIZE as usize
            || align > c_bindings::PGSIZE as usize
            || new_size > c_bindings::PGSIZE as usize
        {
            return ptr::null_mut();
        }

        ptr
    }
}

impl KernelPageAllocator<'_> {
    pub fn init(&self, end: usize) {
        self.end.set(end).unwrap();
    }

    pub(crate) fn pfree_count(&self) -> u64 {
        let mut free_memory = 0u64;
        let freelist = self.freelist.lock();
        let mut optional_run_ref = freelist.get();
        while optional_run_ref.is_some() {
            free_memory += u64::from(c_bindings::PGSIZE);
            optional_run_ref = optional_run_ref.and_then(|ptr| unsafe { ptr.as_ref() }.next.get());
        }
        free_memory
    }

    #[inline]
    fn convert_physical_to_index(&self, physical_address: usize) -> usize {
        usize::try_from(PGROUNDDOWN!(
            physical_address - usize::try_from(PGROUNDUP!(*self.end.get().unwrap())).unwrap()
        ))
        .unwrap()
            / c_bindings::PGSIZE as usize
    }

    pub fn in_place_copy(&self, physical_address: usize) {
        let index = self.convert_physical_to_index(physical_address);
        let refcounts = self.page_refcounts.lock();
        refcounts.borrow_mut()[index] += 1;
    }

    pub(crate) fn exactly_one_reference(&self, physical_address: usize) -> bool {
        let index = self.convert_physical_to_index(physical_address);
        let reference_counts = self.page_refcounts.lock();
        let reference_data = reference_counts.borrow();
        reference_data[index] == 1
    }
}

/// C Entry point for Kernel Page Alloc
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn kalloc() -> *mut core::ffi::c_void {
    let layout =
        Layout::from_size_align_unchecked(c_bindings::PGSIZE as usize, c_bindings::PGSIZE as usize);
    alloc::alloc::alloc(layout).cast()
}

/// C Entry point for Kernel Page Frees
/// # Safety
/// `ptr` should be allocated from `kalloc` and should be page-aligned
#[no_mangle]
pub unsafe extern "C" fn kfree(ptr: *mut core::ffi::c_void) {
    let layout =
        Layout::from_size_align_unchecked(c_bindings::PGSIZE as usize, c_bindings::PGSIZE as usize);
    alloc::alloc::dealloc(ptr.cast(), layout);
}

/// Initialize constants needed in rust from C
/// For linking reasons, `end` is for some reason not set correctly by rustc (may need to switch to an executable for that)
#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn kinit_rust(end: c_bindings::uint64) {
    ALLOCATOR.init(usize::try_from(end).unwrap());
}
