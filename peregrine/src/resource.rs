use crate::history::HistoryAdapter;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use type_map::concurrent::TypeMap;
use type_reg::untagged::TypeReg;

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
pub trait Resource<'h>: Sync + ErasedResource<'h> {
    const LABEL: &'static str;

    /// Whether the resource represents a value that can vary even when not actively written to by
    /// an operation. This is used for cache invalidation.
    const STATIC: bool;

    const ID: u64;

    /// The type that is read from history.
    type Read: 'h + Copy + Send + Sync + Debug;

    /// The type that is written from operations to history.
    type Write: 'h + Clone + Debug + Serialize + DeserializeOwned + Send + Sync;

    /// The type of history container to use to store instances of the `Write` type, currently
    /// either [CopyHistory] or [DerefHistory]. See [Resource] for details.
    type History: 'static + HistoryAdapter<Self::Write, Self::Read> + Debug + Default + Send + Sync;
}

#[macro_export]
macro_rules! resource {
    ($vis:vis $name:ident: $ty:ty) => {
        #[derive(Debug, $crate::reexports::serde::Serialize, $crate::reexports::serde::Deserialize)]
        #[serde(crate = "peregrine::reexports::serde")]
        #[allow(non_camel_case_types)]
        $vis enum $name {
            Unit
        }

        impl<'h> $crate::resource::Resource<'h> for $name {
            const LABEL: &'static str = $crate::reexports::peregrine_macros::code_to_str!($name);
            const STATIC: bool = true;
            const ID: u64 = $crate::reexports::peregrine_macros::random_u64!();
            type Read = $ty;
            type Write = $ty;
            type History = $crate::history::CopyHistory<$ty>;
        }

        impl $crate::resource::ResourceHistoryPlugin for $name {
            fn write_type_string(&self) -> String {
                $crate::reexports::peregrine_macros::code_to_str!($ty).to_string()
            }

            fn ser<'h>(&self, input: &'h $crate::reexports::type_map::concurrent::TypeMap, type_map: &'h mut $crate::reexports::type_reg::untagged::TypeMap<String>) {
                if let Some(h) = input.get::<$crate::history::CopyHistory<$ty>>() {
                    type_map.insert(self.write_type_string(), h.clone());
                }
            }

            fn register(&self, type_reg: &mut $crate::reexports::type_reg::untagged::TypeReg<String>) {
                type_reg.register::<$crate::history::CopyHistory<$ty>>(self.write_type_string());
            }
            fn de<'h>(&self, output: &'h mut $crate::reexports::type_map::concurrent::TypeMap, type_map: &'h mut $crate::reexports::type_reg::untagged::TypeMap<String>) {
                match type_map.remove(&self.write_type_string()) {
                    Some(sub) => {
                        let sub_history = sub.into_inner().downcast::<$crate::history::CopyHistory<$ty>>();
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
    };

    ($vis:vis ref $name:ident: $ty:ty) => {
        #[derive(Debug, $crate::reexports::serde::Serialize, $crate::reexports::serde::Deserialize)]
        #[serde(crate = "peregrine::reexports::serde")]
        #[allow(non_camel_case_types)]
        $vis enum $name {
            Unit
        }

        impl<'h> $crate::resource::Resource<'h> for $name {
            const LABEL: &'static str = $crate::reexports::peregrine_macros::code_to_str!($name);
            const ID: u64 = $crate::reexports::peregrine_macros::random_u64!();
            const STATIC: bool = true;
            type Read = &'h <$ty as std::ops::Deref>::Target;
            type Write = $ty;
            type History = $crate::history::DerefHistory<$ty>;
        }

        impl $crate::resource::ResourceHistoryPlugin for $name {
            fn write_type_string(&self) -> String {
                $crate::reexports::peregrine_macros::code_to_str!($ty).to_string()
            }

            fn ser<'h>(&self, input: &'h $crate::reexports::type_map::concurrent::TypeMap, type_map: &'h mut $crate::reexports::type_reg::untagged::TypeMap<String>) {
                if let Some(h) = input.get::<$crate::history::DerefHistory<$ty>>() {
                    type_map.insert(self.write_type_string(), h.clone());
                }
            }

            fn register(&self, type_reg: &mut $crate::reexports::type_reg::untagged::TypeReg<String>) {
                type_reg.register::<$crate::history::DerefHistory<$ty>>(self.write_type_string());
            }
            fn de<'h>(&self, output: &'h mut $crate::reexports::type_map::concurrent::TypeMap, type_map: &'h mut $crate::reexports::type_reg::untagged::TypeMap<String>) {
                match type_map.remove(&self.write_type_string()) {
                    Some(sub) => {
                        let sub_history = sub.into_inner().downcast::<$crate::history::DerefHistory<$ty>>();
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
    };
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

pub trait ErasedResource<'o>: 'o + Send + Sync {
    fn id(&self) -> u64;
}

impl<'o> dyn ErasedResource<'o> {
    pub(crate) unsafe fn downcast<TO: ErasedResource<'o>>(&self) -> &'o TO {
        unsafe { &*(self as *const Self as *const TO) }
    }

    pub(crate) unsafe fn downcast_mut<TO: ErasedResource<'o>>(&mut self) -> &'o mut TO {
        unsafe { &mut *(self as *mut Self as *mut TO) }
    }
    pub(crate) unsafe fn downcast_owned<TO: ErasedResource<'static> + Sized>(
        self: Box<Self>,
    ) -> Box<TO> {
        unsafe { Box::from_raw(Box::into_raw(self) as *mut TO) }
    }
}

impl<'o, R: Resource<'o>> ErasedResource<'o> for R {
    fn id(&self) -> u64 {
        Self::ID
    }
}
