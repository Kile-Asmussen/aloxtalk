use crate::object::*;
use std::mem;
use std::mem::ManuallyDrop;

use crate::memory::{
    pointers::{LocalOrGlobal, OwnershipBit, RawRef},
    Strong, Weak,
};

#[repr(transparent)]
pub struct Slot(RawSlot);

impl Drop for Slot {
    fn drop(&mut self) {
        let _ = SlotEnum::from(self.0);
    }
}

#[derive(Clone, Copy)]
union RawSlot {
    int: Int,
    raw: RawRef<Object>,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct Int {
    val: i128,
    nonzero: u32,
    discriminant: LocalOrGlobal,
    ownership: OwnershipBit,
}

impl From<SlotEnum> for Slot {
    fn from(it: SlotEnum) -> Self {
        Self(it.into())
    }
}

impl From<SlotEnum> for RawSlot {
    fn from(it: SlotEnum) -> Self {
        match it {
            SlotEnum::Nil => RawSlot {
                int: Int {
                    val: 0,
                    nonzero: 0,
                    discriminant: LocalOrGlobal::Neither,
                    ownership: OwnershipBit::Copy,
                },
            },
            SlotEnum::Int(val) => Self {
                int: Int {
                    val,
                    nonzero: !0,
                    discriminant: LocalOrGlobal::Neither,
                    ownership: OwnershipBit::Copy,
                },
            },
            SlotEnum::Strong(s) => Self { raw: s.into_raw() },
            SlotEnum::Weak(w) => Self { raw: w.as_raw() },
        }
    }
}

impl SlotEnum {
    fn from_raw(it: RawRef<Object>) -> Self {
        match it.ownership {
            Integer => panic!("Nil ownership"),
            Inferred => panic!("Inferred ownership"),
            OwnershipBit::Weak => Self::Weak(unsafe { Weak::from_raw(it) }),
            OwnershipBit::Strong => Self::Strong(unsafe { Strong::from_raw(it) }),
        }
    }
}

impl From<Slot> for SlotEnum {
    fn from(it: Slot) -> Self {
        let res = it.0.into();
        mem::forget(it);
        res
    }
}

impl From<RawSlot> for SlotEnum {
    fn from(it: RawSlot) -> Self {
        use LocalOrGlobal::*;
        use OwnershipBit::*;
        unsafe {
            match it {
                RawSlot {
                    int:
                        Int {
                            val,
                            nonzero: 1..,
                            discriminant: Neither,
                            ownership: Copy,
                        },
                } => SlotEnum::Int(val),
                RawSlot {
                    int:
                        Int {
                            val: 0,
                            nonzero: 0,
                            discriminant: Neither,
                            ownership: Copy,
                        },
                } => SlotEnum::Nil,
                RawSlot {
                    int:
                        Int {
                            discriminant: Local | Global,
                            ownership: Weak | Strong,
                            ..
                        },
                } => SlotEnum::from_raw(it.raw),
                _ => panic!(),
            }
        }
    }
}

enum SlotEnum {
    Nil,
    Int(i128),
    Strong(Strong<Object>),
    Weak(Weak<Object>),
}

#[test]
fn slot_size() {
    assert_eq!(mem::size_of::<RawSlot>(), 3 * mem::size_of::<usize>())
}
