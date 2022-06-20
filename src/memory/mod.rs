use std::{ptr::{NonNull}, ops::{Deref, DerefMut}};

mod pointers;
mod counter;

use pointers::*;
use counter::*;


pub struct Strong<T: 'static>(TransRef<T>);

impl<T: 'static> Strong<T> {
    pub fn new(it: T) -> Self {
        Self(
            LocalRaw {
                genref: COUNTER_INIT,
                genptr: LocalGeneration::new(),
                boxptr: unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(it))) }
            }.into()
        )
    }

    pub fn alias(&self) -> Weak<T> {
        Weak(self.0 .0.get())
    }

    pub fn take(self) -> Result<Box<T>, Self> {
        let gen = self.0.generation();
        if gen.try_lock_exclusive() {
            gen.bump();
            let res = unsafe { Box::from_raw(self.0.pointer().as_ptr()) };
            unsafe { gen.unlock_exclusive(); } 
            LocalOrGlobalGeneration::free(gen);
            std::mem::forget(self);
            Ok(res)
        } else {
            Err(self)
        }
    }

    fn try_read(&self) -> Option<Reading<T>> {
        if self.0.generation().try_lock_shared() {
            Some(Reading(self.0 .0.get()))
        } else {
            None
        }
    }

    fn try_write(&self) -> Option<Reading<T>> {
        if self.0.generation().try_lock_exclusive() {
            Some(Reading(self.0 .0.get()))
        } else {
            None
        }
    }
}

impl<T:Send + Sync + 'static> Strong<T> {
    pub fn send(self) -> Sending<T> {
        let res = match self.0 .0.get() {
            RawRef::Local(l) => Sending(l.globalize()),
            RawRef::Global(g) => Sending(g),
        };
        std::mem::forget(self);
        res
    }
}

impl<T:'static> Drop for Strong<T> {
    fn drop(&mut self) {
        let gen = self.0.generation();
        gen.bump();
        if gen.try_lock_exclusive() {
            std::mem::drop(unsafe { Box::from_raw(self.0.pointer().as_ptr()) });
            unsafe { gen.unlock_exclusive() }
            LocalOrGlobalGeneration::free(gen);
        }
    }
}

impl<T> From<Sending<T>> for Strong<T> {
    fn from(it: Sending<T>) -> Self {
        Strong(it.0.into())
    }
}
 
pub struct Sending<T: 'static>(GlobalRaw<T>);
unsafe impl<T: 'static + Send + Sync> Send for Sending<T> {}

#[repr(transparent)]
pub struct Sharing<T: 'static>(GlobalRaw<T>);
unsafe impl<T: 'static + Sync> Send for Sharing<T> {}

pub struct Weak<T: 'static>(RawRef<T>);
impl<T:'static> Copy for Weak<T> {}
impl<T:'static> Clone for Weak<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T:'static + Sync> Weak<T> {
    pub fn share(self) -> Sharing<T> {
        Sharing(match self.0 {
            RawRef::Local(l) => l.globalize(),
            RawRef::Global(g) => g,
        })
    }
}

impl<T> From<Sharing<T>> for Weak<T> {
    fn from(it: Sharing<T>) -> Self {
        Weak(it.0.into())
    }
}

impl<T> Weak<T> {
    fn try_read(&self) -> Option<Reading<T>> {
        let gen = self.0.generation();
        if self.0.validity() == gen.count() {
            if self.0.generation().try_lock_shared() {
                return Some(Reading(self.0))
            }
        }
        None
    }

    fn try_write(&self) -> Option<Reading<T>> {
        let gen = self.0.generation();
        if self.0.validity() == gen.count() {
            if self.0.generation().try_lock_exclusive() {
                return Some(Reading(self.0))
            }
        }
        None
    }
}

pub struct Reading<T:'static>(RawRef<T>);

impl<T:'static> Deref for Reading<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.pointer().as_ref() }
    }
}

impl<T:'static> Clone for Reading<T> {
    fn clone(&self) -> Self {
        if !self.0.generation().try_lock_shared() { panic!() }
        Self(self.0)
    }
}

impl<T:'static> Drop for Reading<T> {
    fn drop(&mut self) {
        let gen = self.0.generation();
        if self.0.validity() != gen.count() {
            if unsafe { gen.try_shared_into_exclusive() } {
                std::mem::drop(unsafe { Box::from_raw( self.0.pointer().as_ptr()) });
                unsafe { gen.unlock_exclusive() }
                LocalOrGlobalGeneration::free(gen);
                return;
            }
        }
        unsafe { gen.unlock_shared() }
    }
}

pub struct Writing<T:'static>(RawRef<T>);

impl<T:'static> Deref for Writing<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.pointer().as_ref() }
    }
}

impl<T:'static> DerefMut for Writing<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.pointer().as_mut() }
    }
}

impl<T:'static> Drop for Writing<T> {
    fn drop(&mut self) {
        let gen = self.0.generation();
        if self.0.validity() != gen.count() {
            std::mem::drop(unsafe { Box::from_raw( self.0.pointer().as_ptr()) });
            unsafe { gen.unlock_exclusive() }
            LocalOrGlobalGeneration::free(gen);
        } else {
            unsafe { gen.unlock_exclusive() }
        }
    }
}