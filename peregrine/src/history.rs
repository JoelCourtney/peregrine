#![doc(hidden)]

use std::hash::{BuildHasher, Hasher};

use crate::Resource;
use dashmap::DashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use stable_deref_trait::StableDeref;
use type_map::concurrent::TypeMap;
use type_reg::untagged::TypeReg;

pub type PeregrineDefaultHashBuilder = foldhash::fast::FixedState;

#[derive(Default)]
#[repr(transparent)]
pub struct History(TypeMap);

impl History {
    pub fn init<'h, R: Resource<'h>>(&mut self) {
        self.0.insert(R::History::default());
    }
    pub fn insert<'h, R: Resource<'h>>(&'h self, hash: u64, value: R::Write) -> R::Read {
        self.0.get::<R::History>().unwrap().insert(hash, value)
    }
    pub fn get<'h, R: Resource<'h>>(&'h self, hash: u64) -> Option<R::Read> {
        self.0.get::<R::History>().map(|h| h.get(hash)).flatten()
    }
}

pub trait HistoryAdapter<W, R>: Default {
    fn insert(&self, hash: u64, value: W) -> R;
    fn get(&self, hash: u64) -> Option<R>;
}

/// See [Resource].
#[derive(Serialize, Deserialize)]
pub struct CopyHistory<T: Copy>(
    DashMap<u64, T, PassThroughHashBuilder>,
);

impl<T: Copy> Default for CopyHistory<T> {
    fn default() -> Self {
        CopyHistory(DashMap::default())
    }
}

impl<T: Copy> HistoryAdapter<T, T> for CopyHistory<T> {
    fn insert(&self, hash: u64, value: T) -> T {
        self.0.insert(hash, value);
        value
    }

    fn get(&self, hash: u64) -> Option<T> {
        self.0.get(&hash).map(|r| *r)
    }
}

/// See [Resource].
#[derive(Serialize, Deserialize)]
pub struct DerefHistory<T: StableDeref>(DashMap<u64, T, PassThroughHashBuilder>);

impl<T: StableDeref> Default for DerefHistory<T> {
    fn default() -> Self {
        DerefHistory(DashMap::default())
    }
}

impl<'h, T: StableDeref> HistoryAdapter<T, &'h T::Target> for DerefHistory<T> where Self: 'h {
    fn insert(&self, hash: u64, value: T) -> &'h T::Target {
        let inserted: *const T = &*self.0.entry(hash).or_insert(value);
        unsafe { &**inserted }
    }

    fn get(&self, hash: u64) -> Option<&'h T::Target> {
        self.0.get(&hash).map(|r| unsafe {
            let value: *const T = &*r;
            &**value
        })
    }
}

// i suspect the compiler will be able to turn this into a no-op
pub struct PassThroughHasher(u64);

impl Hasher for PassThroughHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _bytes: &[u8]) {
        unreachable!()
    }
    fn write_u8(&mut self, _i: u8) {
        unreachable!()
    }
    fn write_u16(&mut self, _i: u16) {
        unreachable!()
    }
    fn write_u32(&mut self, _i: u32) {
        unreachable!()
    }

    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }

    fn write_usize(&mut self, _i: usize) {
        unreachable!()
    }
}

#[derive(Copy, Clone, Default)]
pub struct PassThroughHashBuilder;

impl BuildHasher for PassThroughHashBuilder {
    type Hasher = PassThroughHasher;

    fn build_hasher(&self) -> PassThroughHasher {
        PassThroughHasher(0)
    }
}

pub trait HistoryPlugin: Sync {
    fn type_label(&self) -> String;
    fn ser(&self, history: &History, type_reg: &mut type_reg::untagged::TypeMap<String>);

    fn register(&self, type_reg: &mut TypeReg<String>);
    fn de(&self, history: &mut History, type_reg: &type_reg::untagged::TypeMap<String>);
}

inventory::collect!(&'static dyn HistoryPlugin);

impl Serialize for History {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        let mut ser_type_map = type_reg::untagged::TypeMap::<String>::new();

        for plugin in inventory::iter::<&'static dyn HistoryPlugin> {
            if !ser_type_map.contains_key(&plugin.type_label()) {
                plugin.ser(self, &mut ser_type_map)
            }
        }

        ser_type_map.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for History {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>
    {
        let mut type_reg = TypeReg::<String>::new();

        for plugin in inventory::iter::<&'static dyn HistoryPlugin> {
            plugin.register(&mut type_reg);
        }

        let de_type_map = type_reg.deserialize_map(deserializer)?;

        let mut history = History::default();

        for plugin in inventory::iter::<&'static dyn HistoryPlugin> {
            plugin.de(&mut history, &de_type_map);
        }

        Ok(history)
    }
}
