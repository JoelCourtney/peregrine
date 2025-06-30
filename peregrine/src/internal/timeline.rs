#![doc(hidden)]

use crate::internal::history::PassThroughHashBuilder;
use crate::internal::operation::grounding::UngroundedUpstreamResolver;
use crate::internal::operation::initial_conditions::InitialConditionOp;
use crate::internal::operation::{Node, Upstream, UpstreamVec};
use crate::internal::placement::Placement;
use crate::internal::resource::ErasedResource;
use crate::public::resource::Resource;
use bumpalo_herd::{Herd, Member};
use hifitime::TimeScale::TAI;
use hifitime::{Duration, Epoch as Time};
use immutable_chunkmap::map::MapM;
use parking_lot::{
    MappedRwLockReadGuard, MappedRwLockWriteGuard, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use slab::Slab;
use smallvec::SmallVec;
use std::collections::{BTreeMap, HashMap};
use std::ops::{Bound, RangeBounds};

pub struct Timelines<'o> {
    map: HashMap<u64, RwLock<Box<dyn ErasedTimeline + 'o>>, PassThroughHashBuilder>,
    herd: &'o Herd,
    reactive_daemons: HashMap<u64, ReactiveDaemon<'o>>,
}

pub struct ReactiveDaemon<'o> {
    triggers: Vec<u64>,
    #[allow(unused_parens)]
    trigger_fn: Box<dyn Fn(Placement<'o>, Member<'o>) -> Vec<&'o dyn Node<'o>> + Sync>,
    #[allow(clippy::type_complexity)]
    record: Mutex<HashMap<(Duration, Option<Duration>), &'o dyn Node<'o>>>,
}

impl<'o> ReactiveDaemon<'o> {
    #[allow(unused_parens)]
    pub fn new(
        triggers: Vec<u64>,
        trigger_fn: Box<dyn Fn(Placement<'o>, Member<'o>) -> Vec<&'o dyn Node<'o>> + Sync>,
    ) -> Self {
        Self {
            triggers,
            trigger_fn,
            record: Mutex::new(HashMap::new()),
        }
    }
}
impl<'o> Timelines<'o> {
    pub fn new(herd: &'o Herd) -> Self {
        Self {
            map: HashMap::with_hasher(PassThroughHashBuilder),
            herd,
            reactive_daemons: HashMap::new(),
        }
    }

    pub fn init_for_resource<R: Resource>(
        &mut self,
        time: Duration,
        op: InitialConditionOp<'o, R>,
    ) {
        assert!(!self.map.contains_key(&R::ID));
        self.map.insert(
            R::ID,
            RwLock::new(Box::new(Timeline::init(time, self.herd.get().alloc(op)))),
        );
    }

    pub fn contains_resource<R: Resource>(&self) -> bool {
        self.map.contains_key(&R::ID)
    }

    pub fn find_upstream<R: Resource>(&self, time: Duration) -> &'o dyn Upstream<'o, R> {
        let mut inner = self.inner_timeline::<R>();
        if inner.should_flush() {
            drop(inner);
            let mut inner_mut = self.inner_timeline_mut::<R>();
            inner_mut.flush();
            drop(inner_mut);
            inner = self.inner_timeline();
        }
        inner.last_before(time, self.herd.get())
    }

    pub fn insert<R: Resource>(
        &self,
        placement: Placement<'o>,
        op: &'o dyn Upstream<'o, R>,
        is_daemon: bool,
    ) -> UpstreamVec<'o, R> {
        let (result, times) = match placement {
            Placement::Static(time) => (
                self.inner_timeline_mut().insert_grounded(time, op),
                (time, None),
            ),
            Placement::Dynamic { min, max, .. } => (
                self.inner_timeline_mut().insert_ungrounded(min, max, op),
                (min, Some(max)),
            ),
        };
        if !is_daemon {
            for trigger in self.reactive_daemons.values() {
                if trigger.triggers.contains(&R::ID) {
                    let mut record = trigger.record.lock();
                    if !record.contains_key(&times) {
                        let nodes = (trigger.trigger_fn)(placement, self.herd.get());
                        for node in nodes {
                            record.insert(times, node);
                            node.insert_self(self, true)
                                .expect("Failed to insert daemon trigger");
                        }
                    }
                }
            }
        }
        result
    }

    pub fn remove<R: Resource + 'o>(&self, placement: Placement<'o>, is_daemon: bool) -> bool {
        let (result, times) = match placement {
            Placement::Static(time) => (
                self.inner_timeline_mut::<R>().remove_grounded(time),
                (time, None),
            ),
            Placement::Dynamic { min, max, .. } => (
                self.inner_timeline_mut::<R>().remove_ungrounded(min, max),
                (min, Some(max)),
            ),
        };
        if !is_daemon {
            for trigger in self.reactive_daemons.values() {
                if trigger.triggers.contains(&R::ID) {
                    let mut record = trigger.record.lock();
                    if record.contains_key(&times) {
                        let node = record.remove(&times).unwrap();
                        node.remove_self(self, true)
                            .expect("Failed to remove daemon trigger");
                    }
                }
            }
        }
        result
    }

    pub(crate) fn range<R: Resource>(
        &self,
        bounds: impl RangeBounds<Duration> + Clone,
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
            .map
            .get(&R::ID)
            .unwrap_or_else(|| {
                panic!(
                    "Could not find resource {}. Is it included in the model?",
                    R::LABEL
                )
            })
            .read();
        RwLockReadGuard::map(reference, |r| unsafe {
            &*(r.as_ref() as *const dyn ErasedTimeline as *const Timeline<'o, R>)
        })
    }

    fn inner_timeline_mut<R: Resource>(&self) -> MappedRwLockWriteGuard<Timeline<'o, R>> {
        let reference = self
            .map
            .get(&R::ID)
            .unwrap_or_else(|| {
                panic!(
                    "Could not find resource {}. Is it included in the model?",
                    R::LABEL
                )
            })
            .write();
        RwLockWriteGuard::map(reference, |r| unsafe {
            &mut *(r.as_mut() as *mut dyn ErasedTimeline as *mut Timeline<'o, R>)
        })
    }

    pub fn add_reactive_daemon(&mut self, id: u64, trigger: ReactiveDaemon<'o>) {
        self.reactive_daemons.insert(id, trigger);
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

/// Represents a range where ungrounded upstreams are active
#[derive(Clone)]
pub struct ActiveUngroundedRanges<'o, R: Resource>(
    /// Map of durations to ungrounded upstream references for intervals active during this entry
    /// The duration key refers to when those intervals end
    BTreeMap<Duration, &'o dyn Upstream<'o, R>>,
);

impl<R: Resource> ActiveUngroundedRanges<'_, R> {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }
}

impl<R: Resource> Default for ActiveUngroundedRanges<'_, R> {
    fn default() -> Self {
        Self::new()
    }
}

// Helper function to find overlapping upstreams for a given ungrounded entry
fn find_overlapping_upstreams<'o, R: Resource>(
    ungrounded_map: &BTreeMap<Duration, ActiveUngroundedRanges<'o, R>>,
    min: Duration,
) -> (Duration, Duration, Vec<&'o dyn Upstream<'o, R>>) {
    let mut overlapping_upstreams = Vec::new();
    // Find the last upstream that ends before the insertion start
    let mut target_upstream = None;
    let mut start = Duration::ZERO;
    let mut end = Duration::ZERO;
    for (_, entry) in ungrounded_map.range(..min).rev() {
        if let Some((e, upstream)) = entry.0.range(..min).next_back() {
            target_upstream = Some(*upstream);
            end = *e;
            break;
        }
    }
    if let Some(target_ptr) = target_upstream {
        // Iterate backward through ungrounded map to find the interval
        for (start_time, entry) in ungrounded_map.range(..=min).rev() {
            // Check if the target upstream is still present in this entry
            let mut found = false;
            for (_, upstream) in entry.0.range(..) {
                if std::ptr::eq(*upstream, target_ptr) {
                    found = true;
                    break;
                }
            }
            if found {
                start = *start_time;
                // Collect all upstreams in this entry
                for (_, upstream) in entry.0.range(..) {
                    overlapping_upstreams.push(*upstream);
                }
            } else {
                break;
            }
        }
    }
    (start, end, overlapping_upstreams)
}

pub struct PossibleUpstreams<'o, R: Resource> {
    pub grounded: Option<(Duration, &'o dyn Upstream<'o, R>)>,
    pub ungrounded: UpstreamVec<'o, R>,
}

impl<'o, R: Resource> PossibleUpstreams<'o, R> {
    pub fn into_upstream_vec(self) -> UpstreamVec<'o, R> {
        let mut result = UpstreamVec::new();
        if let Some((_, gr)) = self.grounded {
            result.push(gr);
        }
        result.extend(self.ungrounded);
        // Deduplicate and sort by pointer address
        result.sort_by(|a, b| {
            let a_ptr = *a as *const _ as *const u8;
            let b_ptr = *b as *const _ as *const u8;
            a_ptr.cmp(&b_ptr)
        });
        result.dedup_by(|a, b| std::ptr::eq(*a, *b));
        result
    }

    pub fn into_single_upstream(self, time: Duration, bump: Member<'o>) -> &'o dyn Upstream<'o, R> {
        if self.ungrounded.is_empty() {
            self.grounded.expect("Set of possible upstreams is empty").1
        } else if self.grounded.is_none() && self.ungrounded.len() == 1 {
            self.ungrounded[0]
        } else {
            bump.alloc(UngroundedUpstreamResolver::new(
                time,
                self.grounded,
                self.ungrounded,
            ))
        }
    }
}

pub struct Timeline<'o, R: Resource> {
    /// Immutable chunk map of grounded upstream references
    grounded_map: MapM<Duration, &'o dyn Upstream<'o, R>>,
    /// Buffer of grounded upstreams that haven't been inserted yet
    grounded_buffer: Slab<(Duration, &'o dyn Upstream<'o, R>)>,
    /// Map of start durations to active ungrounded ranges
    ungrounded_map: BTreeMap<Duration, ActiveUngroundedRanges<'o, R>>,
}

impl<'o, R: Resource> Timeline<'o, R> {
    pub fn init(time: Duration, initial_condition: &'o dyn Upstream<'o, R>) -> Timeline<'o, R> {
        let mut map = MapM::new();
        map.insert_cow(time, initial_condition);
        Timeline {
            grounded_map: map,
            grounded_buffer: Slab::new(),
            ungrounded_map: BTreeMap::new(),
        }
    }

    fn search_possible_upstreams(&self, time: Duration) -> PossibleUpstreams<'o, R> {
        let mut ungrounded: SmallVec<&'o dyn Upstream<'o, R>, 2> = SmallVec::new();

        let mut grounded = Some(
            self.grounded_map
                .range(..time)
                .next_back()
                .map(|t| (*t.0, *t.1))
                .expect("No initial condition found"),
        );

        // All ungrounded operations that straddle the requested time
        for (_, entry) in self.ungrounded_map.range(..time) {
            for (_, upstream) in entry.0.range(time..) {
                // This upstream is active at 'time'
                ungrounded.push(*upstream);
            }
        }

        // The last ungrounded operation that ends before the requested time and all others that overlap with it
        let (start, end, overlapping) = find_overlapping_upstreams(&self.ungrounded_map, time);
        if !overlapping.is_empty() && start > grounded.as_ref().unwrap().0 {
            grounded = None;
        }
        if grounded.as_ref().map(|g| end > g.0).unwrap_or(true) {
            ungrounded.extend(overlapping);
        }

        // Deduplicate and sort by pointer address
        ungrounded.sort_by(|a, b| {
            let a_ptr = *a as *const _ as *const u8;
            let b_ptr = *b as *const _ as *const u8;
            a_ptr.cmp(&b_ptr)
        });
        ungrounded.dedup_by(|a, b| std::ptr::eq(*a, *b));

        PossibleUpstreams {
            grounded,
            ungrounded,
        }
    }

    pub fn last_before(&self, eval_time: Duration, bump: Member<'o>) -> &'o dyn Upstream<'o, R> {
        let possible = self.search_possible_upstreams(eval_time);
        possible.into_single_upstream(eval_time, bump)
    }

    pub fn insert_grounded(
        &mut self,
        time: Duration,
        value: &'o dyn Upstream<'o, R>,
    ) -> UpstreamVec<'o, R> {
        self.grounded_buffer.insert((time, value));
        self.search_possible_upstreams(time).into_upstream_vec()
    }

    pub fn remove_grounded(&mut self, time: Duration) -> bool {
        self.flush();
        self.grounded_map.remove_cow(&time).is_some()
    }

    pub fn insert_ungrounded(
        &mut self,
        min: Duration,
        max: Duration,
        value: &'o dyn Upstream<'o, R>,
    ) -> UpstreamVec<'o, R> {
        let mut result = UpstreamVec::new();

        // Find the previous entry before the insertion start time to get ongoing upstreams
        let mut ongoing_upstreams = BTreeMap::new();
        if let Some((_, prev_entry)) = self.ungrounded_map.range(..min).next_back() {
            // Filter ongoing upstreams to only include those that end after the insertion start
            for (end_time, upstream) in prev_entry.0.range(min..) {
                ongoing_upstreams.insert(*end_time, *upstream);
                // 1st: Add ungrounded upstreams that overlap with the insertion interval
                result.push(*upstream);
            }
        }

        // Create the new active ungrounded ranges entry
        let mut new_entry = ActiveUngroundedRanges::new();
        new_entry.0 = ongoing_upstreams;
        // Add the start upstream to the map
        new_entry.0.insert(max, value);

        // Insert the new entry at the start time
        self.ungrounded_map.insert(min, new_entry);

        // Update all entries within the insertion range to include the new upstream
        for (_, entry) in self.ungrounded_map.range_mut(min..max) {
            entry.0.insert(max, value);
            // 1st: Add ungrounded upstreams that overlap with the insertion interval
            for (_, upstream) in entry.0.range(..) {
                result.push(*upstream);
            }
        }

        // 2nd: Add all grounded upstreams that occurred during the insertion interval
        for (_, upstream) in self.grounded_map.range(min..max) {
            result.push(*upstream);
        }

        // 3rd: Find the last upstream before the insertion interval
        let Some((grounded_time, grounded_upstream)) = self.grounded_map.range(..min).next_back()
        else {
            unreachable!()
        };

        let (start, end, overlapping_upstreams) =
            find_overlapping_upstreams(&self.ungrounded_map, min);

        if start < *grounded_time {
            result.push(*grounded_upstream);
        }

        if end > *grounded_time {
            result.extend(overlapping_upstreams);
        }

        // Sort by pointer address and remove duplicates
        result.sort_by(|a, b| {
            let a_ptr = *a as *const _ as *const u8;
            let b_ptr = *b as *const _ as *const u8;
            a_ptr.cmp(&b_ptr)
        });
        result.dedup_by(|a, b| std::ptr::eq(*a, *b));

        result
    }

    pub fn remove_ungrounded(&mut self, min: Duration, max: Duration) -> bool {
        // Remove the entry at min if it exists
        let entry_removed = self.ungrounded_map.remove(&min).is_some();

        if entry_removed {
            // For each entry in the interval, remove the ongoing upstream that ends at max
            for (_, entry) in self.ungrounded_map.range_mut(min..max) {
                entry.0.remove(&max);
            }
        }

        entry_removed
    }

    pub fn range(&self, range: impl RangeBounds<Duration> + Clone) -> Vec<MaybeGrounded<'o, R>> {
        let start_time = match range.start_bound() {
            Bound::Included(start) | Bound::Excluded(start) => Some(*start),
            _ => None,
        };
        let mut result = Vec::new();

        // Collect grounded upstreams from the grounded map
        for (t, upstream) in self.grounded_map.range(range.clone()) {
            result.push(MaybeGrounded::Grounded(*t, *upstream));
        }

        // Handle the case where we need to look before the range start
        if let Some(t) = start_time {
            if result.is_empty() {
                let mut below_range = self.grounded_map.range(..t);
                if let Some((early_entry_time, upstream)) = below_range.next_back() {
                    result.push(MaybeGrounded::Grounded(*early_entry_time, *upstream));
                }
            }
        }

        // Collect ungrounded upstreams from active ungrounded range entries
        let mut ungrounded_upstreams = Vec::new();

        // Get all active ungrounded range entries that happen during the requested range
        for (_, entry) in self.ungrounded_map.range(range) {
            ungrounded_upstreams.extend(entry.0.values().copied());
        }

        // Get the last entry to happen before the range
        if let Some(start_time) = start_time {
            if let Some((_, last_entry)) = self.ungrounded_map.range(..start_time).next_back() {
                ungrounded_upstreams.extend(last_entry.0.range(..).map(|(_, upstream)| *upstream));
            }
        }

        // Deduplicate ungrounded upstreams using pointer equality
        ungrounded_upstreams.sort_by(|a, b| {
            let a_ptr = *a as *const _ as *const u8;
            let b_ptr = *b as *const _ as *const u8;
            a_ptr.cmp(&b_ptr)
        });
        ungrounded_upstreams.dedup_by(|a, b| std::ptr::eq(*a, *b));

        // Add deduplicated ungrounded upstreams to the result
        result.extend(
            ungrounded_upstreams
                .into_iter()
                .map(|upstream| MaybeGrounded::Ungrounded(upstream)),
        );
        result
    }
}

impl<R: Resource> ErasedTimeline for Timeline<'_, R> {
    fn should_flush(&self) -> bool {
        !self.grounded_buffer.is_empty()
    }
    fn flush(&mut self) {
        if self.should_flush() {
            self.grounded_map = self.grounded_map.insert_many(self.grounded_buffer.drain());
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
    Ungrounded(&'o dyn Upstream<'o, R>),
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::internal::exec::ExecEnvironment;
    use crate::internal::operation::{Continuation, Downstream, Node, Upstream};
    use bumpalo_herd::Herd;
    use hifitime::Duration;
    use once_cell::sync::Lazy;
    use oneshot::channel;
    use rayon::Scope;

    #[allow(unused_imports)]
    use crate as peregrine;

    peregrine::resource! {
        dummy: u32;
    }

    // Minimal Upstream implementation for testing
    struct DummyUpstream {
        id: u32,
    }
    impl DummyUpstream {
        pub fn new_alloc<'o>(herd: &'o Herd, id: u32) -> &'o dyn Upstream<'o, dummy> {
            herd.get().alloc(Self { id })
        }
    }
    impl<'o> Node<'o> for DummyUpstream {
        fn insert_self(
            &'o self,
            _timelines: &Timelines<'o>,
            _is_daemon: bool,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        fn remove_self(&self, _timelines: &Timelines<'o>, _is_daemon: bool) -> anyhow::Result<()> {
            Ok(())
        }
    }
    impl<'o> Upstream<'o, dummy> for DummyUpstream {
        fn request<'s>(
            &'o self,
            continuation: Continuation<'o, dummy>,
            _already_registered: bool,
            _scope: &Scope<'s>,
            _timelines: &'s Timelines<'o>,
            _env: ExecEnvironment<'s, 'o>,
        ) where
            'o: 's,
        {
            // Return the id as the value
            continuation.run(Ok((self.id as u64, self.id)), _scope, _timelines, _env);
        }
        fn notify_downstreams(&self, _time_of_change: Duration) {}
        fn register_downstream_early(&self, _downstream: &'o dyn Downstream<'o, dummy>) {}
        fn request_grounding<'s>(
            &'o self,
            _continuation: crate::internal::operation::grounding::GroundingContinuation<'o>,
            _already_registered: bool,
            _scope: &Scope<'s>,
            _timelines: &'s Timelines<'o>,
            _env: ExecEnvironment<'s, 'o>,
        ) where
            'o: 's,
        {
        }
    }

    macro_rules! dummy_timeline {
        ($herd:ident, $($id:ident $pattern:tt),* $(,)?) => {{
            let mut timeline = Timeline::<dummy>::init(
                hifitime::Duration::from_seconds(0.0),
                DummyUpstream::new_alloc(&$herd, 0)
            );
            $(
                dummy_timeline!(@parse $id $pattern, timeline, $herd);
            )*
            timeline.flush();
            timeline
        }};
        (@parse grounded($time:expr, $id:expr), $timeline:ident, $herd:ident) => {
            $timeline.insert_grounded(
                hifitime::Duration::from_seconds($time),
                DummyUpstream::new_alloc(&$herd, $id)
            );
        };
        (@parse ungrounded($start:expr, $end:expr, $id:expr), $timeline:ident, $herd:ident) => {
            $timeline.insert_ungrounded(
                hifitime::Duration::from_seconds($start),
                hifitime::Duration::from_seconds($end),
                DummyUpstream::new_alloc(&$herd, $id)
            );
        };
    }

    static HISTORY: Lazy<crate::internal::history::History> =
        Lazy::new(crate::internal::history::History::default);
    static ERRORS: Lazy<crate::internal::exec::ErrorAccumulator> =
        Lazy::new(crate::internal::exec::ErrorAccumulator::default);

    fn get_id<'o>(up: &'o dyn Upstream<'o, dummy>, herd: &'o Herd) -> u32 {
        let (tx, rx) = channel();
        // SAFETY: We never use the scope in DummyUpstream::request, so this is fine for the test.
        let timelines = Timelines::new(herd);
        let env = crate::internal::exec::ExecEnvironment {
            history: &HISTORY,
            errors: &ERRORS,
            stack_counter: 0,
        };
        rayon::scope(|scope| {
            up.request(Continuation::Root(tx), false, scope, &timelines, env);
        });
        rx.recv().unwrap().unwrap()
    }

    #[test]
    fn test_nonzero_dummy_size() {
        assert!(std::mem::size_of::<DummyUpstream>() > 0);
    }

    #[test]
    fn test_insert_and_find_grounded() {
        let herd = Herd::new();
        let timeline = dummy_timeline!(herd, grounded(10.0, 1), grounded(20.0, 2));
        let found0 = timeline.last_before(Duration::from_seconds(0.1), herd.get());
        let found10 = timeline.last_before(Duration::from_seconds(10.1), herd.get());
        let found15 = timeline.last_before(Duration::from_seconds(15.0), herd.get());
        let found20 = timeline.last_before(Duration::from_seconds(20.1), herd.get());
        assert_eq!(get_id(found0, &herd), 0);
        assert_eq!(get_id(found10, &herd), 1);
        assert_eq!(get_id(found15, &herd), 1);
        assert_eq!(get_id(found20, &herd), 2);
    }

    #[test]
    fn test_insert_and_find_ungrounded() {
        let herd = Herd::new();
        let timeline = dummy_timeline!(herd, ungrounded(5.0, 15.0, 1), ungrounded(10.0, 20.0, 2));
        let ups7 = timeline.search_possible_upstreams(Duration::from_seconds(7.0));
        let ups17 = timeline.search_possible_upstreams(Duration::from_seconds(17.0));
        let ids7: HashSet<u32> = ups7
            .into_upstream_vec()
            .into_iter()
            .map(|up| get_id(up, &herd))
            .collect();
        let ids17: HashSet<u32> = ups17
            .into_upstream_vec()
            .into_iter()
            .map(|up| get_id(up, &herd))
            .collect();
        assert_eq!(ids7, HashSet::from([0, 1]));
        assert_eq!(ids17, HashSet::from([1, 2]));
    }

    #[test]
    fn test_grounded_and_ungrounded_overlap() {
        let herd = Herd::new();
        let timeline = dummy_timeline!(herd, grounded(5.0, 1), ungrounded(5.0, 15.0, 2));
        let ups5 = timeline.search_possible_upstreams(Duration::from_seconds(5.0));
        let ups10 = timeline.search_possible_upstreams(Duration::from_seconds(10.0));
        let ids5: HashSet<u32> = ups5
            .into_upstream_vec()
            .into_iter()
            .map(|up| get_id(up, &herd))
            .collect();
        let ids10: HashSet<u32> = ups10
            .into_upstream_vec()
            .into_iter()
            .map(|up| get_id(up, &herd))
            .collect();
        // The set of possible upstreams at these times can include any of the inserted ids
        assert_eq!(ids5, HashSet::from([0]));
        assert_eq!(ids10, HashSet::from([1, 2]));
    }

    #[test]
    fn test_remove_grounded() {
        let herd = Herd::new();
        let mut timeline = dummy_timeline!(herd, grounded(5.0, 1));
        assert!(timeline.remove_grounded(Duration::from_seconds(5.0)));
        let found5 = timeline.last_before(Duration::from_seconds(5.0), herd.get());
        assert_eq!(get_id(found5, &herd), 0);
    }

    #[test]
    fn test_remove_ungrounded() {
        let herd = Herd::new();
        let mut timeline = dummy_timeline!(herd, ungrounded(5.0, 15.0, 1));
        assert!(
            timeline.remove_ungrounded(Duration::from_seconds(5.0), Duration::from_seconds(15.0))
        );
        let found10 = timeline.last_before(Duration::from_seconds(10.0), herd.get());
        assert_eq!(get_id(found10, &herd), 0);
    }

    #[test]
    fn test_adjacent_ungrounded_intervals() {
        let herd = Herd::new();
        let timeline = dummy_timeline!(herd, ungrounded(5.0, 10.0, 1), ungrounded(10.0, 15.0, 2));
        let ups7 = timeline.search_possible_upstreams(Duration::from_seconds(7.0));
        let ups12 = timeline.search_possible_upstreams(Duration::from_seconds(12.0));
        let ids7: Vec<u32> = ups7
            .ungrounded
            .iter()
            .map(|up| get_id(*up, &herd))
            .collect();
        let ids12: Vec<u32> = ups12
            .ungrounded
            .iter()
            .map(|up| get_id(*up, &herd))
            .collect();
        assert!(ids7.contains(&1));
        assert!(ids12.contains(&2));
    }
}
