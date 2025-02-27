#![doc(hidden)]

use crate::operation::ungrounded::{UngroundedUpstream, UngroundedUpstreamResolver};
use crate::operation::{Node, NodeVec, Upstream};
use crate::resource::Resource;
use crate::Model;
use bumpalo_herd::{Herd, Member};
use hifitime::TimeScale::TAI;
use hifitime::{Duration, Epoch as Time};
use std::collections::btree_map::Range;
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};
use std::ops::RangeBounds;

pub trait HasTimeline<'o, R: Resource<'o>, M: Model<'o>> {
    fn find_child(&self, time: Duration) -> Option<&'o dyn Upstream<'o, R, M>>;

    fn insert_grounded(
        &mut self,
        time: Duration,
        op: &'o dyn Upstream<'o, R, M>,
    ) -> Option<&'o dyn Upstream<'o, R, M>>;
    fn remove_grounded(&mut self, time: Duration) -> Option<&'o dyn Node<'o, M>>;

    fn insert_ungrounded(
        &mut self,
        min: Duration,
        max: Duration,
        op: &'o dyn UngroundedUpstream<'o, R, M>,
    ) -> NodeVec<'o, M>;
    fn remove_ungrounded(&mut self, min: Duration) -> Option<&'o dyn Node<'o, M>>;

    fn get_operations(
        &self,
        bounds: impl RangeBounds<Duration>,
    ) -> Vec<(Duration, &'o dyn Upstream<'o, R, M>)>;
}

// All Epochs/Times are converted to TAI durations because the Ord implementation
// on Epoch does a timescale conversion every time, which is very inefficient.

// TAI (international atomic time) is chosen as the base representation
// because hifitime does all epoch conversions through TAI, so it is the most
// efficient format to convert to.
pub fn epoch_to_duration(time: Time) -> Duration {
    time.to_tai_duration()
}
pub fn duration_to_epoch(duration: Duration) -> Time {
    Time {
        duration,
        time_scale: TAI,
    }
}

pub struct Timeline<'o, R: Resource<'o>, M: Model<'o>>(BTreeMap<Duration, TimelineEntry<'o, R, M>>, Member<'o>)
where
    M::Timelines: HasTimeline<'o, R, M>;

#[derive(Default)]
struct TimelineEntry<'o, R: Resource<'o>, M: Model<'o>> {
    grounded: Option<&'o dyn Upstream<'o, R, M>>,
    ungrounded: BTreeMap<Duration, &'o dyn UngroundedUpstream<'o, R, M>>,
}

impl<'o, R: Resource<'o>, M: Model<'o>> TimelineEntry<'o, R, M> {
    fn new_empty() -> Self {
        Default::default()
    }

    fn new_grounded(gr: &'o dyn Upstream<'o, R, M>) -> Self {
        TimelineEntry {
            grounded: Some(gr),
            ungrounded: BTreeMap::new(),
        }
    }

    fn new_ungrounded(ug: &'o dyn UngroundedUpstream<'o, R, M>, max: Duration) -> Self {
        TimelineEntry {
            grounded: None,
            ungrounded: BTreeMap::from([(max, ug)]),
        }
    }

    fn merge(&mut self, other: &TimelineEntry<'o, R, M>) {
        assert_ne!(self.grounded.is_some(), other.grounded.is_some());

        self.grounded = self.grounded.or(other.grounded);
        self.ungrounded.extend(other.ungrounded.iter().cloned());
    }

    fn into_upstream(self, entry_time: Duration, eval_time: Duration, bump: &Member<'o>) -> &'o dyn Upstream<'o, R, M> {
        if self.ungrounded.is_empty() {
            self.grounded.unwrap()
        } else {
            bump.alloc(UngroundedUpstreamResolver::new(
                eval_time,
                self.grounded.map(|g| (entry_time, g)),
                self.ungrounded.into_values().collect(),
            ))
        }
    }

    fn into_node_vec(self) -> NodeVec<'o, M> {
        let mut result = self.ungrounded.into_values().map(|ug| ug.as_ref()).collect();
        result.extend(self.grounded);
        result
    }
}

impl<'o, R: Resource<'o>, M: Model<'o>> Timeline<'o, R, M>
where
    M::Timelines: HasTimeline<'o, R, M>,
{
    pub fn init(
        time: Duration,
        initial_condition: &'o dyn Upstream<'o, R, M>,
        bump: Member<'o>,
    ) -> Timeline<'o, R, M> {
        Timeline(BTreeMap::from([(
            time,
            TimelineEntry::new_grounded(initial_condition),
        )]), bump)
    }

    pub fn search_possible_upstreams(&self, time: Duration) -> Option<(Duration, TimelineEntry<'o, R, M>)> {
        let mut result = TimelineEntry::new_empty();
        let mut iter = self.0.range(..time);
        let entry_time;
        loop {
            let entry = iter.next_back()?;
            result.merge(entry.1);
            if result.grounded.is_some()
                || result
                    .ungrounded
                    .first_entry()
                    .map(|e| e.key() <= &time)
                    .unwrap_or(false)
            {
                entry_time = *entry.0;
                break;
            }
        }

        Some((entry_time, result))
    }

    pub fn last_before(&self, eval_time: Duration) -> Option<&'o dyn Upstream<'o, R, M>> {
        let (entry_time, possible) = self.search_possible_upstreams(time)?;
        Some(possible.into_upstream(entry_time, eval_time, &self.1))
    }

    pub fn insert_grounded(
        &mut self,
        time: Duration,
        value: &'o (dyn Upstream<'o, R, M>),
    ) -> NodeVec<'o, M> {
        self.0.insert(time, TimelineEntry::new_grounded(value));
        self.search_possible_upstreams(time).map(|(_, e)| e.into_node_vec()).unwrap_or(NodeVec::new())
    }

    pub fn remove_grounded(&mut self, time: Duration) -> bool {
        self.0.remove(&time).is_some()
    }

    pub fn insert_ungrounded(
        &mut self,
        min: Duration,
        max: Duration,
        value: &'o dyn UngroundedUpstream<'o, R, M>,
    ) -> NodeVec<'o, M> {
        let mut entry = TimelineEntry::new_ungrounded(value, max);
        entry.ungrounded.extend(
            self.0
                .range(..min)
                .next_back()
                .map(|(_, entry)| entry.ungrounded.range((Excluded(min), Unbounded)))
                .unwrap_or(Range::default()),
        );

        // Need to collect the list of all nodes that might lose a downstream after this change
        let mut result = NodeVec::new();
        let mut ungrounded_collector = TimelineEntry::new_empty();
        for (_, e) in self.0.range_mut(min..max) {
            ungrounded_collector.merge(e);
            if let Some(gr) = ungrounded_collector.grounded.take() {
                result.push(gr.as_ref());
            }

            e.ungrounded.insert(max, value);
        }
        self.0.insert(min, entry);

        result.extend(ungrounded_collector.ungrounded.into_values().map(|ug| ug.as_ref()));
        result
    }

    pub fn remove_ungrounded(
        &mut self,
        min: Duration,
        max: Duration,
    ) -> bool {
        let entry = self.0.remove(&min);
        if entry.is_some() {
            for (_, e) in self.0.range_mut(min..max) {
                e.ungrounded.remove(&max);
            }
            true
        } else {
            false
        }
    }

    pub fn range<'a>(
        &'a self,
        range: impl RangeBounds<Duration>,
    ) -> impl Iterator<Item = MaybeGrounded<'o, R, M>> + 'a {
        let mut result = Vec::new();
        let mut ungrounded_collector = TimelineEntry::new_empty();
        for (t, e) in self.0.range(range) {
            ungrounded_collector.merge(e);
            if let Some(gr) = ungrounded_collector.grounded.take() {
                result.push(MaybeGrounded::Grounded((*t, gr)));
            }
        }

        result.extend(ungrounded_collector.ungrounded.into_values().map(|ug| MaybeGrounded::Ungrounded(ug)));
        result
    }
}

pub enum MaybeGrounded<'o, R: Resource<'o>, M: Model<'o>> {
    Grounded((Duration, &'o dyn Upstream<'o, R, M>)),
    Ungrounded(&'o dyn UngroundedUpstream<'o, R, M>),
}
