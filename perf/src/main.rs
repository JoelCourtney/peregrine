use clap::Parser;
use peregrine::macro_prelude::hifitime::TimeScale;
use peregrine::reexports::hifitime::TimeUnits;
use peregrine::{Result, Session, Time};
use perf_macros::{declare_activities, declare_model, make_initial_conditions, make_plan, make_samples};
use serde::{Deserialize, Serialize};

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Number of activities to spam
    #[arg(short, long)]
    num_activities: usize,
}

const PREBAKED_ACTIVITIES: usize = 100;

declare_model!(1000);

declare_activities!(1000, 100, 10, 5);

fn main() -> Result<()> {
    let args = Args::parse();

    let plan_start = Time::now()?.to_time_scale(TimeScale::TAI);
    let session = Session::new();
    let mut plan = session.new_plan::<Perf>(plan_start, make_initial_conditions!(1000));

    for i in 0..(args.num_activities / PREBAKED_ACTIVITIES) {
        let offset = (i as i64) * 137.nanoseconds();
        make_plan!(100, 100);
    }

    make_samples!(1000);

    Ok(())
}
