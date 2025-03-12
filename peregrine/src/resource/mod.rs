pub mod builtins;
pub mod impls;
pub mod piecewise;
pub mod polynomial;

use crate::Time;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::hash::Hasher;
use type_map::concurrent::TypeMap;
use type_reg::untagged::TypeReg;

pub trait Data<'h>:
    'static + MaybeHash + Clone + Serialize + DeserializeOwned + Send + Sync
{
    type Read: 'h + Copy + Send + Sync;
    type Sample: 'h + MaybeHash;

    fn to_read(&self, written: Time) -> Self::Read;
    fn from_read(read: Self::Read, now: Time) -> Self;
    fn sample(read: &Self::Read, now: Time) -> Self::Sample;
}

/// Marks a type as a resource label.
///
/// Resources are not part of a model, the model is a selection of existing resources. This allows
/// activities, which are also not part of a model, to be applied to any model that has the relevant
/// resources.
///
/// ## Reading & Writing
///
/// Resources are not represented one data type, but two, one for reading and one for writing.
/// For simple [Copy] resources these two types will be the same, and you won't have to worry about it.
/// For more complex resources they may be different but related types, like [String] and [&str][str].
/// This is for performance reasons, to avoid unnecessary cloning of heap-allocated data.
pub trait Resource: 'static + Sync {
    const LABEL: &'static str;

    const ID: u64;

    /// The type that is written from operations to history.
    type Data: for<'h> Data<'h>;
}

#[macro_export]
macro_rules! resource {
    ($($vis:vis $name:ident: $ty:ty),* $(,)?) => {
        $(
            #[derive(Debug, peregrine::reexports::serde::Serialize, peregrine::reexports::serde::Deserialize)]
            #[serde(crate = "peregrine::reexports::serde")]
            #[allow(non_camel_case_types)]
            $vis enum $name {
                Unit
            }

            impl $crate::resource::Resource for $name {
                const LABEL: &'static str = $crate::reexports::peregrine_macros::code_to_str!(#name);
                const ID: u64 = $crate::reexports::peregrine_macros::random_u64!();
                type Data = $ty;
            }

            impl $crate::resource::ResourceHistoryPlugin for $name {
                fn write_type_string(&self) -> String {
                    $crate::reexports::peregrine_macros::code_to_str!($ty).to_string()
                }

                fn ser<'h>(&self, input: &'h $crate::reexports::type_map::concurrent::TypeMap, type_map: &'h mut $crate::reexports::type_reg::untagged::TypeMap<String>) {
                    if let Some(h) = input.get::<$crate::history::InnerHistory<$ty>>() {
                        type_map.insert(self.write_type_string(), h.clone());
                    }
                }

                fn register(&self, type_reg: &mut $crate::reexports::type_reg::untagged::TypeReg<String>) {
                    type_reg.register::<$crate::history::InnerHistory<$ty>>(self.write_type_string());
                }
                fn de<'h>(&self, output: &'h mut $crate::reexports::type_map::concurrent::TypeMap, type_map: &'h mut $crate::reexports::type_reg::untagged::TypeMap<String>) {
                    match type_map.remove(&self.write_type_string()) {
                        Some(sub) => {
                            let sub_history = sub.into_inner().downcast::<$crate::history::InnerHistory<$ty>>();
                            match sub_history {
                                Ok(downcasted) => {
                                    output.insert(*downcasted);
                                }
                                Err(_) => unreachable!()
                            }
                        }
                        None => {}
                    }
                }
            }

            $crate::reexports::inventory::submit!(&$name::Unit as &dyn $crate::resource::ResourceHistoryPlugin);
        )*
    };
}

pub trait MaybeHash {
    fn is_hashable(&self) -> bool;
    fn hash_unchecked<H: Hasher>(&self, state: &mut H);
}

impl MaybeHash for f32 {
    fn is_hashable(&self) -> bool {
        self.is_normal()
    }
    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        state.write(&self.to_be_bytes());
    }
}

impl MaybeHash for f64 {
    fn is_hashable(&self) -> bool {
        self.is_normal()
    }
    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        state.write(&self.to_be_bytes());
    }
}

pub trait ResourceHistoryPlugin: Sync {
    fn write_type_string(&self) -> String;

    fn ser<'h>(&self, input: &'h TypeMap, type_map: &'h mut type_reg::untagged::TypeMap<String>);

    fn register(&self, type_reg: &mut TypeReg<String>);
    fn de<'h>(
        &self,
        output: &'h mut TypeMap,
        type_reg: &'h mut type_reg::untagged::TypeMap<String>,
    );
}

pub trait ErasedResource: Send + Sync {
    fn id(&self) -> u64;
}

impl dyn ErasedResource {
    pub(crate) unsafe fn _downcast<TO: ErasedResource>(&self) -> &TO {
        unsafe { &*(self as *const Self as *const TO) }
    }

    pub(crate) unsafe fn _downcast_mut<TO: ErasedResource>(&mut self) -> &mut TO {
        unsafe { &mut *(self as *mut Self as *mut TO) }
    }
    pub(crate) unsafe fn downcast_owned<TO: ErasedResource + Sized>(self: Box<Self>) -> Box<TO> {
        unsafe { Box::from_raw(Box::into_raw(self) as *mut TO) }
    }
}
