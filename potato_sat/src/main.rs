use peregrine::{model, Resource};
use peregrine::{CopyHistory, DerefHistory};
use serde::{Deserialize, Serialize};

mod activities;

model! {
    pub PotatoSat {
        battery: Battery,
        mode: Mode
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum Battery {}

impl<'h> Resource<'h> for Battery {
    const STATIC: bool = true;

    type Read = f32;
    type Write = f32;

    type History = CopyHistory<f32>;
}

#[derive(Debug, Serialize, Deserialize)]
enum Mode {}

impl<'h> Resource<'h> for Mode {
    const STATIC: bool = true;
    type Read = &'h str;
    type Write = String;
    type History = DerefHistory<String>;
}

fn main() {}
