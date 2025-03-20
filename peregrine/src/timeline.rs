#![doc(hidden)]

use crate::history::PassThroughHashBuilder;
use crate::macro_prelude::UngroundedUpstream;
use crate::operation::grounding::UngroundedUpstreamResolver;
use crate::operation::initial_conditions::InitialConditionOp;
use crate::operation::{Upstream, UpstreamVec};
use crate::resource::{ErasedResource, Resource};
use bumpalo_herd::{Herd, Member};
use hifitime::TimeScale::TAI;
use hifitime::{Duration, Epoch as Time};
use immutable_chunkmap::map::{MapM, MapS};
use parking_lot::{
    MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use slab::Slab;
use std::collections::HashMap;
use std::ops::{Bound, RangeBounds};

pub struct Timelines<'o>(
    HashMap<u64, RwLock<Box<dyn ErasedTimeline + 'o>>, PassThroughHashBuilder>,
    &'o Herd,
);

impl<'o> Timelines<'o> {
    pub fn new(herd: &'o Herd) -> Self {
        Self(HashMap::with_hasher(PassThroughHashBuilder), herd)
    }

    pub fn init_for_resource<R: Resource>(
        &mut self,
        time: Duration,
        op: InitialConditionOp<'o, R>,
    ) {
        assert!(!self.0.contains_key(&R::ID));
        self.0.insert(
            R::ID,
            RwLock::new(Box::new(Timeline::init(time, self.1.get().alloc(op)))),
        );
    }

    pub fn contains_resource<R: Resource>(&self) -> bool {
        self.0.contains_key(&R::ID)
    }

    pub fn find_upstream<R: Resource>(&self, time: Duration) -> Option<&'o dyn Upstream<'o, R>> {
        let mut inner = self.inner_timeline::<R>();
        if inner.should_flush() {
            drop(inner);
            let mut inner_mut = self.inner_timeline_mut::<R>();
            inner_mut.flush();
            drop(inner_mut);
            inner = self.inner_timeline();
        }
        inner.last_before(time, self.1.get())
    }

    pub fn insert_grounded<R: Resource>(
        &self,
        time: Duration,
        op: &'o dyn Upstream<'o, R>,
    ) -> UpstreamVec<'o, R> {
        self.inner_timeline_mut().insert_grounded(time, op)
    }
    pub fn remove_grounded<R: Resource + 'o>(&self, time: Duration) -> bool {
        self.inner_timeline_mut::<R>().remove_grounded(time)
    }

    pub fn insert_ungrounded<R: Resource>(
        &self,
        min: Duration,
        max: Duration,
        op: &'o dyn UngroundedUpstream<'o, R>,
    ) -> UpstreamVec<'o, R> {
        self.inner_timeline_mut().insert_ungrounded(min, max, op)
    }

    pub fn remove_ungrounded<R: Resource + 'o>(&self, min: Duration, max: Duration) -> bool {
        self.inner_timeline_mut::<R>().remove_ungrounded(min, max)
    }

    pub(crate) fn range<R: Resource>(
        &self,
        bounds: impl RangeBounds<Duration>,
    ) -> Vec<MaybeGrounded<'o, R>> {
        let mut inner = self.inner_timeline::<R>();
        if inner.should_flush() {
            drop(inner);
            let mut inner_mut = self.inner_timeline_mut::<R>();
            inner_mut.flush();
            drop(inner_mut);
            inner = self.inner_timeline();
        }
        inner.range(bounds)
    }

    fn inner_timeline<R: Resource>(&self) -> MappedRwLockReadGuard<Timeline<'o, R>> {
        let reference = self
            .0
            .get(&R::ID)
            .unwrap_or_else(|| {
                panic!(
                    "Could not find resource {}. Is it included in the model?",
                    R::LABEL
                )
            })
            .read();
        RwLockReadGuard::map(reference, |r| {
            let transmuted =
                unsafe { &*(r.as_ref() as *const dyn ErasedTimeline as *const Timeline<'o, R>) };
            transmuted
        })
    }

    fn inner_timeline_mut<R: Resource>(&self) -> MappedRwLockWriteGuard<Timeline<'o, R>> {
        let reference = self
            .0
            .get(&R::ID)
            .unwrap_or_else(|| {
                panic!(
                    "Could not find resource {}. Is it included in the model?",
                    R::LABEL
                )
            })
            .write();
        RwLockWriteGuard::map(reference, |r| {
            let transmuted =
                unsafe { &mut *(r.as_mut() as *mut dyn ErasedTimeline as *mut Timeline<'o, R>) };
            transmuted
        })
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
pub const fn duration_to_epoch(duration: Duration) -> Time {
    Time {
        duration,
        time_scale: TAI,
    }
}

pub struct Timeline<'o, R: Resource>(
    MapM<Duration, TimelineEntry<'o, R>>,
    Slab<(Duration, &'o dyn Upstream<'o, R>)>,
);

#[derive(Clone)]
pub struct TimelineEntry<'o, R: Resource> {
    pub grounded: Option<&'o dyn Upstream<'o, R>>,
    pub ungrounded: MapS<Duration, &'o dyn UngroundedUpstream<'o, R>>,
}

impl<'o, R: Resource> TimelineEntry<'o, R> {
    fn new_empty() -> Self {
        TimelineEntry {
            grounded: None,
            ungrounded: MapS::new(),
        }
    }

    fn new_grounded(gr: &'o dyn Upstream<'o, R>) -> Self {
        TimelineEntry {
            grounded: Some(gr),
            ungrounded: MapS::new(),
        }
    }

    fn _new_ungrounded(ug: &'o dyn UngroundedUpstream<'o, R>, max: Duration) -> Self {
        let mut map = MapS::new();
        map.insert_cow(max, ug);
        TimelineEntry {
            grounded: None,
            ungrounded: map,
        }
    }

    fn merge(&mut self, other: &TimelineEntry<'o, R>) {
        assert_ne!(self.grounded.is_some(), other.grounded.is_some());

        self.grounded = self.grounded.take().or(other.grounded);
        if other.ungrounded.len() > 0 {
            self.ungrounded
                .insert_many(other.ungrounded.into_iter().map(|(d, n)| (*d, *n)));
        }
    }

    pub fn into_upstream(
        self,
        entry_time: Duration,
        eval_time: Duration,
        bump: Member<'o>,
    ) -> &'o dyn Upstream<'o, R> {
        if self.ungrounded.len() == 0 {
            self.grounded.unwrap()
        } else {
            bump.alloc(UngroundedUpstreamResolver::new(
                eval_time,
                self.grounded.map(|g| (entry_time, g)),
                self.ungrounded.into_iter().map(|(_, v)| *v).collect(),
            ))
        }
    }

    pub fn into_upstream_vec(self) -> UpstreamVec<'o, R> {
        let mut result = UpstreamVec::new();
        if let Some(gr) = self.grounded {
            result.push(gr);
        }
        if self.ungrounded.len() > 0 {
            result.extend(self.ungrounded.into_iter().map(|(_, ug)| (*ug).as_ref()))
        }
        result
    }
}

impl<'o, R: Resource> Timeline<'o, R> {
    pub fn init(time: Duration, initial_condition: &'o dyn Upstream<'o, R>) -> Timeline<'o, R> {
        let mut map = MapM::new();
        map.insert_cow(time, TimelineEntry::new_grounded(initial_condition));
        Timeline(map, Slab::new())
    }

    fn search_possible_upstreams(
        &self,
        time: Duration,
    ) -> Option<(Duration, TimelineEntry<'o, R>)> {
        let mut iter = self.0.range(..time);
        let elem = iter.next_back()?;
        let (mut entry_time, mut result) = (*elem.0, elem.1.clone());
        loop {
            if result.grounded.is_some()
                || result
                    .ungrounded
                    .into_iter()
                    .map(|(t, _)| t <= &time)
                    .next()
                    .unwrap_or(false)
            {
                break;
            }
            let elem = iter.next_back()?;
            result.merge(elem.1);
            entry_time = *elem.0;
        }

        Some((entry_time, result))
    }

    pub fn last_before(
        &self,
        eval_time: Duration,
        bump: Member<'o>,
    ) -> Option<&'o dyn Upstream<'o, R>> {
        let (entry_time, possible) = self.search_possible_upstreams(eval_time)?;
        Some(possible.into_upstream(entry_time, eval_time, bump))
    }

    pub fn insert_grounded(
        &mut self,
        time: Duration,
        value: &'o dyn Upstream<'o, R>,
    ) -> UpstreamVec<'o, R> {
        self.1.insert((time, value));
        self.search_possible_upstreams(time)
            .map(|e| e.1.into_upstream_vec())
            .unwrap_or_default()
    }

    pub fn remove_grounded(&mut self, time: Duration) -> bool {
        self.flush();
        self.0.remove_cow(&time).is_some()
    }

    pub fn insert_ungrounded(
        &mut self,
        _min: Duration,
        _max: Duration,
        _value: &'o dyn UngroundedUpstream<'o, R>,
    ) -> UpstreamVec<'o, R> {
        todo!()
        // let mut entry = TimelineEntry::new_ungrounded(value, max);
        // entry.ungrounded.extend(
        //     self.0
        //         .range(..min)
        //         .next_back()
        //         .map(|(_, entry)| entry.ungrounded.range((Excluded(min), Unbounded)))
        //         .unwrap_or_default(),
        // );

        // // Need to collect the list of all nodes that might lose a downstream after this change
        // let mut result = UpstreamVec::new();
        // let mut ungrounded_collector = TimelineEntry::new_empty();
        // for (_, e) in self.0.range_mut(min..max) {
        //     ungrounded_collector.merge(e);
        //     if let Some(gr) = ungrounded_collector.grounded.take() {
        //         result.push(gr);
        //     }

        //     e.ungrounded.insert(max, value);
        // }

        // result.extend(
        //     ungrounded_collector
        //         .ungrounded
        //         .into_values()
        //         .map(|ug| ug.as_ref()),
        // );
        // self.0.insert(min, entry);
        // result
    }

    pub fn remove_ungrounded(&mut self, _min: Duration, _max: Duration) -> bool {
        todo!()
        // let entry = self.0.remove(&min);
        // if entry.is_some() {
        //     for (_, e) in self.0.range_mut(min..max) {
        //         e.ungrounded.remove(&max);
        //     }
        //     true
        // } else {
        //     false
        // }
    }

    pub fn range(&self, range: impl RangeBounds<Duration>) -> Vec<MaybeGrounded<'o, R>> {
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
                    let mut found = e.ungrounded.into_iter().any(|(end_time, _)| *end_time <= t);
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
                .into_iter()
                .map(|(_, ug)| MaybeGrounded::Ungrounded(*ug)),
        );
        result
    }
}

impl<R: Resource> ErasedTimeline for Timeline<'_, R> {
    fn should_flush(&self) -> bool {
        !self.1.is_empty()
    }
    fn flush(&mut self) {
        if self.should_flush() {
            self.0 = self.0.insert_many(
                self.1
                    .drain()
                    .map(|(t, v)| (t, TimelineEntry::new_grounded(v))),
            );
        }
    }
}

trait ErasedTimeline: ErasedResource {
    fn should_flush(&self) -> bool;
    fn flush(&mut self);
}

impl<R: Resource> ErasedResource for Timeline<'_, R> {
    fn id(&self) -> u64 {
        R::ID
    }
}

pub enum MaybeGrounded<'o, R: Resource> {
    Grounded(Duration, &'o dyn Upstream<'o, R>),
    Ungrounded(&'o dyn UngroundedUpstream<'o, R>),
}
