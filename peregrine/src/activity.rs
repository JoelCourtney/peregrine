use crate::operation::Node;
use crate::{Model, Time};
use anyhow::Result;
use bumpalo_herd::Member;
use hifitime::Duration;
use peregrine_macros::impl_activity;
use serde::{Deserialize, Serialize};

/// An activity, which decomposes into a statically-known set of operations. Implemented
/// with the [impl_activity] macro.
pub trait Activity<'o, M: Model<'o>>: Send + Sync {
    fn decompose(
        &'o self,
        start: Placement,
        timelines: &M::Timelines,
        bump: &Member<'o>,
    ) -> Result<(Duration, Vec<&'o dyn Node<'o, M>>)>;
}

pub trait ActivityLabel {
    const LABEL: &'static str;
}

pub enum Placement {
    Grounded(Time),
    Ungrounded { min: Time, max: Time },
}

/// A unique activity ID.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, Debug)]
pub struct ActivityId(u32);
impl ActivityId {
    pub fn new(id: u32) -> ActivityId {
        ActivityId(id)
    }
}
