pub(crate) mod collector;

use std::{mem::take, sync::Arc};

use oneshot::{Receiver, Sender, channel};
use parking_lot::Mutex;
use replace_with::replace_with_or_abort;

use crate::Data;

#[derive(Default, Debug)]
enum DataState<T> {
    Constant(T),
    Variable {
        value: T,
        revalidated: bool,
    },
    Invalid(T),
    Working(Option<T>),
    #[default]
    Empty,
}

impl<T> DataState<T> {
    fn invalidate(&mut self) {
        match std::mem::take(self) {
            DataState::Variable { value: d, .. } | DataState::Invalid(d) => {
                *self = DataState::Invalid(d);
            }
            DataState::Constant(_) => unreachable!(),
            DataState::Working(_) => {
                panic!("Cannot invalidate a cache while the node is being executed.")
            }
            DataState::Empty => {}
        }
    }
}

type Invalidator = Box<dyn FnOnce() + Send + Sync>;

pub struct Cache<T> {
    result: Mutex<DataState<T>>,
    invalidators: Mutex<Vec<Receiver<Invalidator>>>,
}

#[expect(
    clippy::enum_variant_names,
    reason = "I think these variant names are both descriptive and funny, I'm not changing it"
)]
pub(crate) enum CheckResult<T> {
    NoProblem(Cached<T>),
    SomeoneElsesProblem,
    YourProblem { can_be_revalidated: bool },
}

impl<T: Data> Default for Cache<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Data> Cache<T> {
    pub fn new() -> Self {
        Self {
            result: Mutex::new(DataState::Empty),
            invalidators: Mutex::new(vec![]),
        }
    }

    #[must_use]
    pub fn new_arc() -> Arc<Self> {
        Arc::new(Self::new())
    }

    pub(crate) fn check(&self) -> CheckResult<T>
    where
        T: Clone,
    {
        match &mut *self.result.lock() {
            DataState::Constant(v) => CheckResult::NoProblem(Cached::Constant(v.clone())),
            DataState::Variable {
                value: v,
                revalidated,
            } => {
                let (send, recv) = channel();
                self.invalidators.lock().push(recv);
                CheckResult::NoProblem(Cached::variable(v.clone(), send, *revalidated))
            }
            DataState::Working(_) => CheckResult::SomeoneElsesProblem,
            ds @ DataState::Invalid(_) => {
                replace_with_or_abort(ds, |ds| match ds {
                    DataState::Invalid(v) => DataState::Working(Some(v)),
                    _ => unreachable!(),
                });
                CheckResult::YourProblem {
                    can_be_revalidated: true,
                }
            }
            DataState::Empty => CheckResult::YourProblem {
                can_be_revalidated: false,
            },
        }
    }

    pub(crate) fn resolve(
        self: &Arc<Self>,
        result: T,
        is_constant: bool,
    ) -> Box<dyn FnMut() -> Cached<T> + '_>
    where
        T: Clone,
    {
        let mut state = self.result.lock();
        if is_constant {
            *state = DataState::Constant(result.clone());
            Box::new(move || Cached::Constant(result.clone()))
        } else {
            let revalidated =
                matches!(take(&mut *state), DataState::Working(Some(v)) if v == result);
            *state = DataState::Variable {
                value: result.clone(),
                revalidated,
            };
            let mut invalidators = self.invalidators.lock();
            Box::new(move || {
                let (send, recv) = channel();
                invalidators.push(recv);
                Cached::variable(result.clone(), send, revalidated)
            })
        }
    }

    pub fn revalidate(&self) -> Box<dyn FnMut() -> Cached<T> + '_>
    where
        T: Clone,
    {
        let mut state = self.result.lock();
        match take(&mut *state) {
            DataState::Working(Some(v)) => {
                *state = DataState::Variable {
                    value: v.clone(),
                    revalidated: true,
                };
                let mut invalidators = self.invalidators.lock();
                Box::new(move || {
                    let (send, recv) = channel();
                    invalidators.push(recv);
                    Cached::variable(v.clone(), send, true)
                })
            }
            _ => unreachable!(),
        }
    }

    pub fn invalidate(&self) {
        self.result.lock().invalidate();
        for downstream in self.invalidators.lock().drain(..) {
            if let Ok(inv) = downstream.recv() {
                inv();
            }
        }
    }

    pub fn is_valid(&self) -> bool {
        matches!(
            &*self.result.lock(),
            DataState::Variable { .. } | DataState::Constant(_)
        )
    }

    pub fn get_invalidator_sender(&self) -> Sender<Invalidator> {
        let (send, recv) = channel();
        self.invalidators.lock().push(recv);
        send
    }

    pub fn get_invalidator(self: &Arc<Self>) -> impl FnOnce() + Send + use<T> {
        let weak = Arc::downgrade(self);
        move || {
            if let Some(c) = weak.upgrade() {
                c.invalidate();
            }
        }
    }
}

pub enum Cached<T> {
    Constant(T),
    Variable {
        value: T,
        senders: Vec<Sender<Invalidator>>,
        revalidated: bool,
    },
}

impl<T> Cached<T> {
    pub fn variable(value: T, sender: Sender<Invalidator>, revalidated: bool) -> Self {
        Cached::Variable {
            value,
            senders: vec![sender],
            revalidated,
        }
    }

    pub fn track(self, invalidator: impl FnOnce() + Clone + Send + Sync + 'static) -> T {
        match self {
            Cached::Constant(v) => v,
            Cached::Variable { value, senders, .. } => {
                for s in senders {
                    let i = invalidator.clone();
                    s.send(Box::new(i)).expect("Could not sent invalidator");
                }
                value
            }
        }
    }

    pub fn open(self) -> T {
        match self {
            Cached::Constant(v) => v,
            Cached::Variable { value, .. } => value,
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Cached<U> {
        match self {
            Cached::Constant(v) => Cached::Constant(f(v)),
            Cached::Variable {
                value,
                senders,
                revalidated,
            } => Cached::Variable {
                value: f(value),
                senders,
                revalidated,
            },
        }
    }

    pub fn push_sender(&mut self, sender: Sender<Invalidator>, revalidated: bool) {
        replace_with_or_abort(self, |c| match c {
            Cached::Constant(v) => Cached::Variable {
                value: v,
                senders: vec![sender],
                revalidated,
            },
            Cached::Variable {
                value,
                revalidated,
                mut senders,
                ..
            } => {
                senders.push(sender);
                Cached::Variable {
                    value,
                    senders,
                    revalidated,
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use peregrine_macros::op;

    use crate::{self as peregrine, run};
    use crate::{
        cache::{Cache, Cached},
        graph::variable::Var,
    };

    #[test]
    #[allow(unused_must_use)]
    fn test_cache_manual_invalidation() {
        let cache_a = Cache::<u32>::new_arc();
        cache_a.resolve(5, false);

        let cache_b = Cache::<String>::new_arc();

        let b = (cache_b.resolve(String::from("hello world"), false))();

        cache_a
            .get_invalidator_sender()
            .send(Box::new(cache_b.get_invalidator()))
            .unwrap();

        assert!(matches!(b, Cached::Variable { .. }));
        assert_eq!(b.open(), "hello world");

        assert!(cache_b.is_valid());

        cache_a.invalidate();
        assert!(!cache_b.is_valid());
    }

    #[test]
    fn revalidation() {
        let a_counter = &AtomicU32::new(0);
        let a = Var::new(op! {
            a_counter.fetch_add(1, Ordering::Relaxed);
            0
        });

        let b_counter = &AtomicU32::new(0);
        let b = op! {
            b_counter.fetch_add(1, Ordering::Relaxed);
            i!(&a) + 1
        };

        let c_counter = &AtomicU32::new(0);
        let c = op! {
            c_counter.fetch_add(1, Ordering::Relaxed);
            i!(&b) + 1
        };

        assert_eq!(run(&c), 2);
        assert_eq!(a_counter.load(Ordering::Relaxed), 1);
        assert_eq!(b_counter.load(Ordering::Relaxed), 1);
        assert_eq!(c_counter.load(Ordering::Relaxed), 1);

        a.set(op! {
            a_counter.fetch_add(1, Ordering::Relaxed);
            10
        });

        assert_eq!(run(&c), 12);
        assert_eq!(a_counter.load(Ordering::Relaxed), 2);
        assert_eq!(b_counter.load(Ordering::Relaxed), 2);
        assert_eq!(c_counter.load(Ordering::Relaxed), 2);

        a.set(op! {
            a_counter.fetch_add(1, Ordering::Relaxed);
            10
        });

        assert_eq!(run(&c), 12);
        assert_eq!(a_counter.load(Ordering::Relaxed), 3);
        assert_eq!(b_counter.load(Ordering::Relaxed), 3);
        assert_eq!(c_counter.load(Ordering::Relaxed), 2);
    }
}
