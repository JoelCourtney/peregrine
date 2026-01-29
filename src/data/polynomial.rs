use std::ops::{Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Sub, SubAssign};

use crate::graph::series::dense::Dense;
use hifitime::Duration;
use num::{NumCast, ToPrimitive, Zero};
use peregrine_macros::AutoSource;

use crate::data::{Data, Evolving};

use crate as peregrine;

#[derive(Copy, Clone, PartialEq, Debug, AutoSource)]
pub struct Polynomial<const N: usize, I, T> {
    intercept: T,
    higher_coefficients: [T; N],
    pub basis: I,
}

macro_rules! impl_constructors {
    ($($n:literal => $($etc:ident)*;)*) => {
        $(
            impl<I, T> Polynomial<$n, I, T> {
                pub fn new(basis: I, a: T, $($etc: T,)*) -> Self {
                    Self {
                        intercept: a,
                        higher_coefficients: [$($etc,)*],
                        basis
                    }
                }
            }
        )*
    };
}

impl_constructors![
    0 => ;
    1 => b;
    2 => b c;
    3 => b c d;
    4 => b c d e;
    5 => b c d e f;
];

macro_rules! impl_constant_constructors {
    ($($basis:ident => $basis_value:expr;)*) => {
        $(
            impl<const N: usize, T: Copy + Data + Zero> Polynomial<N, $basis, T> {
                pub fn constant(a: T) -> Self {
                    Self {
                        intercept: a,
                        higher_coefficients: [T::zero(); N],
                        basis: $basis_value,
                    }
                }
            }
        )*
    };
}

impl_constant_constructors![
    Duration => Duration::from_seconds(1.0);
    f32 => 1.0;
    f64 => 1.0;
    i8 => 1;
    i16 => 1;
    i32 => 1;
    i64 => 1;
    i128 => 1;
    isize => 1;
    u8 => 1;
    u16 => 1;
    u32 => 1;
    u64 => 1;
    u128 => 1;
    usize => 1;
];

impl<const N: usize, I, T> Index<usize> for Polynomial<N, I, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        if index == 0 {
            &self.intercept
        } else {
            &self.higher_coefficients[index - 1]
        }
    }
}

impl<const N: usize, I, T> IndexMut<usize> for Polynomial<N, I, T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        if index == 0 {
            &mut self.intercept
        } else {
            &mut self.higher_coefficients[index - 1]
        }
    }
}

impl<const N: usize, I, T: Zero> Polynomial<N, I, T> {
    pub fn intercept(&self) -> &T {
        &self.intercept
    }

    pub fn slope(&self) -> &T {
        if N >= 1 {
            &self.higher_coefficients[0]
        } else {
            panic!("Polynomial degree {N} is too low to reference slope");
        }
    }

    pub fn acceleration(&self) -> &T {
        if N >= 2 {
            &self.higher_coefficients[1]
        } else {
            panic!("Polynomial degree {N} is too low to reference acceleration");
        }
    }

    pub fn jerk(&self) -> &T {
        if N >= 3 {
            &self.higher_coefficients[2]
        } else {
            panic!("Polynomial degree {N} is too low to reference jerk");
        }
    }

    pub fn intercept_mut(&mut self) -> &mut T {
        &mut self.intercept
    }

    pub fn slope_mut(&mut self) -> &mut T {
        if N >= 1 {
            &mut self.higher_coefficients[0]
        } else {
            panic!("Polynomial degree {N} is too low to mutate slope");
        }
    }

    pub fn acceleration_mut(&mut self) -> &mut T {
        if N >= 2 {
            &mut self.higher_coefficients[1]
        } else {
            panic!("Polynomial degree {N} is too low to mutate acceleration");
        }
    }

    pub fn jerk_mut(&mut self) -> &mut T {
        if N >= 3 {
            &mut self.higher_coefficients[2]
        } else {
            panic!("Polynomial degree {N} is too low to mutate jerk");
        }
    }
}

impl<const N: usize, I, T: Add<T, Output = T>> Add<T> for Polynomial<N, I, T> {
    type Output = Polynomial<N, I, T>;

    fn add(mut self, rhs: T) -> Self::Output {
        self.intercept = self.intercept + rhs;
        self
    }
}

impl<const N: usize, I, T: AddAssign<T>> AddAssign<T> for Polynomial<N, I, T> {
    fn add_assign(&mut self, rhs: T) {
        self.intercept += rhs;
    }
}

impl<const N: usize, I, T: Sub<T, Output = T>> Sub<T> for Polynomial<N, I, T> {
    type Output = Polynomial<N, I, T>;

    fn sub(mut self, rhs: T) -> Self::Output {
        self.intercept = self.intercept - rhs;
        self
    }
}

impl<const N: usize, I, T: SubAssign<T>> SubAssign<T> for Polynomial<N, I, T> {
    fn sub_assign(&mut self, rhs: T) {
        self.intercept -= rhs;
    }
}

impl<C: Copy, const N: usize, I, T: Mul<C, Output = T>> Mul<C> for Polynomial<N, I, T> {
    type Output = Polynomial<N, I, T>;

    fn mul(mut self, rhs: C) -> Self::Output {
        self.intercept = self.intercept * rhs;
        self.higher_coefficients = self.higher_coefficients.map(|coeff| coeff * rhs);
        self
    }
}

impl<C: Copy, const N: usize, I, T: MulAssign<C>> MulAssign<C> for Polynomial<N, I, T> {
    fn mul_assign(&mut self, rhs: C) {
        self.intercept *= rhs;
        for i in 0..N {
            self.higher_coefficients[i] *= rhs;
        }
    }
}

impl<C: Copy, const N: usize, I, T: Div<C, Output = T>> Div<C> for Polynomial<N, I, T> {
    type Output = Polynomial<N, I, T>;

    fn div(mut self, rhs: C) -> Self::Output {
        self.intercept = self.intercept / rhs;
        self.higher_coefficients = self.higher_coefficients.map(|coeff| coeff / rhs);
        self
    }
}

impl<C: Copy, const N: usize, I, T: DivAssign<C>> DivAssign<C> for Polynomial<N, I, T> {
    fn div_assign(&mut self, rhs: C) {
        self.intercept /= rhs;
        for i in 0..N {
            self.higher_coefficients[i] /= rhs;
        }
    }
}

macro_rules! impl_evolving {
    ($($basis:ty => $basis_div_transform:ident -> $measure_type:ty;)*) => {
        $(
            impl<const N: usize, T: Data + Mul<$measure_type, Output=T> + Add<T, Output=T>> Evolving<$basis> for Polynomial<N, $basis, T> {
                type Sample = T;

                fn sample(&self, start: $basis, sample_at: $basis) -> Self::Sample {
                    let mut result = self.intercept.clone();
                    let measure = $basis_div_transform(sample_at - start) / $basis_div_transform(self.basis);
                    let mut x = measure;
                    for i in 0..N {
                        result = result + self.higher_coefficients[i].clone() * x;
                        x *= measure;
                    }
                    result
                }

                fn evolve(&self, from: $basis, to: $basis) -> Self {
                    let measure = $basis_div_transform(to - from) / $basis_div_transform(self.basis);
                    let mut result = self.clone();

                    let mut acc = result.higher_coefficients[N - 1].clone();
                    for i in (0..N - 1).rev() {
                        let old = self.higher_coefficients[i].clone();
                        let diff = acc * measure;
                        result.higher_coefficients[i] = self.higher_coefficients[i].clone() + diff.clone();
                        acc = diff + old;
                    }
                    result.intercept = result.intercept + acc * measure;
                    result
                }
            }

            impl<const N: usize, T: Data + Mul<$measure_type, Output=T> + Add<T, Output=T>> Evolving<Dense<$basis>> for Polynomial<N, $basis, T> {
                type Sample = T;

                fn sample(&self, start: Dense<$basis>, sample_at: Dense<$basis>) -> Self::Sample {
                    self.sample(start.index, sample_at.index)
                }

                fn evolve(&self, from: Dense<$basis>, to: Dense<$basis>) -> Self {
                    let measure = $basis_div_transform(to.index - from.index) / $basis_div_transform(self.basis);
                    let mut result = self.clone();

                    let mut acc = result.higher_coefficients[N - 1].clone();
                    for i in (0..N - 1).rev() {
                        let old = self.higher_coefficients[i].clone();
                        let diff = acc * measure;
                        result.higher_coefficients[i] = self.higher_coefficients[i].clone() + diff.clone();
                        acc = diff + old;
                    }
                    result.intercept = result.intercept + acc * measure;
                    result
                }
            }
        )*
    };
}

impl_evolving! {
    f32 => identity -> f32;
    f64 => identity -> f64;
    u8 => to_f64 -> f64;
    u16 => to_f64 -> f64;
    u32 => to_f64 -> f64;
    u64 => to_f64 -> f64;
    u128 => to_f64 -> f64;
    usize => to_f64 -> f64;
    i8 => to_f64 -> f64;
    i16 => to_f64 -> f64;
    i32 => to_f64 -> f64;
    i64 => to_f64 -> f64;
    i128 => to_f64 -> f64;
    isize => to_f64 -> f64;
    Duration => duration_to_seconds -> f64;
}

fn identity<T>(value: T) -> T {
    value
}

fn to_f64<T: ToPrimitive>(value: T) -> f64 {
    <f64 as NumCast>::from(value).unwrap()
}

fn duration_to_seconds(duration: Duration) -> f64 {
    duration.to_seconds()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polynomial_evolution() {
        let poly = Polynomial::<2, _, _>::new(1.0, 1.0, 2.0, 3.0);
        let evolved = poly.evolve(0.0, 1.0);
        assert_eq!(evolved[2], 3.0);
        assert_eq!(evolved[1], 5.0);
        assert_eq!(evolved[0], 6.0);
    }

    #[test]
    fn polynomial_sample() {
        let poly = Polynomial::<2, _, _>::new(1.0, 1.0, 2.0, 3.0);
        assert_eq!(poly.sample(0.0, 2.0), 17.0);
    }
}
