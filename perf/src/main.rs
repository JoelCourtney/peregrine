use peregrine::macro_prelude::hifitime::TimeScale;
use peregrine::reexports::hifitime::TimeUnits;
use peregrine::{Result, Session, Time};
use perf_macros::{declare_activities, declare_model, make_initial_conditions, make_plan};
use serde::{Deserialize, Serialize};

declare_model!(1000);

declare_activities!(1000, 100, 10, 5);

fn main() -> Result<()> {
    let plan_start = Time::now()?.to_time_scale(TimeScale::TAI);
    let session = Session::new();
    let mut plan = session.new_plan::<Perf>(plan_start, make_initial_conditions!(1000));

    make_plan!(100, 10);

    let sample = plan.sample::<res_001>(plan_start + 100.centuries())?;
    println!("Result: {sample}");

    Ok(())
}
