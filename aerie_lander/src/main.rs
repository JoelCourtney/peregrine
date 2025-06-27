use crate::model::{Apss, Comm, Dsn, Eng, HeatProbe, Power};

mod model;

fn main() {
    println!("Hello, world!");
}

peregrine::model! {
    pub Lander {
        ..Eng,
        ..Power,
        ..Comm,
        ..Dsn,
        ..Apss,
        ..HeatProbe,
    }
}
