#![doc(hidden)]

use crate::Time;
use crate::resource::ResourceHistoryPlugin;
use crate::resource::{Data, Resource};
use ahash::AHasher;
use dashmap::DashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::hash::{BuildHasher, Hasher};
use std::mem::swap;
use type_map::concurrent::{Entry, TypeMap};
use type_reg::untagged::TypeReg;

pub type PeregrineDefaultHashBuilder = AHasher;

#[derive(Default)]
#[repr(transparent)]
pub struct History(TypeMap);

impl History {
    pub fn new() -> Self {
        History(TypeMap::new())
    }
    pub fn init<R: Resource>(&mut self) {
        match self.0.entry::<InnerHistory<R::Data>>() {
            Entry::Occupied(_) => {}
            Entry::Vacant(v) => {
                v.insert(InnerHistory::default());
            }
        }
    }
    pub fn insert<R: Resource>(
        &self,
        hash: u64,
        value: R::Data,
        written: Time,
    ) -> <R::Data as Data>::Read {
        self.0
            .get::<InnerHistory<R::Data>>()
            .unwrap()
            .insert(hash, value, written)
    }
    pub fn get<R: Resource>(&self, hash: u64, written: Time) -> Option<<R::Data as Data>::Read> {
        self.0
            .get::<InnerHistory<R::Data>>()
            .and_then(|h| h.get(hash, written))
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

const DASHMAP_STARTING_CAPACITY: usize = 1000;

/// See [Resource].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InnerHistory<T>(DashMap<u64, T, PassThroughHashBuilder>);

impl<T: for<'h> Data<'h>> Default for InnerHistory<T> {
    fn default() -> Self {
        InnerHistory(DashMap::with_capacity_and_hasher(
            DASHMAP_STARTING_CAPACITY,
            PassThroughHashBuilder,
        ))
    }
}

impl<'h, T: Data<'h>> InnerHistory<T>
where
    Self: 'h,
{
    fn insert(&self, hash: u64, value: T, written: Time) -> T::Read {
        let inserted = self.0.entry(hash).or_insert(value);
        inserted.to_read(written)
    }

    fn get(&self, hash: u64, written: Time) -> Option<T::Read> {
        self.0.get(&hash).map(move |r| r.value().to_read(written))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macro_prelude::duration_to_epoch;
    use crate::resource;
    use bincode::config::standard;
    use hifitime::Duration;

    const TIME: Time = duration_to_epoch(Duration::ZERO);

    #[test]
    fn deref_history_valid_across_realloc() {
        let history = InnerHistory::<String>::default();

        // Chosen by button mashing :)
        let hash = 0b10110100100101001010;
        history.insert(hash, "Hello World!".to_string(), TIME);
        let reference = history.get(hash, TIME).unwrap();
        assert_eq!("Hello World!", reference);

        // History default capacity is 1000.
        for _ in 0..2_000 {
            history.insert(rand::random(), "its a string".to_string(), TIME);
        }

        assert_eq!("Hello World!", reference);
    }

    resource!(a: u32);
    resource!(b: String);

    #[test]
    fn history_serde() -> anyhow::Result<()> {
        let mut history = History::default();
        history.init::<a>();
        history.init::<b>();

        history.insert::<a>(0, 5, TIME);
        history.insert::<a>(1, 6, TIME);
        history.insert::<b>(10, "string".to_string(), TIME);
        history.insert::<b>(11, "another string".to_string(), TIME);

        let serialized = bincode::serde::encode_to_vec(history, standard())?;
        let deserialized: History = bincode::serde::decode_from_slice(&serialized, standard())?.0;

        assert_eq!(5, deserialized.get::<a>(0, TIME).unwrap());
        assert_eq!(6, deserialized.get::<a>(1, TIME).unwrap());

        assert_eq!("string", deserialized.get::<b>(10, TIME).unwrap());
        assert_eq!("another string", deserialized.get::<b>(11, TIME).unwrap());

        assert_eq!(None, deserialized.get::<a>(100, TIME));

        Ok(())
    }
}
