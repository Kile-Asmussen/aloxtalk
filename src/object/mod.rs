use std::sync::mpsc::*;
use std::{collections::HashMap, mem::ManuallyDrop};

use crate::memory::{Transferrable, Weak};

use self::slots::Slot;

pub(crate) mod slots;

struct Object {
    class: &'static Class,
    data: ObjectUnion,
}

struct Class {}

union ObjectUnion {
    boolean: ManuallyDrop<(bool, Slot)>,
    string: ManuallyDrop<String>,
    symbol: &'static str,
    array: ManuallyDrop<Vec<Slot>>,
    hash: ManuallyDrop<HashMap<Weak<Object>, Slot>>,
    record: ManuallyDrop<Vec<Slot>>,
    bag: ManuallyDrop<HashMap<Symbol, Slot>>,
    message: ManuallyDrop<(Symbol, Vec<Slot>, HashMap<Symbol, Slot>)>,
    procedure: ManuallyDrop<Procedure>,
    class: &'static Class,
    out_channel: ManuallyDrop<Sender<Transferrable<Slot>>>,
    in_channel: ManuallyDrop<Receiver<Transferrable<Slot>>>,
    file: usize,
    thread: usize,
    extended: ManuallyDrop<(&'static Class, Slot)>,
}

type Symbol = &'static str;
struct Interner {}

struct Procedure {}
