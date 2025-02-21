#![doc(hidden)]

use crate::exec::{BumpedFuture, ExecEnvironment};
use crate::history::{History, PeregrineDefaultHashBuilder};
use crate::timeline::HasTimeline;
use crate::{Model, Resource};
use hifitime::Duration;
use std::hash::BuildHasher;
use std::pin::Pin;
use std::sync::Mutex;
use tokio::sync::{RwLock, RwLockReadGuard};

pub trait Operation<'o, M: Model<'o>>: Sync {
    fn find_children(&'o self, time_of_change: Duration, timelines: &M::Timelines);
    fn add_parent(&self, parent: &'o dyn Operation<'o, M>);
    fn remove_parent(&self, parent: &dyn Operation<'o, M>);

    fn insert_self(&'o self, timelines: &mut M::Timelines);
    fn remove_self(&self, timelines: &mut M::Timelines);

    fn parents(&self) -> Vec<&'o dyn Operation<'o, M>>;
    fn notify_parents(&self, time_of_change: Duration, timelines: &M::Timelines);
    fn clear_cache(&self) -> bool;
}

pub trait Writer<'o, R: Resource<'o>, M: Model<'o>>: Operation<'o, M> {
    fn read<'b>(
        &'o self,
        histories: &'o History,
        env: ExecEnvironment<'b>,
    ) -> BumpedFuture<'b, (u64, RwLockReadGuard<'o, <R as Resource<'o>>::Read>)>
    where
        'o: 'b;
}

pub struct InitialConditionOpInner<'o, R: Resource<'o>> {
    value: <R as Resource<'o>>::Write,
    result: Option<(u64, <R as Resource<'o>>::Read)>,
}

pub struct InitialConditionOp<'o, R: Resource<'o>, M: Model<'o>>
where
    M::Timelines: HasTimeline<'o, R, M>,
{
    lock: RwLock<InitialConditionOpInner<'o, R>>,
    parents: Mutex<Vec<&'o dyn Operation<'o, M>>>,
    time: Duration,
}

impl<'o, R: Resource<'o>, M: Model<'o>> InitialConditionOp<'o, R, M>
where
    M::Timelines: HasTimeline<'o, R, M>,
{
    pub fn new(time: Duration, value: <R as Resource<'o>>::Write) -> Self {
        Self {
            lock: RwLock::new(InitialConditionOpInner {
                value,
                result: None,
            }),
            parents: Mutex::new(vec![]),
            time,
        }
    }
}

impl<'o, R: Resource<'o>, M: Model<'o>> Operation<'o, M> for InitialConditionOp<'o, R, M>
where
    M::Timelines: HasTimeline<'o, R, M>,
{
    fn find_children(&'o self, _time_of_change: Duration, _timelines: &M::Timelines) {}

    fn add_parent(&self, parent: &'o dyn Operation<'o, M>) {
        self.parents.lock().unwrap().push(parent);
    }

    fn remove_parent(&self, parent: &dyn Operation<'o, M>) {
        self.parents
            .lock()
            .unwrap()
            .retain(|p| !std::ptr::eq(*p, parent));
    }

    fn insert_self(&'o self, timelines: &mut M::Timelines) {
        <M::Timelines as HasTimeline<'o, R, M>>::insert_operation(timelines, self.time, self);
    }

    fn remove_self(&self, timelines: &mut M::Timelines) {
        <M::Timelines as HasTimeline<'o, R, M>>::remove_operation(timelines, self.time);
    }

    fn parents(&self) -> Vec<&'o dyn Operation<'o, M>> {
        self.parents.lock().unwrap().clone()
    }

    fn notify_parents(&self, time_of_change: Duration, timelines: &M::Timelines) {
        for parent in self.parents.lock().unwrap().iter() {
            parent.find_children(time_of_change, timelines);
        }
    }

    fn clear_cache(&self) -> bool {
        false
    }
}

impl<'o, R: Resource<'o> + 'o, M: Model<'o>> Writer<'o, R, M> for InitialConditionOp<'o, R, M>
where
    M::Timelines: HasTimeline<'o, R, M>,
{
    fn read<'b>(
        &'o self,
        histories: &'o History,
        env: ExecEnvironment<'b>,
    ) -> BumpedFuture<'b, (u64, RwLockReadGuard<'o, <R as Resource<'o>>::Read>)>
    where
        'o: 'b,
    {
        unsafe {
            Pin::new_unchecked(env.bump.alloc(async move {
                let read_guard = if let Ok(mut write_guard) = self.lock.try_write() {
                    if write_guard.result.is_none() {
                        let hash = PeregrineDefaultHashBuilder::default().hash_one(
                            bincode::serde::encode_to_vec(
                                &write_guard.value,
                                bincode::config::standard(),
                            )
                            .unwrap(),
                        );
                        if let Some(r) = histories.get::<R>(hash) {
                            write_guard.result = Some((hash, r));
                        } else {
                            write_guard.result =
                                Some((hash, histories.insert::<R>(hash, write_guard.value.clone())));
                        }
                    }
                    write_guard.downgrade()
                } else {
                    self.lock.read().await
                };
                let hash = read_guard.result.unwrap().0;
                (
                    hash,
                    RwLockReadGuard::map(read_guard, |o| &o.result.as_ref().unwrap().1),
                )
            }))
        }
    }
}
