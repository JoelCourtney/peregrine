use crate::activities::recharge_potato::RechargePotato;
use peregrine::anyhow::Result;
use peregrine::hifitime::{TimeScale, TimeUnits};
use peregrine::public::resource::piecewise::Piecewise;
use peregrine::public::resource::polynomial::{Linear, Quadratic};
use peregrine::public::resource::timer::Stopwatch;
use peregrine::{Session, Time, initial_conditions, model, pieces, resource};

mod activities;

model! {
    pub PotatoSat {
        battery,
        mode,
        line,
        int_pieces,

        pub timer: Stopwatch
    }
}

resource!(battery: f32);
resource!(mode: String);
resource!(line: Quadratic);
resource!(int_pieces: Piecewise<Linear>);

fn main() -> Result<()> {
    let session = Session::new();

    let plan_start = Time::now()?.to_time_scale(TimeScale::TAI);
    let mut plan = session.new_plan::<PotatoSat>(
        plan_start,
        initial_conditions! {
            battery: 0.0,
            mode: "hello".to_string(),
            line: Quadratic {
                value: 0.0,
                higher_coefficients: [0.0; 2],
                basis: 1.seconds(),
            },
            int_pieces: pieces!(Linear::constant(-1.0)),
            timer: Stopwatch::new()
        },
    );

    plan.insert(plan_start + 5.seconds(), RechargePotato { amount: 1 })?;
    for i in 1..110 {
        let result = plan.sample::<int_pieces>(plan_start + i.seconds())?;
        println!("{i} seconds: {result:?}")
    }

    Ok(())
}
