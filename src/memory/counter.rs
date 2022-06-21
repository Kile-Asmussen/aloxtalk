use std::{cell::{Cell, RefCell}, sync::atomic::{AtomicU32, Ordering::*, AtomicUsize}, ptr::NonNull};

use lock_api::{RawRwLock, RawRwLockUpgrade, RawRwLockDowngrade, RawRwLockRecursive, RawRwLockUpgradeDowngrade};
use parking_lot::Mutex;

pub(crate) trait Generation : Sized {
    fn free(this: Self);
}

#[derive(Clone, Copy)]
pub(crate) struct LocalGeneration(pub(crate) NonNull<LocalCounter>);

impl LocalGeneration {
    pub(crate) fn new() -> Self {
        Self::re_use().unwrap_or_else(Self::fresh)
    }

    pub(crate) fn globalize(&self) -> GlobalGeneration {
        unsafe{ self.0.as_ref() }.globalize()
    }

    fn fresh() -> Self {
        let mut fresh = Self::FRESH_LIST.with(|c| c.replace(vec![].into_boxed_slice())); 
        let mut next = Self::NEXT_FRESH.with(Cell::get);
        
        if next == fresh.len() {
            Self::LEAKED_COUNTERS.with_borrow_mut(|v| v.push(fresh));
            fresh = (0.. next + next/2).map(|_| LocalCounter::new()).collect();
            next = 0;
        }

        let res = Self(NonNull::from(&fresh[next]));
        next += 1;
        Self::FRESH_LIST.with(|c| c.set(fresh));
        Self::NEXT_FRESH.with(|c| c.set(next));

        res
    }

    fn re_use() -> Option<Self> {
        Self::FREE_LIST.with_borrow_mut(Vec::pop)
    }

    thread_local! {
        static FREE_LIST : RefCell<Vec<LocalGeneration>> = RefCell::new(Vec::new());
        static NEXT_FRESH : Cell<usize> = Cell::new(0);
        static FRESH_LIST : Cell<Box<[LocalCounter]>> = Cell::new((0..32).map(|_| LocalCounter::new()).collect());
        static LEAKED_COUNTERS : RefCell<Vec<Box<[LocalCounter]>>> = RefCell::new(Vec::new());
    }
    
    #[inline(always)]
    fn delegate<R>(&self, f: fn(&LocalCounter) -> R) -> R {
        f(unsafe { self.0.as_ref() })
    }

    #[inline(always)]
    unsafe fn unsafe_delegate<R>(&self, f: unsafe fn(&LocalCounter) -> R) -> R {
        f(self.0.as_ref())
    }
}

impl Generation for LocalGeneration {
    fn free(this: Self) {
        if this.count() != 0 {
            Self::FREE_LIST.with_borrow_mut(|v| v.push(this))
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct GlobalGeneration(pub(crate) &'static GlobalCounter);

impl GlobalGeneration {
    pub(crate) fn new() -> Self {
        Self::re_use().unwrap_or_else(Self::fresh)
    }

    fn fresh() -> Self {
        let mut fresh = FRESH_LIST.lock();

        if fresh.1.len() == 0 {

            fresh.1 = (0.. fresh.0).map(|_| GlobalCounter::new()).collect::<Vec<GlobalCounter>>().leak();

            fresh.0 += fresh.0 / 2;
        }

        let res = Self(&fresh.1[0]);
        fresh.1 = &fresh.1[1..];
        res
    }

    fn re_use() -> Option<Self> {
        FREE_LIST.lock().pop()
    }

    fn from_local(rlc: RawLocalCounter) -> Self {
        let this = Self::fresh();

        this.0.set_gen(rlc.count());

        rlc.access_state().inflict(&this.0.access);

        this
    }

    #[inline(always)]
    fn delegate<R>(&self, f: fn(&GlobalCounter) -> R) -> R {
        f(self.0)
    }

    #[inline(always)]
    unsafe fn unsafe_delegate<R>(&self, f: unsafe fn(&GlobalCounter) -> R) -> R {
        f(self.0)
    }
}

impl Generation for GlobalGeneration {
    fn free(this: Self) {
        if this.count() != 0 {
            FREE_LIST.lock().push(this)
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum LocalOrGlobalGeneration {
    Local(LocalGeneration),
    Global(GlobalGeneration)
}

impl LocalOrGlobalGeneration {
    #[inline(always)]
    fn delegate<R>(&self, fl: fn(&LocalGeneration) -> R, fg: fn(&GlobalGeneration) -> R) -> R
    {
        match self {
            Self::Local(l) => fl(l),
            Self::Global(g) => fg(g),
        }
    }

    #[inline(always)]
    unsafe fn unsafe_delegate<R>(&self, fl: unsafe fn(&LocalGeneration) -> R, fg: unsafe fn(&GlobalGeneration) -> R) -> R
    {
        match self {
            Self::Local(l) => fl(l),
            Self::Global(g) => fg(g),
        }
    }
}

impl Generation for LocalOrGlobalGeneration {
    #[inline(always)]
    fn free(this: Self) {
        this.delegate(|l| LocalGeneration::free(*l), |g| GlobalGeneration::free(*g))
    }
}

lazy_static::lazy_static! {
    static ref FREE_LIST : Mutex<Vec<GlobalGeneration>> = Mutex::new(Vec::new());
    static ref FRESH_LIST : Mutex<(usize, &'static [GlobalCounter])> = Mutex::new((32, &[]));
}

pub(crate) struct LocalCounter(pub(crate) Cell<LocalOrGlobalCounter>);

impl LocalCounter {
    fn new() -> Self {
        Self(Cell::new(LocalOrGlobalCounter::new()))
    }

    pub(crate) fn globalize(&self) -> GlobalGeneration {
        let res = match 
        self.0.replace(LocalOrGlobalCounter::Placeholder) {
            LocalOrGlobalCounter::Placeholder => panic!(),
            LocalOrGlobalCounter::Local(l) => GlobalGeneration::from_local(l),
            LocalOrGlobalCounter::Global(g) => g,
        };
        self.0.set(LocalOrGlobalCounter::Global(res));
        res
    }

    #[inline(always)]
    fn delegate<R>(&self, f: fn(&LocalOrGlobalCounter) -> R) -> R {
        let log = self.0.replace(LocalOrGlobalCounter::Placeholder);
        let res = f(&log);
        self.0.set(log);
        return res;
    }

    #[inline(always)]
    unsafe fn unsafe_delegate<R>(&self, f: unsafe fn(&LocalOrGlobalCounter) -> R) -> R {
        let log = self.0.replace(LocalOrGlobalCounter::Placeholder);
        let res = f(&log);
        self.0.set(log);
        return res;
    }
}

pub(crate) enum LocalOrGlobalCounter {
    Placeholder,
    Local(RawLocalCounter),
    Global(GlobalGeneration)
}

impl LocalOrGlobalCounter {
   fn new() -> Self {
        Self::Local(RawLocalCounter::new())
    }

    #[inline(always)]
    fn delegate<R>(&self, fl: fn(&RawLocalCounter) -> R, fg: fn(& GlobalGeneration) -> R) -> R
    {
        match self {
            Self::Placeholder => panic!(),
            Self::Local(l) => fl(l),
            Self::Global(g) => fg(g),
        }
    }

    #[inline(always)]
    unsafe fn unsafe_delegate<R>(&self, fl: unsafe fn(&RawLocalCounter) -> R, fg: unsafe fn(&GlobalGeneration) -> R) -> R
    {
        match self {
            Self::Placeholder => panic!(),
            Self::Local(l) => fl(l),
            Self::Global(g) => fg(g),
        }
    }
}

pub(crate) const COUNTER_INIT : u32 = 1;

pub(crate) struct GlobalCounter {
    pub(crate) access: parking_lot::RawRwLock,
    pub(crate) counter: AtomicU32,
}

impl GlobalCounter {
    pub(crate) fn new() -> Self {
        Self { access: parking_lot::RawRwLock::INIT, counter: AtomicU32::new(COUNTER_INIT) }
    }

    fn set_gen(&self, gen: u32) -> bool {
        loop {
            let n = self.counter.load(Relaxed);
            if gen <= n { return false; }
            if self.counter.compare_exchange(n, gen, Relaxed, Relaxed).is_ok() { return true; };
        }
    }

    #[inline(always)]
    fn delegate<R>(&self, f: fn(&parking_lot::RawRwLock) -> R) -> R {
        f(&self.access)
    }

    #[inline(always)]
    unsafe fn unsafe_delegate<R>(&self, f: unsafe fn(&parking_lot::RawRwLock) -> R) -> R {
        f(&self.access)
    }
}

pub(crate) struct RawLocalCounter {
    pub(crate) access: Cell<i32>,
    pub(crate) counter: Cell<u32>,
}

impl RawLocalCounter {
    fn new() -> Self { Self { access: Cell::new(0), counter: Cell::new(COUNTER_INIT) } }

    fn access_state(&self) -> AccessState {
        AccessState::new(self.access.get())
    }
}

enum AccessState {
    Readers { normal: i32, upgrade: bool },
    Writer,
    None
}

impl AccessState {
    fn new(desc: i32) -> Self {
        match desc {
            1.. => Self::Readers { normal: desc / 2, upgrade: desc & 1 == 1 },
            0 => Self::None,
            -1 => Self::Writer,
            _ => panic!()
        }
    }

    fn inflict(&self, access: &parking_lot::RawRwLock) {
        use AccessState::*;
        match self {
            Readers { normal, upgrade } => {
                for _ in 0..*normal { access.lock_shared(); }
                if *upgrade { access.lock_upgradable() }
            },
            Writer => access.lock_exclusive(),
            None => {}
        }
    }
}

pub(crate) trait AccessControl {
    fn try_lock_shared(&self) -> bool;
    fn try_lock_exclusive(&self) -> bool;
    fn try_lock_upgradable(&self) -> bool;

    unsafe fn downgrade(&self);
    unsafe fn downgrade_to_upgradable(&self);
    unsafe fn downgrade_upgradable(&self);
    unsafe fn try_upgrade(&self) -> bool;
    unsafe fn unlock_shared(&self);
    unsafe fn unlock_upgradable(&self);
    unsafe fn unlock_exclusive(&self);

    unsafe fn try_shared_into_exclusive(&self) -> bool {
        if self.try_lock_upgradable() {
            self.unlock_shared();
            if self.try_upgrade() { return true; }
            if !self.try_lock_shared() { panic!() }
            self.unlock_upgradable();
        }
        false
    }
}

pub(crate) trait GenerationCounter {
    fn bump(&self);
    fn count(&self) -> u32;
}

impl GenerationCounter for RawLocalCounter {
    fn bump(&self) {
        let mut n = self.counter.get();
        if n != 0 {
            n = n.wrapping_add(1);
            self.counter.set(n);
        }
    }

    fn count(&self) -> u32 {
        self.counter.get()
    }
}

impl AccessControl for RawLocalCounter {
    fn try_lock_shared(&self) -> bool {
        if self.access.get() >= 0 {
            self.access.set(self.access.get() + 1);
            true
        } else {
            false
        }
    }

    fn try_lock_exclusive(&self) -> bool {
        if self.access.get() == 0 {
            self.access.set(-1);
            true
        } else {
            false
        }
    }

    fn try_lock_upgradable(&self) -> bool {
        self.try_lock_shared()
    }

    unsafe fn downgrade(&self) {
        match self.access.get() {
            -1 => self.access.set(1),
            _ => panic!(),
        }
    }

    unsafe fn downgrade_to_upgradable(&self) {}

    unsafe fn downgrade_upgradable(&self) {}

    unsafe fn try_upgrade(&self) -> bool {
        if self.access.get() == 1 {
            self.access.set(-1);
            true
        } else {
            false
        }
    }

    unsafe fn unlock_shared(&self) {
        let n = self.access.get();
        self.access.set(n - 1);
    }

    unsafe fn unlock_exclusive(&self) {
        self.access.set(0);
    }

    unsafe fn unlock_upgradable(&self) {
        self.unlock_shared()
    }
}

macro_rules! delegate {
    (fn $name:ident -> $ret:ty, $($sub:ty),+) => {
        #[inline(always)]
        fn $name(&self) -> $ret {
            self.delegate($(< $sub > :: $name),+)
        }
    };
    (unsafe fn $name:ident -> $ret:ty, $($sub:ty),+) => {
        #[inline(always)]
        unsafe fn $name(&self) -> $ret {
            self.unsafe_delegate($(< $sub > :: $name),+)
        }
    };
}

impl GenerationCounter for GlobalCounter {
    fn bump(&self){
        loop {
            let n = self.counter.load(Relaxed);
            if n == 0 { break; }
            let m = n.wrapping_add(1);
            if self.counter.compare_exchange(n, m, Relaxed, Relaxed).is_ok() { break; }
        }
    }

    fn count(&self) -> u32 {
        self.counter.load(Relaxed)
    }
}

impl AccessControl for GlobalCounter {

    fn try_lock_shared(&self) -> bool {
        self.access.try_lock_shared_recursive()
    }

    delegate!(fn try_lock_exclusive -> bool, parking_lot::RawRwLock);
    delegate!(fn try_lock_upgradable -> bool, parking_lot::RawRwLock);
    delegate!(unsafe fn downgrade -> (), parking_lot::RawRwLock);
    delegate!(unsafe fn downgrade_upgradable -> (), parking_lot::RawRwLock);
    delegate!(unsafe fn downgrade_to_upgradable -> (), parking_lot::RawRwLock);
    delegate!(unsafe fn try_upgrade -> bool, parking_lot::RawRwLock);
    delegate!(unsafe fn unlock_shared -> (), parking_lot::RawRwLock);
    delegate!(unsafe fn unlock_upgradable -> (), parking_lot::RawRwLock);
    delegate!(unsafe fn unlock_exclusive -> (), parking_lot::RawRwLock);
}

macro_rules! delegate_all {
    ($it:ty : use $($sub:ty),+) => {
        impl GenerationCounter for $it {
            delegate!(fn bump -> (), $($sub),+);
            delegate!(fn count -> u32, $($sub),+);
        }
        impl AccessControl for $it {
            delegate!(fn try_lock_shared -> bool, $($sub),+);
            delegate!(fn try_lock_exclusive -> bool, $($sub),+);
            delegate!(fn try_lock_upgradable -> bool, $($sub),+);
            delegate!(unsafe fn downgrade -> (), $($sub),+);
            delegate!(unsafe fn downgrade_upgradable -> (), $($sub),+);
            delegate!(unsafe fn downgrade_to_upgradable -> (), $($sub),+);
            delegate!(unsafe fn try_upgrade -> bool, $($sub),+);
            delegate!(unsafe fn unlock_shared -> (), $($sub),+);
            delegate!(unsafe fn unlock_exclusive -> (), $($sub),+);
            delegate!(unsafe fn unlock_upgradable -> (), $($sub),+);
        }
    };
}

delegate_all!(LocalOrGlobalCounter: use RawLocalCounter, GlobalGeneration);
delegate_all!(LocalCounter: use LocalOrGlobalCounter);
delegate_all!(GlobalGeneration: use GlobalCounter);
delegate_all!(LocalGeneration: use LocalCounter);
delegate_all!(LocalOrGlobalGeneration: use LocalGeneration, GlobalGeneration);