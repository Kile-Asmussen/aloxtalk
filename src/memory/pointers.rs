use std::{cell::Cell, ptr::NonNull};

use super::counter::*;

macro_rules! clone_copy {
    ($t:ident) => {
        impl<T: 'static> Copy for $t<T> {}
        impl<T: 'static> Clone for $t<T> {
            fn clone(&self) -> Self {
                *self
            }
        }
    };
}

pub(crate) struct LocalRaw<T: 'static> {
    pub(crate) genref: u32,
    pub(crate) genptr: LocalGeneration,
    pub(crate) boxptr: NonNull<T>,
}
clone_copy!(LocalRaw);

impl<T: 'static> LocalRaw<T> {
    pub(crate) fn globalize(&self) -> GlobalRaw<T> {
        let LocalRaw {
            genref,
            genptr,
            boxptr,
        } = *self;
        let genptr = genptr.globalize();
        GlobalRaw {
            genref,
            genptr,
            boxptr,
        }
    }
}

pub(crate) struct GlobalRaw<T: 'static> {
    pub(crate) genref: u32,
    pub(crate) genptr: GlobalGeneration,
    pub(crate) boxptr: NonNull<T>,
}
clone_copy!(GlobalRaw);

pub(crate) enum RawRef<T: 'static> {
    Local(LocalRaw<T>),
    Global(GlobalRaw<T>),
}
clone_copy!(RawRef);

impl<T: 'static> From<LocalRaw<T>> for RawRef<T> {
    fn from(it: LocalRaw<T>) -> Self {
        Self::Local(it)
    }
}
impl<T: 'static> From<GlobalRaw<T>> for RawRef<T> {
    fn from(it: GlobalRaw<T>) -> Self {
        Self::Global(it)
    }
}

pub(crate) struct TransRef<T: 'static>(pub(crate) Cell<RawRef<T>>);
impl<T: 'static> Clone for TransRef<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: 'static> TransRef<T> {
    fn new(it: RawRef<T>) -> Self {
        Self(Cell::new(it))
    }
}
impl<T: 'static> From<LocalRaw<T>> for TransRef<T> {
    fn from(it: LocalRaw<T>) -> Self {
        Self::new(it.into())
    }
}
impl<T: 'static> From<GlobalRaw<T>> for TransRef<T> {
    fn from(it: GlobalRaw<T>) -> Self {
        Self::new(it.into())
    }
}

pub(crate) trait Reference<T: 'static> {
    type Gen: Generation + GenerationCounter + AccessControl;
    fn pointer(&self) -> NonNull<T>;
    fn validity(&self) -> u32;
    fn generation(&self) -> Self::Gen;
}

impl<T: 'static> Reference<T> for LocalRaw<T> {
    type Gen = LocalGeneration;

    #[inline(always)]
    fn pointer(&self) -> NonNull<T> {
        self.boxptr
    }
    #[inline(always)]
    fn validity(&self) -> u32 {
        self.genref
    }
    #[inline(always)]
    fn generation(&self) -> Self::Gen {
        self.genptr
    }
}

impl<T: 'static> Reference<T> for GlobalRaw<T> {
    type Gen = GlobalGeneration;
    #[inline(always)]
    fn pointer(&self) -> NonNull<T> {
        self.boxptr
    }
    #[inline(always)]
    fn validity(&self) -> u32 {
        self.genref
    }
    #[inline(always)]
    fn generation(&self) -> Self::Gen {
        self.genptr
    }
}

impl<T: 'static> Reference<T> for RawRef<T> {
    type Gen = LocalOrGlobalGeneration;

    #[inline(always)]
    fn pointer(&self) -> NonNull<T> {
        match self {
            RawRef::Local(l) => l.pointer(),
            RawRef::Global(g) => g.pointer(),
        }
    }

    #[inline(always)]
    fn validity(&self) -> u32 {
        match self {
            RawRef::Local(l) => l.validity(),
            RawRef::Global(g) => g.validity(),
        }
    }

    #[inline(always)]
    fn generation(&self) -> Self::Gen {
        match self {
            RawRef::Local(l) => Self::Gen::Local(l.generation()),
            RawRef::Global(g) => Self::Gen::Global(g.generation()),
        }
    }
}

impl<T: 'static> Reference<T> for TransRef<T> {
    type Gen = LocalOrGlobalGeneration;
    #[inline(always)]
    fn pointer(&self) -> NonNull<T> {
        self.0.get().pointer()
    }
    #[inline(always)]
    fn validity(&self) -> u32 {
        self.0.get().validity()
    }
    #[inline(always)]
    fn generation(&self) -> Self::Gen {
        self.0.get().generation()
    }
}
