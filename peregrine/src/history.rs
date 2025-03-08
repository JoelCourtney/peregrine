#![doc(hidden)]

use crate::resource::Resource;
use crate::resource::ResourceHistoryPlugin;
use dashmap::DashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use stable_deref_trait::StableDeref;
use std::hash::{BuildHasher, Hasher};
use std::mem::swap;
use type_map::concurrent::{Entry, TypeMap};
use type_reg::untagged::TypeReg;

pub type PeregrineDefaultHashBuilder = foldhash::fast::FixedState;

#[derive(Default)]
#[repr(transparent)]
pub struct History(TypeMap);

impl History {
    pub fn new() -> Self {
        History(TypeMap::new())
    }
    pub fn init<'h, R: Resource<'h>>(&mut self) {
        match self.0.entry::<R::History>() {
            Entry::Occupied(_) => {}
            Entry::Vacant(v) => {
                v.insert(R::History::default());
            }
        }
    }
    pub fn insert<'h, R: Resource<'h>>(&'h self, hash: u64, value: R::Write) -> R::Read {
        self.0.get::<R::History>().unwrap().insert(hash, value)
    }
    pub fn get<'h, R: Resource<'h>>(&'h self, hash: u64) -> Option<R::Read> {
        self.0.get::<R::History>().and_then(|h| h.get(hash))
    }
    pub fn take_inner(&mut self) -> TypeMap {
        let mut replacement = TypeMap::new();
        swap(&mut self.0, &mut replacement);
        replacement
    }
    pub fn into_inner(self) -> TypeMap {
        self.0
    }
}

impl From<TypeMap> for History {
    fn from(value: TypeMap) -> Self {
        History(value)
    }
}

pub trait HistoryAdapter<W, R>: Default {
    fn insert(&self, hash: u64, value: W) -> R;
    fn get(&self, hash: u64) -> Option<R>;
}

const DASHMAP_STARTING_CAPACITY: usize = 1000;

/// See [Resource].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CopyHistory<T: Copy + Clone>(DashMap<u64, T, PassThroughHashBuilder>);

impl<T: Copy + Clone> Default for CopyHistory<T> {
    fn default() -> Self {
        CopyHistory(DashMap::with_capacity_and_hasher(
            DASHMAP_STARTING_CAPACITY,
            PassThroughHashBuilder,
        ))
    }
}

impl<T: Copy + Clone> HistoryAdapter<T, T> for CopyHistory<T> {
    fn insert(&self, hash: u64, value: T) -> T {
        self.0.insert(hash, value);
        value
    }

    fn get(&self, hash: u64) -> Option<T> {
        self.0.get(&hash).map(|r| *r)
    }
}

/// See [Resource].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DerefHistory<T: StableDeref + Clone>(DashMap<u64, T, PassThroughHashBuilder>);

impl<T: StableDeref + Clone> Default for DerefHistory<T> {
    fn default() -> Self {
        DerefHistory(DashMap::with_capacity_and_hasher(
            DASHMAP_STARTING_CAPACITY,
            PassThroughHashBuilder,
        ))
    }
}

impl<'h, T: StableDeref + Clone + From<&'h T::Target>> HistoryAdapter<T, &'h T::Target>
    for DerefHistory<T>
where
    Self: 'h,
{
    fn insert(&self, hash: u64, value: T) -> &'h T::Target {
        let inserted: *const T = &*self.0.entry(hash).or_insert(value);
        unsafe { &*inserted }
    }

    fn get(&self, hash: u64) -> Option<&'h T::Target> {
        self.0.get(&hash).map(|r| unsafe {
            let value: *const T = &*r;
            &**value
        })
    }
}

impl<W, R> HistoryAdapter<W, R> for () {
    fn insert(&self, _hash: u64, _value: W) -> R {
        unreachable!()
    }

    fn get(&self, _hash: u64) -> Option<R> {
        unreachable!()
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

inventory::collect!(&'static dyn ResourceHistoryPlugin);

impl Serialize for History {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut ser_type_map = type_reg::untagged::TypeMap::<String>::new();

        for plugin in inventory::iter::<&'static dyn ResourceHistoryPlugin> {
            if !ser_type_map.contains_key(&plugin.write_type_string()) {
                plugin.ser(&self.0, &mut ser_type_map)
            }
        }

        ser_type_map.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for History {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut type_reg = TypeReg::<String>::new();

        for plugin in inventory::iter::<&'static dyn ResourceHistoryPlugin> {
            plugin.register(&mut type_reg);
        }

        let mut de_type_map = type_reg.deserialize_map(deserializer)?;

        let mut result = TypeMap::new();

        for plugin in inventory::iter::<&'static dyn ResourceHistoryPlugin> {
            plugin.de(&mut result, &mut de_type_map);
        }

        Ok(result.into())
    }
}
