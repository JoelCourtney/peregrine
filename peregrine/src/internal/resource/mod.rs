#[cfg(feature = "uom")]
mod uom;

#[cfg(feature = "bigdecimal")]
mod bigdecimal;

#[cfg(feature = "nalgebra")]
mod nalgebra;

mod basic;
mod num;

use type_map::concurrent::TypeMap;
use type_reg::untagged::TypeReg;

#[doc(hidden)]
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
