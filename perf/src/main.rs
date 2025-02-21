use peregrine::exec::SyncBump;
use peregrine::reexports::hifitime::TimeScale;
use peregrine::{impl_activity, model, resource, Duration, History, Model, Plan, Time};
use serde::{Deserialize, Serialize};

model! {
    pub Perf {
        a: a,
        b: b
    }
}

resource!(static a: u32);
resource!(static ref b: String);

struct IncrementA;
impl_activity! { for IncrementA
    @(start) a: a -> a {
        a += 1;
    }
    Duration::ZERO
}

struct ConvertAToB;
impl_activity! { for ConvertAToB
    @(start) a: a -> b: b {
        b = a.to_string()
    }
    Duration::ZERO
}

struct ConvertBToA;
impl_activity! { for ConvertBToA
    @(start) b: b -> a: a {
        a = b.parse().unwrap();
    }
    Duration::ZERO
}

fn main() {
    let bump = SyncBump::new();
    let mut history = History::default();
    Perf::init_history(&mut history);
    let plan_start = Time::now().unwrap().to_time_scale(TimeScale::TAI);
    let mut plan = Plan::<Perf>::new(
        &bump,
        plan_start,
        PerfInitialConditions {
            a: 0,
            b: "".to_string(),
        },
    );

    let mut cursor = plan_start + Duration::from_microseconds(1.0);

    for _ in 0..10_000_000 {
        plan.insert(cursor, IncrementA);
        cursor += Duration::from_seconds(1.0);
        plan.insert(cursor, ConvertAToB);
        cursor += Duration::from_seconds(1.0);
        plan.insert(cursor, ConvertBToA);
        cursor += Duration::from_seconds(1.0);
    }

    println!("built");

    let start = plan_start + Duration::from_seconds(30_000_000.0 - 10.0);
    let result = plan.view::<b>(start..start + Duration::from_seconds(10.0), &history);

    dbg!(result);
}
