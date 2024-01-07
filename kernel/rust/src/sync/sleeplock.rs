use core::{
    cell::{Cell, UnsafeCell},
    ptr::NonNull,
};

use crate::{c_bindings, proc::sleep_rust};

use super::spinlock::Spintex;

#[derive(Debug, Default)]
pub(crate) struct Sleeplock<'a> {
    locked: Spintex<'a, bool>,
    name: &'a str,
    pid: Cell<Option<usize>>,
}

impl<'a> Sleeplock<'a> {
    pub(crate) const fn new(name: &'a str) -> Self {
        Self {
            locked: Spintex::new(false, "sleep lock"),
            name,
            pid: Cell::new(None),
        }
    }

    pub(crate) fn acquire(&self) {
        let mut locked = self.locked.lock();
        while *locked {
            sleep_rust(NonNull::from(self), locked);
            locked = self.locked.lock();
        }

        *locked = true;
        self.pid.set(unsafe {
            c_bindings::myproc()
                .as_ref()
                .and_then(|proc| usize::try_from(proc.pid).ok())
        });
    }

    pub(crate) fn release(&self) {
        let mut locked = self.locked.lock();
        *locked = false;
        self.pid.set(None);
    }

    pub(crate) fn holding(&self) -> bool {
        let locked = self.locked.lock();
        *locked
            && self.pid.get()
                == unsafe {
                    c_bindings::myproc()
                        .as_ref()
                        .and_then(|proc| usize::try_from(proc.pid).ok())
                }
    }
}

#[derive(Default)]
pub(crate) struct Sleeptex<'a, T: 'a> {
    lock: Sleeplock<'a>,
    data: UnsafeCell<T>,
}

pub(crate) struct SleeptexGuard<'a, 'b: 'a, T: 'b> {
    lock: &'a Sleeptex<'b, T>,
}

impl<'a, T: 'a> Sleeptex<'a, T> {
    pub const fn new(value: T, name: &'a str) -> Self {
        Self {
            lock: Sleeplock::new(name),
            data: UnsafeCell::new(value),
        }
    }

    pub fn lock(&'a self) -> SleeptexGuard<'_, 'a, T> {
        self.lock.acquire();
        SleeptexGuard::new(self)
    }

    pub fn unlock(guard: SleeptexGuard<'_, 'a, T>) {
        drop(guard);
    }

    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
}

impl<T> From<T> for Sleeptex<'_, T> {
    fn from(value: T) -> Self {
        Sleeptex::new(value, Default::default())
    }
}

impl<'a, T> From<(T, &'a str)> for Sleeptex<'a, T> {
    fn from(value: (T, &'a str)) -> Self {
        Sleeptex::new(value.0, value.1)
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for Sleeptex<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_struct("Sleeptex");
        let guard = self.lock();
        d.field("lock_name", &self.lock.name);
        d.field("data", &&*guard);
        d.finish_non_exhaustive()
    }
}

unsafe impl<T: Send> Send for Sleeptex<'_, T> {}
unsafe impl<T: Send> Sync for Sleeptex<'_, T> {}

impl<'a, 'b: 'a, T: 'b> SleeptexGuard<'a, 'b, T> {
    pub fn new(lock: &'b Sleeptex<T>) -> Self {
        Self { lock }
    }
}

impl<T> core::ops::Deref for SleeptexGuard<'_, '_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> core::ops::DerefMut for SleeptexGuard<'_, '_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SleeptexGuard<'_, '_, T> {
    #[inline]
    fn drop(&mut self) {
        self.lock.lock.release();
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for SleeptexGuard<'_, '_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}

impl<T: core::fmt::Display> core::fmt::Display for SleeptexGuard<'_, '_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        (**self).fmt(f)
    }
}

// SleeptexGuard is !Send (not implementable here)
unsafe impl<T: Sync> Sync for SleeptexGuard<'_, '_, T> {}
