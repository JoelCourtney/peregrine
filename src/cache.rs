use std::sync::Arc;

use oneshot::{Receiver, Sender, channel};
use parking_lot::Mutex;
use replace_with::replace_with_or_abort;

#[derive(Default)]
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
                *self = DataState::Invalid(d)
            }
            DataState::Constant(_) => unreachable!(),
            DataState::Working(_) => {
                panic!("Cannot invalidate a cache while the node is being executed.")
            }
            _ => {}
        }
    }
}

type Invalidator = Box<dyn FnOnce() + Send>;
pub struct Cache<T> {
    data: Mutex<DataState<T>>,
    invalidators: Mutex<Vec<Receiver<Invalidator>>>,
}

impl<T: Send + 'static> Default for Cache<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::enum_variant_names)]
pub(crate) enum CheckResult<T> {
    NoProblem(Cached<T>),
    SomeoneElsesProblem,
    YourProblem,
}

impl<T: Send + 'static> Cache<T> {
    pub fn new() -> Cache<T> {
        Cache {
            data: Mutex::new(DataState::Empty),
            invalidators: Mutex::new(vec![]),
        }
    }
    pub fn new_arc() -> Arc<Cache<T>> {
        Arc::new(Self::new())
    }

    pub(crate) fn check(&self) -> CheckResult<T>
    where
        T: Clone,
    {
        match &mut *self.data.lock() {
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
            ds @ (DataState::Invalid(_) | DataState::Empty) => {
                replace_with_or_abort(ds, |ds| match ds {
                    DataState::Invalid(v) => DataState::Working(Some(v)),
                    DataState::Empty => DataState::Empty,
                    _ => unreachable!(),
                });
                CheckResult::YourProblem
            }
        }
    }

    pub(crate) fn resolve<I>(
        self: &Arc<Self>,
        inputs: Cached<I>,
        run: impl FnOnce(I) -> T,
    ) -> Box<dyn FnMut() -> Cached<T> + '_>
    where
        T: Clone,
    {
        let mut state = self.data.lock();
        match inputs {
            Cached::Constant(v) => {
                let result = run(v);
                *state = DataState::Constant(result.clone());
                Box::new(move || Cached::Constant(result.clone()))
            }
            Cached::Variable {
                value,
                senders,
                revalidated,
            } => {
                let mut invalidators = self.invalidators.lock();
                if revalidated && let DataState::Invalid(v) = std::mem::take(&mut *state) {
                    *state = DataState::Variable {
                        value: v.clone(),
                        revalidated: true,
                    };
                    Box::new(move || {
                        let (send, recv) = channel();
                        invalidators.push(recv);
                        Cached::variable(v.clone(), send, true)
                    })
                } else {
                    let result = run(value);
                    *state = DataState::Variable {
                        value: result.clone(),
                        revalidated: false,
                    };
                    for sender in senders {
                        sender
                            .send(Box::new(self.get_invalidator()))
                            .expect("Could not send invalidator");
                    }
                    Box::new(move || {
                        let (send, recv) = channel();
                        invalidators.push(recv);
                        Cached::variable(result.clone(), send, false)
                    })
                }
            }
        }
    }

    pub fn invalidate(&self) {
        self.data.lock().invalidate();
        for downstream in self.invalidators.lock().drain(..) {
            if let Ok(inv) = downstream.recv() {
                inv();
            }
        }
    }

    pub fn is_valid(&self) -> bool {
        matches!(
            &*self.data.lock(),
            DataState::Variable { .. } | DataState::Constant(_)
        )
    }

    pub fn get_invalidator_sender(&self) -> Sender<Invalidator> {
        let (send, recv) = channel();
        self.invalidators.lock().push(recv);
        send
    }

    pub fn get_invalidator(self: &Arc<Self>) -> impl FnOnce() + Clone + 'static
    where
        T: 'static,
    {
        let weak = Arc::downgrade(self);
        move || {
            if let Some(c) = weak.upgrade() {
                c.invalidate()
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

    pub fn track(self, invalidator: impl FnOnce() + Clone + Send + 'static) -> T {
        match self {
            Cached::Constant(v) => v,
            Cached::Variable { value, senders, .. } => {
                senders.into_iter().for_each(|s| {
                    let i = invalidator.clone();
                    s.send(Box::new(i)).expect("Could not sent invalidator")
                });
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
    use crate::cache::{Cache, Cached};

    #[test]
    fn test_cache_manual_invalidation() {
        let cache_a = Cache::<u32>::new_arc();
        let a = cache_a.resolve(
            Cached::Variable {
                value: (),
                senders: vec![],
                revalidated: false,
            },
            |()| 42,
        )();

        let cache_b = Cache::<String>::new_arc();

        let b = cache_b.resolve(a, |a| format!("{a}"))();

        assert!(matches!(b, Cached::Variable { .. }));
        assert_eq!(b.open(), "42");

        assert!(cache_b.is_valid());

        cache_a.invalidate();
        assert!(!cache_b.is_valid());
    }
}
