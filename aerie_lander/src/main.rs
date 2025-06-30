use crate::model::{Apss, Comm, Dsn, Eng, HeatProbe, Ids, Power, Seis};

mod model;

fn main() {
    println!("Hello, world!");
}

peregrine::model! {
    pub Lander {}
    mod Eng;
    mod Power;
    mod Comm;
    mod Dsn;
    mod Apss;
    mod HeatProbe;
    mod Ids;
    mod Seis;
}
