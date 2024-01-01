use crate::{
    c_bindings,
    interrupts::{pop_off, push_off},
};
use core::{
    cell::{Cell, UnsafeCell},
    ptr::NonNull,
    sync::atomic::{fence, AtomicBool},
};

/// A rv6 Spinlock, only accessible from Rust
#[derive(Debug, Default)]
pub(crate) struct Spinlock<'a> {
    locked: AtomicBool,
    name: &'a str,
    cpu: Cell<Option<NonNull<c_bindings::cpu>>>,
}

impl<'a> Spinlock<'a> {
    /// Creates a new Spinlock with the given name
    pub(crate) const fn new(name: &'a str) -> Self {
        Self {
            locked: AtomicBool::new(false),
            name,
            cpu: Cell::new(None),
        }
    }

    /// Acquires the lock, blocking the thread until the lock is acquired, in a busy spin lock
    /// Disables interrupts, panics if this CPU is already holding the Spinlock
    pub(crate) fn acquire(&self) {
        push_off();
        if self.holding() {
            unsafe {
                c_bindings::panic(b"acquire_rust\0".as_ptr().cast::<i8>().cast_mut());
            }
        }

        while self
            .locked
            .compare_exchange(
                false,
                true,
                core::sync::atomic::Ordering::Relaxed,
                core::sync::atomic::Ordering::Relaxed,
            )
            .is_err()
        {
            core::hint::spin_loop();
        }

        fence(core::sync::atomic::Ordering::SeqCst);

        self.cpu
            .set(unsafe { c_bindings::mycpu().as_mut() }.map(NonNull::from));
    }

    /// Releases a lock held by this CPU
    /// Enables interrupts if outer interrupt disable, panics if tihs CPU is not holding the Spinlock
    pub(crate) fn release(&self) {
        if !self.holding() {
            unsafe {
                c_bindings::panic(b"release_rust\0".as_ptr().cast::<i8>().cast_mut());
            }
        }

        self.cpu.set(None);

        fence(core::sync::atomic::Ordering::SeqCst);

        self.locked
            .store(false, core::sync::atomic::Ordering::Release);

        pop_off();
    }

    /// Determines if this CPU is holding the Spinlock
    pub(crate) fn holding(&self) -> bool {
        // We have to be running on a CPU, so if the lock is locked, we can guarantee that the CPU is set
        self.locked.load(core::sync::atomic::Ordering::Acquire)
            && self
                .cpu
                .get()
                .is_some_and(|ptr| ptr.as_ptr() == unsafe { c_bindings::mycpu() })
    }
}

/// A RAII, [`Spinlock`]-based Mutex
/// Modeled after `std::sync::Mutex`
/// Since rv6 aborts on panic, no need to store poisoning
#[derive(Default)]
pub(crate) struct Spintex<'a, T: 'a> {
    lock: Spinlock<'a>,
    data: UnsafeCell<T>,
}

/// A RAII [`Spintex Guard`], releases the Spin lock on Drop
/// Modeled after `std::sync::MutexGuard`
pub(crate) struct SpintexGuard<'a, 'b: 'a, T: 'b> {
    lock: &'a Spintex<'b, T>,
}

impl<'a, T: 'a> Spintex<'a, T> {
    /// Creates a new Spintex holding the given value, with the given [`Spinlock`] name
    pub const fn new(value: T, name: &'a str) -> Self {
        Self {
            lock: Spinlock::new(name),
            data: UnsafeCell::new(value),
        }
    }

    /// Acquire the underlying [`Spinlock`], and return an exclusive view of the data
    pub fn lock(&'a self) -> SpintexGuard<'_, 'a, T> {
        self.lock.acquire();
        SpintexGuard::new(self)
    }

    /// Unlock the spintex without removing Guards
    /// # Safety
    /// Should only be used when process sleeping
    pub(crate) unsafe fn unlock_unsafe(&'a self) {
        self.lock.release();
    }

    /// Manually unlock a held [`SpintexGuard`]
    /// Explicit version of dropping the guard
    pub fn unlock(guard: SpintexGuard<'_, 'a, T>) {
        drop(guard);
    }

    /// Consumes the owned [`Spintex`] and returns the held value
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }

    /// Returns a mutable reference to the underlying data
    /// Statically checks the lock doesn't exist, so no lock
    /// is needed to be taken
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
}

impl<T> From<T> for Spintex<'_, T> {
    fn from(value: T) -> Self {
        Spintex::new(value, Default::default())
    }
}

impl<'a, T> From<(T, &'a str)> for Spintex<'a, T> {
    fn from(value: (T, &'a str)) -> Self {
        Spintex::new(value.0, value.1)
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for Spintex<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_struct("Spintex");
        let guard = self.lock();
        d.field("lock_name", &self.lock.name);
        d.field("data", &&*guard);
        d.finish_non_exhaustive()
    }
}

unsafe impl<T: Send> Send for Spintex<'_, T> {}
unsafe impl<T: Send> Sync for Spintex<'_, T> {}

impl<'a, 'b: 'a, T: 'b> SpintexGuard<'a, 'b, T> {
    pub fn new(lock: &'b Spintex<T>) -> Self {
        Self { lock }
    }
}

impl<T> core::ops::Deref for SpintexGuard<'_, '_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> core::ops::DerefMut for SpintexGuard<'_, '_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpintexGuard<'_, '_, T> {
    #[inline]
    fn drop(&mut self) {
        self.lock.lock.release();
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for SpintexGuard<'_, '_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}

impl<T: core::fmt::Display> core::fmt::Display for SpintexGuard<'_, '_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        (**self).fmt(f)
    }
}

// SpintexGuard is !Send (not implementable here)
unsafe impl<T: Sync> Sync for SpintexGuard<'_, '_, T> {}
