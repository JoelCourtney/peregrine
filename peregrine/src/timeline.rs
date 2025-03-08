#![doc(hidden)]

use crate::Model;
use crate::history::PassThroughHashBuilder;
use crate::operation::grounding::{UngroundedUpstream, UngroundedUpstreamResolver};
use crate::operation::initial_conditions::InitialConditionOp;
use crate::operation::{Upstream, UpstreamVec};
use crate::resource::{ErasedResource, Resource};
use bumpalo_herd::{Herd, Member};
use hifitime::TimeScale::TAI;
use hifitime::{Duration, Epoch as Time};
use std::collections::{BTreeMap, HashMap};
use std::marker::PhantomData;
use std::ops::Bound::{Excluded, Unbounded};
use std::ops::{Bound, RangeBounds};

pub struct Timelines<'o, M: Model<'o> + ?Sized>(
    HashMap<u64, Box<dyn ErasedResource<'o>>, PassThroughHashBuilder>,
    &'o Herd,
    PhantomData<&'o M>,
);

impl<'o, M: Model<'o>> Timelines<'o, M> {
    pub fn new(herd: &'o Herd) -> Self {
        Self(
            HashMap::with_hasher(PassThroughHashBuilder),
            herd,
            PhantomData,
        )
    }

    pub fn init_for_resource<R: Resource<'o>>(
        &mut self,
        time: Duration,
        op: InitialConditionOp<'o, R, M>,
    ) {
        assert!(!self.0.contains_key(&R::ID));
        self.0.insert(
            R::ID,
            Box::new(Timeline::init(time, self.1.get().alloc(op))),
        );
    }

    pub fn contains_resource<R: Resource<'o>>(&self) -> bool {
        self.0.contains_key(&R::ID)
    }

    pub fn find_upstream<R: Resource<'o>>(
        &self,
        time: Duration,
    ) -> Option<&'o dyn Upstream<'o, R, M>> {
        unsafe {
            self.0
                .get(&R::ID)?
                .downcast::<Timeline<'o, R, M>>()
                .last_before(time, self.1.get())
        }
    }

    pub fn insert_grounded<R: Resource<'o>>(
        &mut self,
        time: Duration,
        op: &'o dyn Upstream<'o, R, M>,
    ) -> UpstreamVec<'o, R, M> {
        unsafe {
            self.0
                .get_mut(&R::ID)
                .unwrap()
                .downcast_mut::<Timeline<'o, R, M>>()
                .insert_grounded(time, op)
        }
    }
    pub fn remove_grounded<R: Resource<'o> + 'o>(&mut self, time: Duration) -> bool {
        unsafe {
            self.0
                .get_mut(&R::ID)
                .unwrap()
                .downcast_mut::<Timeline<'o, R, M>>()
                .remove_grounded(time)
        }
    }

    pub fn insert_ungrounded<R: Resource<'o>>(
        &mut self,
        min: Duration,
        max: Duration,
        op: &'o dyn UngroundedUpstream<'o, R, M>,
    ) -> UpstreamVec<'o, R, M> {
        unsafe {
            self.0
                .get_mut(&R::ID)
                .unwrap()
                .downcast_mut::<Timeline<'o, R, M>>()
                .insert_ungrounded(min, max, op)
        }
    }

    pub fn remove_ungrounded<R: Resource<'o> + 'o>(
        &mut self,
        min: Duration,
        max: Duration,
    ) -> bool {
        unsafe {
            self.0
                .get_mut(&R::ID)
                .unwrap()
                .downcast_mut::<Timeline<'o, R, M>>()
                .remove_ungrounded(min, max)
        }
    }

    pub(crate) fn range<R: Resource<'o>>(
        &self,
        bounds: impl RangeBounds<Duration>,
    ) -> Vec<MaybeGrounded<'o, R, M>> {
        unsafe {
            self.0
                .get(&R::ID)
                .unwrap()
                .downcast::<Timeline<'o, R, M>>()
                .range(bounds)
        }
    }
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

pub struct Timeline<'o, R: Resource<'o>, M: Model<'o>>(BTreeMap<Duration, TimelineEntry<'o, R, M>>);

pub struct TimelineEntry<'o, R: Resource<'o>, M: Model<'o>> {
    pub grounded: Option<&'o dyn Upstream<'o, R, M>>,
    pub ungrounded: BTreeMap<Duration, &'o dyn UngroundedUpstream<'o, R, M>>,
}

impl<'o, R: Resource<'o>, M: Model<'o>> TimelineEntry<'o, R, M> {
    fn new_empty() -> Self {
        TimelineEntry {
            grounded: None,
            ungrounded: BTreeMap::new(),
        }
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

        self.grounded = self.grounded.take().or(other.grounded);
        self.ungrounded
            .extend(other.ungrounded.iter().map(|(d, n)| (*d, *n)));
    }

    pub fn into_upstream(
        self,
        entry_time: Duration,
        eval_time: Duration,
        bump: Member<'o>,
    ) -> &'o dyn Upstream<'o, R, M> {
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

    pub fn into_upstream_vec(self) -> UpstreamVec<'o, R, M> {
        let mut result: UpstreamVec<'o, R, M> = self
            .ungrounded
            .into_values()
            .map(|ug| ug.as_ref())
            .collect();
        result.extend(self.grounded);
        result
    }
}

impl<'o, R: Resource<'o>, M: Model<'o>> Timeline<'o, R, M> {
    pub fn init(
        time: Duration,
        initial_condition: &'o dyn Upstream<'o, R, M>,
    ) -> Timeline<'o, R, M> {
        Timeline(BTreeMap::from([(
            time,
            TimelineEntry::new_grounded(initial_condition),
        )]))
    }

    fn search_possible_upstreams(
        &self,
        time: Duration,
    ) -> Option<(Duration, TimelineEntry<'o, R, M>)> {
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

    pub fn last_before(
        &self,
        eval_time: Duration,
        bump: Member<'o>,
    ) -> Option<&'o dyn Upstream<'o, R, M>> {
        let (entry_time, possible) = self.search_possible_upstreams(eval_time)?;
        Some(possible.into_upstream(entry_time, eval_time, bump))
    }

    #[cfg(not(feature = "nightly"))]
    pub fn insert_grounded(
        &mut self,
        time: Duration,
        value: &'o dyn Upstream<'o, R, M>,
    ) -> UpstreamVec<'o, R, M> {
        self.0.insert(time, TimelineEntry::new_grounded(value));
        self.search_possible_upstreams(time)
            .map(|e| e.1.into_upstream_vec())
            .unwrap_or_default()
    }

    #[cfg(feature = "nightly")]
    pub fn insert_grounded(
        &mut self,
        time: Duration,
        value: &'o dyn Upstream<'o, R, M>,
    ) -> UpstreamVec<'o, R, M> {
        let mut cursor_mut = self.0.upper_bound_mut(Unbounded);
        let mut cursor_mut = if let Some((t, _)) = cursor_mut.peek_prev() {
            if *t < time {
                cursor_mut
            } else {
                self.0.upper_bound_mut(Bound::Included(&time))
            }
        } else {
            self.0.upper_bound_mut(Bound::Included(&time))
        };

        let mut new_entry = TimelineEntry::new_grounded(value);

        let continuing_ungrounded = cursor_mut
            .peek_prev()
            .unwrap()
            .1
            .ungrounded
            .range((Excluded(&time), Unbounded));
        new_entry.ungrounded.extend(continuing_ungrounded);

        cursor_mut.insert_after(time, new_entry).unwrap();

        let mut result = TimelineEntry::new_empty();
        loop {
            let entry = cursor_mut.prev().unwrap();
            result.merge(entry.1);
            if result.grounded.is_some()
                || result
                    .ungrounded
                    .first_entry()
                    .map(|e| e.key() <= &time)
                    .unwrap_or(false)
            {
                break result.into_upstream_vec();
            }
        }
    }

    pub fn remove_grounded(&mut self, time: Duration) -> bool {
        self.0.remove(&time).is_some()
    }

    pub fn insert_ungrounded(
        &mut self,
        min: Duration,
        max: Duration,
        value: &'o dyn UngroundedUpstream<'o, R, M>,
    ) -> UpstreamVec<'o, R, M> {
        let mut entry = TimelineEntry::new_ungrounded(value, max);
        entry.ungrounded.extend(
            self.0
                .range(..min)
                .next_back()
                .map(|(_, entry)| entry.ungrounded.range((Excluded(min), Unbounded)))
                .unwrap_or_default(),
        );

        // Need to collect the list of all nodes that might lose a downstream after this change
        let mut result = UpstreamVec::new();
        let mut ungrounded_collector = TimelineEntry::new_empty();
        for (_, e) in self.0.range_mut(min..max) {
            ungrounded_collector.merge(e);
            if let Some(gr) = ungrounded_collector.grounded.take() {
                result.push(gr);
            }

            e.ungrounded.insert(max, value);
        }

        result.extend(
            ungrounded_collector
                .ungrounded
                .into_values()
                .map(|ug| ug.as_ref()),
        );
        self.0.insert(min, entry);
        result
    }

    pub fn remove_ungrounded(&mut self, min: Duration, max: Duration) -> bool {
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

    pub fn range(&self, range: impl RangeBounds<Duration>) -> Vec<MaybeGrounded<'o, R, M>> {
        let start_time = match range.start_bound() {
            Bound::Included(start) | Bound::Excluded(start) => Some(*start),
            _ => None,
        };
        let mut result = Vec::new();
        let mut ungrounded_collector = TimelineEntry::new_empty();
        for (t, e) in self.0.range(range) {
            ungrounded_collector.merge(e);
            if let Some(gr) = ungrounded_collector.grounded.take() {
                result.push(MaybeGrounded::Grounded(*t, gr));
            }
        }

        if let Some(t) = start_time {
            if result.is_empty()
                || matches!(result[0], MaybeGrounded::Grounded(first_ground_time, _) if first_ground_time > t)
            {
                let mut below_range = self.0.range(..t);
                loop {
                    let (early_entry_time, e) = below_range.next_back()
                        .expect("Cannot find operations to cover the beginning of view range. Did you request before the initial conditions?");
                    let mut found = e.ungrounded.keys().any(|end_time| *end_time <= t);
                    ungrounded_collector.merge(e);
                    if let Some(gr) = ungrounded_collector.grounded.take() {
                        result.push(MaybeGrounded::Grounded(*early_entry_time, gr));
                        found = true;
                    }
                    if found {
                        break;
                    }
                }
            }
        }

        result.extend(
            ungrounded_collector
                .ungrounded
                .into_values()
                .map(|ug| MaybeGrounded::Ungrounded(ug)),
        );
        result
    }
}

impl<'o, R: Resource<'o>, M: Model<'o>> ErasedResource<'o> for Timeline<'o, R, M> {
    fn id(&self) -> u64 {
        R::ID
    }
}

pub enum MaybeGrounded<'o, R: Resource<'o>, M: Model<'o>> {
    Grounded(Duration, &'o dyn Upstream<'o, R, M>),
    Ungrounded(&'o dyn UngroundedUpstream<'o, R, M>),
}
