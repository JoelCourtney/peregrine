use crate::model::{Apss, Comm, Dsn, Eng, HeatProbe, Ids, Power, Seis};

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
        ..Ids,
        ..Seis,
    }
}
