use crate::c_bindings;
use crate::device_load::PHYSICAL_ADDRESS_STOP;
use crate::printf::panic;
use crate::sync::spinlock::{Spintex, SpintexGuard};
use crate::vm::{PGROUNDDOWN, PGROUNDUP};
use alloc::alloc::{GlobalAlloc, Layout};
use core::cell::{Cell, OnceCell};
use core::ptr::{self, null_mut, NonNull};

const MEM_LOCK_NAME: &str = "kmem";
const TINY_MEM_LOCK_NAME: &str = "kmem_tiny";
const REFCOUNTS_LOCK_NAME: &str = "page_refcounts";

#[repr(C)]
struct Run {
    pub next: Cell<Option<NonNull<Run>>>,
}

#[repr(C, align(16))]
struct TinyHeader {
    next: Cell<Option<NonNull<TinyHeader>>>,
    size: usize,
}

pub(crate) struct KernelPageAllocator<'a> {
    freelist: Spintex<'a, Cell<Option<NonNull<Run>>>>,
    page_refcounts: Spintex<'a, Cell<Option<&'a mut [u8]>>>,
    end: OnceCell<usize>,
}

pub(crate) struct KernelAllocator<'a> {
    page_allocator: KernelPageAllocator<'a>,
    tiny_page_list: Spintex<'a, Cell<Option<NonNull<TinyHeader>>>>,
}

#[global_allocator]
pub(crate) static ALLOCATOR: KernelAllocator = KernelAllocator {
    page_allocator: KernelPageAllocator {
        freelist: Spintex::new(Cell::new(None), MEM_LOCK_NAME),
        page_refcounts: Spintex::new(Cell::new(None), REFCOUNTS_LOCK_NAME),
        end: OnceCell::new(),
    },
    tiny_page_list: Spintex::new(Cell::new(None), TINY_MEM_LOCK_NAME),
};

unsafe impl<'a> Sync for KernelPageAllocator<'a> {}
unsafe impl<'a> Send for KernelPageAllocator<'a> {}
unsafe impl<'a> Sync for KernelAllocator<'a> {}
unsafe impl<'a> Send for KernelAllocator<'a> {}

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
                    let refcount_data = page_refcounts.take().unwrap();
                    // The index in the refcount data to update.
                    let page_index = self.convert_physical_to_index(final_ptr as usize);
                    refcount_data[page_index] += 1;
                    page_refcounts.set(Some(refcount_data));
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
            let refcount_data = page_refcounts.take().unwrap();
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
            page_refcounts.set(Some(refcount_data));
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
}

impl KernelPageAllocator<'_> {
    pub fn init(&self, end: usize, page_count: usize) {
        self.end.set(end).unwrap();
        unsafe {
            core::ptr::write_bytes(end as *mut u8, 1, page_count);
        }
        let refcount_cell = self.page_refcounts.lock();
        refcount_cell.set(Some(unsafe {
            core::slice::from_raw_parts_mut(end as *mut u8, page_count)
        }));
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
        let refcount_data = refcounts.take().unwrap();
        refcount_data[index] += 1;
        refcounts.set(Some(refcount_data));
    }

    pub(crate) fn exactly_one_reference(&self, physical_address: usize) -> bool {
        let index = self.convert_physical_to_index(physical_address);
        let reference_counts = self.page_refcounts.lock();
        let reference_data = reference_counts.take().unwrap();
        let is_exactly_one_reference = reference_data[index] == 1;
        reference_counts.set(Some(reference_data));
        is_exactly_one_reference
    }
}

unsafe impl GlobalAlloc for KernelAllocator<'_> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // Pass off allocations greater or equal to a page to the page allocator
        // Size will delegate to the page allocator if it is bigger than
        if size >= (c_bindings::PGSIZE as usize - (2 * core::mem::size_of::<TinyHeader>()))
            || align >= c_bindings::PGSIZE as usize
        {
            self.page_allocator.alloc(layout)
        } else {
            if align > Self::MAX_ALIGNMENT {
                return ptr::null_mut();
            }
            let size = (size + Self::MAX_ALIGNMENT - 1) & !(Self::MAX_ALIGNMENT - 1);
            let tiny_list = self.tiny_page_list.lock();
            if let Some(list) = tiny_list.get() {
                let mut header = list;
                let mut prev: Option<NonNull<TinyHeader>> = None;
                let data = loop {
                    let header_mut = header.as_mut();
                    let header_size = header_mut.size;
                    if header_size >= size {
                        break self.write_blocks(&mut prev, &tiny_list, header_mut, size);
                    }
                    /*
                    if header_size > size {
                        if let Some(mut prev) = prev {
                            prev.as_mut().next = Cell::new(header_mut.next.get());
                        } else {
                            tiny_list.set(header_mut.next.get());
                        }
                        break header.as_ptr().add(1).cast::<u8>();
                    }*/

                    if header_mut.next.get().is_none() {
                        break ptr::null_mut();
                    }

                    prev = Some(header);
                    header = header_mut.next.get().unwrap();
                };
                if data.is_null() {
                    let page_layout = Layout::from_size_align_unchecked(
                        c_bindings::PGSIZE as usize,
                        c_bindings::PGSIZE as usize,
                    );
                    let new_page = self.page_allocator.alloc(page_layout);
                    if new_page.is_null() {
                        new_page
                    } else {
                        let free_header = new_page
                            .add(size + core::mem::size_of::<TinyHeader>())
                            .cast::<TinyHeader>();
                        unsafe {
                            *free_header = TinyHeader {
                                next: Cell::new(tiny_list.get()),
                                size: c_bindings::PGSIZE as usize
                                    - (size + 2 * core::mem::size_of::<TinyHeader>()),
                            }
                        };
                        unsafe {
                            *new_page.cast::<TinyHeader>() = TinyHeader {
                                next: Cell::new(None),
                                size,
                            }
                        }
                        tiny_list.set(NonNull::new(free_header));
                        new_page.cast::<TinyHeader>().add(1).cast()
                    }
                } else {
                    data
                }
            } else {
                let page_layout = Layout::from_size_align_unchecked(
                    c_bindings::PGSIZE as usize,
                    c_bindings::PGSIZE as usize,
                );
                let new_page = self.page_allocator.alloc(page_layout);
                if new_page.is_null() {
                    new_page
                } else {
                    let free_header = new_page
                        .add(size + core::mem::size_of::<TinyHeader>())
                        .cast::<TinyHeader>();
                    unsafe {
                        *free_header = TinyHeader {
                            next: Cell::new(None),
                            size: c_bindings::PGSIZE as usize
                                - (size + 2 * core::mem::size_of::<TinyHeader>()),
                        }
                    };
                    unsafe {
                        *new_page.cast::<TinyHeader>() = TinyHeader {
                            next: Cell::new(None),
                            size,
                        }
                    }
                    tiny_list.set(NonNull::new(free_header));
                    new_page.cast::<TinyHeader>().offset(1).cast()
                }
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        let align = layout.align();

        // Pass off deallocations greater or equal to a page to the page allocator
        // Size will delegate to the page allocator if it is bigger than
        if size >= (c_bindings::PGSIZE as usize - (2 * core::mem::size_of::<TinyHeader>()))
            || align >= c_bindings::PGSIZE as usize
        {
            self.page_allocator.dealloc(ptr, layout);
        } else {
            let ptr_int = ptr as usize;
            if ptr_int % Self::MAX_ALIGNMENT != 0
                || ptr_int < *self.page_allocator.end.get().unwrap()
                || ptr_int >= usize::try_from(PHYSICAL_ADDRESS_STOP).unwrap()
                || size > c_bindings::PGSIZE as usize
                || align > c_bindings::PGSIZE as usize
            {
                panic!("KTA_dealloc: Out of bounds\0");
            }

            let header_list = self.tiny_page_list.lock();
            let header = unsafe { ptr.cast::<TinyHeader>().offset(-1).as_mut().unwrap() };
            header.next = Cell::new(header_list.get());
            header_list.set(Some(header.into()));
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old_size = layout.size();
        let align = layout.align();
        if align >= c_bindings::PGSIZE as usize {
            self.page_allocator.realloc(ptr, layout, new_size)
        } else if old_size >= c_bindings::PGSIZE as usize - (2 * core::mem::size_of::<TinyHeader>())
        {
            if new_size <= old_size {
                ptr
            } else {
                self.page_allocator.realloc(ptr, layout, new_size)
            }
        } else {
            match unsafe { ptr.cast::<TinyHeader>().sub(1).as_ref() } {
                Some(header) => {
                    if header.size >= new_size {
                        ptr
                    } else {
                        self.default_realloc(ptr, layout, new_size)
                    }
                }
                None => self.default_realloc(ptr, layout, new_size),
            }
        }
    }
}

impl KernelAllocator<'_> {
    const MAX_ALIGNMENT: usize = 16;

    pub fn init(&self, end: usize, page_count: usize) {
        self.page_allocator.init(end, page_count);
    }

    pub(crate) fn memfree_count(&self) -> u64 {
        let tiny_space = {
            let tiny_allocations = self.tiny_page_list.lock();
            let mut list_ptr = tiny_allocations.get();
            let mut tiny_space = 0usize;
            while list_ptr.is_some() {
                if let Some(ptr) = list_ptr {
                    let data_ref = unsafe { ptr.as_ptr().as_ref() }.unwrap();
                    tiny_space += data_ref.size;
                    list_ptr = data_ref.next.get();
                }
            }
            u64::try_from(tiny_space).unwrap()
        };
        tiny_space + self.page_allocator.pfree_count()
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn in_place_copy(&self, physical_address: usize) {
        if PGROUNDDOWN!(physical_address) as usize == physical_address {
            self.page_allocator.in_place_copy(physical_address);
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn exactly_one_reference(&self, physical_address: usize) -> bool {
        PGROUNDDOWN!(physical_address) as usize == physical_address
            && self.page_allocator.exactly_one_reference(physical_address)
    }

    fn write_blocks(
        &self,
        prev: &mut Option<NonNull<TinyHeader>>,
        tiny_list: &SpintexGuard<'_, '_, Cell<Option<NonNull<TinyHeader>>>>,
        header: &mut TinyHeader,
        size: usize,
    ) -> *mut u8 {
        if header.size > size {
            header.size -= size + core::mem::size_of::<TinyHeader>();
            let new_header = unsafe { ptr::addr_of_mut!(*header).add(1).byte_add(header.size) };
            unsafe {
                *new_header = TinyHeader {
                    next: Cell::new(None),
                    size,
                };
            }
            unsafe { new_header.add(1).cast() }
        } else {
            if let Some(mut prev) = prev {
                unsafe { prev.as_mut() }.next = Cell::new(header.next.get());
            } else {
                tiny_list.set(header.next.get());
            }
            unsafe { ptr::addr_of_mut!(*header).add(1).cast::<u8>() }
        }
    }

    fn default_realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        // SAFETY: the caller must ensure that `new_layout` is greater than zero.
        let new_ptr = unsafe { self.alloc(new_layout) };
        if !new_ptr.is_null() {
            // SAFETY: the previously allocated block cannot overlap the newly allocated block.
            // The safety contract for `dealloc` must be upheld by the caller.
            unsafe {
                ptr::copy_nonoverlapping(ptr, new_ptr, core::cmp::min(layout.size(), new_size));
                self.dealloc(ptr, layout);
            }
        }
        new_ptr
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
pub extern "C" fn kinit_rust(end: c_bindings::uint64) -> c_bindings::uint64 {
    let page_count = (PGROUNDDOWN!(unsafe { PHYSICAL_ADDRESS_STOP }) - PGROUNDUP!(end))
        / u64::from(c_bindings::PGSIZE);
    let end = usize::try_from(end).unwrap();
    ALLOCATOR.init(end, usize::try_from(page_count).unwrap());
    page_count
}
