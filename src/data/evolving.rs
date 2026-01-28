use crate::data::Data;

pub trait Evolving<I>: Data {
    type Sample: Data;

    fn evolve(&self, from: I, to: I) -> Self;
    fn sample(&self, start: I, sample_at: I) -> Self::Sample;
}
