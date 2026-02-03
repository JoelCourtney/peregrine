use std::sync::Arc;
use std::time::{Duration, Instant};

use derive_more::{Deref, DerefMut};
use parking_lot::Mutex;
use peregrine_macros::AutoSource;

use crate::data::Data;
use crate::graph::series::dense::Dense;
use crate::plan::Time;

use crate as peregrine;

pub trait Evolving<I>: Data {
    type Sample: Data;

    fn evolve(&self, from: I, to: I) -> Self;
    fn sample(&self, start: I, sample_at: I) -> Self::Sample;
}

macro_rules! impl_evolving_over_dense {
    ($($t:ty),*) => {
        $(
            impl<D: Evolving<$t>> Evolving<Dense<$t>> for D {
                type Sample = <D as Evolving<$t>>::Sample;
                fn sample(&self, upstream_at: Dense<$t>, at: Dense<$t>) -> Self::Sample {
                    <D as Evolving<$t>>::sample(self, upstream_at.index, at.index)
                }
                fn evolve(&self, from: Dense<$t>, to: Dense<$t>) -> Self {
                    <D as Evolving<$t>>::evolve(self, from.index, to.index)
                }
            }
        )*
    };
}

impl_evolving_over_dense! {
    u8, u16, u32, u64, u128, usize,
    i8, i16, i32, i64, i128, isize,
    f32, f64,
    Duration, hifitime::Duration, Time, Instant
}

#[derive(Debug, Deref, DerefMut, AutoSource)]
pub struct Evolution<I, T: EvolvingSteps<I>> {
    #[deref]
    #[deref_mut]
    value: T,
    #[allow(clippy::type_complexity)]
    evolution: Arc<Mutex<Vec<EvolutionStep<I, T::State>>>>,
}

impl<I, T: EvolvingSteps<I>> PartialEq for Evolution<I, T> {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}

impl<I, T: EvolvingSteps<I>> Clone for Evolution<I, T> {
    fn clone(&self) -> Self {
        Evolution {
            value: self.value.clone(),
            evolution: self.evolution.clone(),
        }
    }
}

#[derive(Debug)]
struct EvolutionStep<I, S> {
    index: I,
    value: S,
}

impl<I: PartialEq, S> PartialEq for EvolutionStep<I, S> {
    fn eq(&self, other: &Self) -> bool {
        self.index.eq(&other.index)
    }
}

impl<I: Eq, S> Eq for EvolutionStep<I, S> {}

impl<I: PartialOrd, S> PartialOrd for EvolutionStep<I, S> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.index.partial_cmp(&other.index)
    }
}

impl<I: Ord, S> Ord for EvolutionStep<I, S> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.index.cmp(&other.index)
    }
}

impl<I, T: EvolvingSteps<I>> Evolution<I, T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            evolution: Arc::new(Mutex::new(Vec::default())),
        }
    }
}

pub trait EvolvingSteps<I>: Data {
    type State: Data;

    fn current_state(&self, at: I) -> Self::State;
    fn step(&self, state: &Self::State, from: I) -> (I, Self::State);
    fn interpolate(
        &self,
        from_state: &Self::State,
        to_state: &Self::State,
        from: I,
        to: I,
        at: I,
    ) -> Self::State;
    fn rebuild(&self, state: Self::State) -> Self;
}

impl<I: Data + PartialOrd, T: EvolvingSteps<I>> Evolving<I> for Evolution<I, T> {
    type Sample = T::State;

    fn evolve(&self, from: I, to: I) -> Self {
        let state = self.sample(from, to);
        Evolution::new(self.value.rebuild(state))
    }

    fn sample(&self, start: I, sample_at: I) -> Self::Sample {
        let evolution = &mut *self.evolution.lock();
        if evolution.is_empty() {
            evolution.push(EvolutionStep {
                index: start.clone(),
                value: self.value.current_state(start),
            });
        }
        let mut last = evolution.last().unwrap();
        let mut computed_new = false;
        while last.index < sample_at {
            computed_new = true;
            let (next_index, next_value) = self.value.step(&last.value, last.index.clone());
            evolution.push(EvolutionStep {
                index: next_index,
                value: next_value,
            });
            last = evolution.last().unwrap();
        }

        if computed_new {
            if last.index == sample_at {
                last.value.clone()
            } else {
                let before_last = &evolution[evolution.len() - 2];
                self.value.interpolate(
                    &before_last.value,
                    &last.value,
                    before_last.index.clone(),
                    last.index.clone(),
                    sample_at,
                )
            }
        } else {
            match evolution.binary_search_by(|step| {
                step.index
                    .partial_cmp(&sample_at)
                    .expect("Failed to compare indices")
            }) {
                Ok(index) => evolution[index].value.clone(),
                Err(index) => {
                    let before = &evolution[index - 1];
                    let after = &evolution[index];
                    self.value.interpolate(
                        &before.value,
                        &after.value,
                        before.index.clone(),
                        after.index.clone(),
                        sample_at,
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use ordered_float::OrderedFloat;

    use crate::{
        data::evolving::{Evolution, EvolvingSteps},
        graph::series::Series,
        run,
    };

    #[derive(Debug)]
    struct Lorenz {
        state: (f64, f64, f64),
        sigma: f64,
        rho: f64,
        beta: f64,
        step_size: f64,
        step_counter: Arc<AtomicUsize>,
    }

    impl Clone for Lorenz {
        fn clone(&self) -> Self {
            Lorenz {
                step_counter: self.step_counter.clone(),
                ..*self
            }
        }
    }

    impl PartialEq for Lorenz {
        fn eq(&self, other: &Self) -> bool {
            self.state == other.state
                && self.sigma == other.sigma
                && self.rho == other.rho
                && self.beta == other.beta
                && self.step_size == other.step_size
        }
    }

    impl EvolvingSteps<OrderedFloat<f64>> for Lorenz {
        type State = (f64, f64, f64);

        fn current_state(&self, _at: OrderedFloat<f64>) -> Self::State {
            self.state
        }

        fn step(
            &self,
            &(x, y, z): &Self::State,
            from: OrderedFloat<f64>,
        ) -> (OrderedFloat<f64>, Self::State) {
            self.step_counter.fetch_add(1, Ordering::Relaxed);
            dbg!(from);
            let to = from + self.step_size;
            let dx = self.sigma * (y - x);
            let dy = x * (self.rho - z) - y;
            let dz = x * y - self.beta * z;
            (
                to,
                (
                    x + dx * self.step_size,
                    y + dy * self.step_size,
                    z + dz * self.step_size,
                ),
            )
        }

        fn interpolate(
            &self,
            &(x1, y1, z1): &Self::State,
            &(x2, y2, z2): &Self::State,
            from: OrderedFloat<f64>,
            to: OrderedFloat<f64>,
            at: OrderedFloat<f64>,
        ) -> Self::State {
            let x = (at - from) * (x2 - x1) / (to - from) + x1;
            let y = (at - from) * (y2 - y1) / (to - from) + y1;
            let z = (at - from) * (z2 - z1) / (to - from) + z1;
            (*x, *y, *z)
        }

        fn rebuild(&self, state: Self::State) -> Self {
            Lorenz {
                state,
                step_counter: Arc::new(AtomicUsize::new(0)),
                ..*self
            }
        }
    }

    #[test]
    fn sample_step_counter() {
        let step_counter = Arc::new(AtomicUsize::new(0));
        let lorenz = Lorenz {
            state: (0.0, 1.0, 1.0),
            step_size: 0.01,
            sigma: 10.0,
            rho: 28.0,
            beta: 8.0 / 3.0,
            step_counter: step_counter.clone(),
        };
        let s = Series::new(Evolution::new(lorenz.clone()));
        s.set_at(OrderedFloat(0.0), Evolution::new(lorenz));

        run(s.sample_at(OrderedFloat(5.0)));

        // 501 instead of 500 because floating point math
        assert_eq!(501, step_counter.load(Ordering::Relaxed));

        run(s.sample_at(6.0.into()));

        assert_eq!(601, step_counter.load(Ordering::Relaxed));

        run(s.sample_at(3.0.into()));

        assert_eq!(601, step_counter.load(Ordering::Relaxed));
    }
}
