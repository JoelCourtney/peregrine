use crate::model::{Eng, Power};

mod model;

fn main() {
    println!("Hello, world!");
}

peregrine::model! {
    pub Lander {
        ..Eng,
        ..Power
    }
}
