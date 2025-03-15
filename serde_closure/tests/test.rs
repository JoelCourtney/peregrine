#![forbid(unsafe_code)]
#![warn(
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications,
    unused_results,
    unreachable_pub,
    clippy::pedantic
)]
#![allow(clippy::no_effect_underscore_binding, clippy::unnecessary_wraps)]

use std::{fmt::Debug, hash::Hash};

use serde_closure::{Fn, FnMut, FnMutNamed, FnOnce, traits};

FnMutNamed! {
    pub type Closure<P, E> = |self|partition=> P| -> Result<P,E>
    where
        P: Default,
        E: 'static
    {
        Ok(partition)
    }
}

trait Various: Hash + Debug {}
impl<T: ?Sized> Various for T where T: Hash + Debug {}

#[test]
fn fn_once() {
    fn test<F, A, O>(_: F)
    where
        F: Various + traits::FnOnce<A, Output = O>,
    {
    }

    test::<_, (), ()>(FnOnce!(|| ()));

    let b = String::new();
    test::<_, (String,), ()>(FnOnce!(move |_: String| drop(b)));
}

#[test]
fn fn_once_plain() {
    fn test<F, A, O>(_: F)
    where
        F: traits::FnOnce<A, Output = O>,
    {
    }

    test::<_, (), ()>(|| ());

    let b = String::new();
    test::<_, (String,), ()>(move |_: String| drop(b));
}

#[test]
fn fnmut() {
    fn test<F, A, O>(_: F)
    where
        F: Various + traits::FnMut<A, Output = O>,
    {
    }

    test::<_, (), ()>(FnMut!(|| ()));

    let b = String::new();
    test::<_, (String,), ()>(FnMut!(move |_: String| {
        let _ = b.as_mut_str();
    }));
}

#[test]
fn fnmut_plain() {
    fn test<F, A, O>(_: F)
    where
        F: traits::FnMut<A, Output = O>,
    {
    }

    test::<_, (), ()>(|| ());

    let mut b = String::new();
    test::<_, (String,), ()>(move |_: String| {
        let _ = b.as_mut_str();
    });
}

#[test]
fn fnref() {
    fn test<F, A, O>(_: F)
    where
        F: Various + traits::Fn<A, Output = O>,
    {
    }

    test::<_, (), ()>(Fn!(|| ()));

    let b = String::new();
    test::<_, (String,), ()>(Fn!(move |_: String| {
        let _ = b.as_str();
    }));
}

#[test]
fn fnref_plain() {
    fn test<F, A, O>(_: F)
    where
        F: traits::Fn<A, Output = O>,
    {
    }

    test::<_, (), ()>(|| ());

    let b = String::new();
    test::<_, (String,), ()>(move |_: String| {
        let _ = b.as_str();
    });
}

#[test]
fn multiple_async() {
    let x = 10_usize;

    let _left = async {
        FnOnce!(move || {
            let _ = x;
        })
    };

    let _right = async {
        FnOnce!(move || {
            let _ = x;
        })
    };
}

#[test]
fn static_var() {
    static STATIC: String = String::new();

    FnMut!(move || {
        let _a = &STATIC;
    });
}

mod no_prelude {
    #![no_implicit_prelude]

    #[test]
    fn no_prelude() {
        let a = ::std::string::String::new();
        ::serde_closure::FnOnce!(|| a);
    }
}

#[test]
fn unused_unit() {
    let _ = FnOnce!(|| ());
}

#[test]
fn source() {
    assert!(format!("{:?}", FnOnce!(|| "qwerty")).contains("qwerty"));
}

#[test]
#[serde_closure::desugar]
fn upcast() {
    let closure = FnOnce!(|_x: &str| "test");
    let closure: Box<dyn traits::FnOnceBox(&str) -> &'static str + Send + Sync> = Box::new(closure);
    let _ = closure.call_once_box(("test",));
}
