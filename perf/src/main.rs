use peregrine::activity::{Ops, OpsReceiver};
use peregrine::macro_prelude::Resource;
use peregrine::reexports::hifitime::{TimeScale, TimeUnits};
use peregrine::reexports::peregrine_macros::op;
use peregrine::{Activity, Duration, Session, Time, initial_conditions, model};
use serde::{Deserialize, Serialize};

fn add_to_u32<'o, Res: Resource<Data = u32>>(add: u32, mut ops: impl OpsReceiver<'o>) {
    ops.push(op! {
        ref mut: Res += add;
    });
}

model! {
    pub Perf {
        a: u32,
        b: String,
        c: u32
    }
}

#[derive(Hash, Serialize, Deserialize)]
struct IncrementA;

#[typetag::serde]
impl Activity for IncrementA {
    fn run(&self, ops: Ops) -> peregrine::Result<Duration> {
        add_to_u32::<a>(1, ops);
        Ok(Duration::ZERO)
    }
}

#[derive(Hash, Serialize, Deserialize)]
struct IncrementC;
#[typetag::serde]
impl Activity for IncrementC {
    fn run(&self, mut ops: Ops) -> peregrine::Result<Duration> {
        ops += op! { ref mut: c += 1; };
        Ok(Duration::ZERO)
    }
}

#[derive(Hash, Serialize, Deserialize)]
struct ConvertAToB;
#[typetag::serde]
impl Activity for ConvertAToB {
    fn run(&self, mut ops: Ops) -> peregrine::Result<Duration> {
        ops += op! { mut:b = ref:a.to_string(); };
        Ok(Duration::ZERO)
    }
}

#[derive(Hash, Serialize, Deserialize)]
struct ConvertBToA;
#[typetag::serde]
impl Activity for ConvertBToA {
    fn run(&self, mut ops: Ops) -> peregrine::Result<Duration> {
        ops += op! { mut:a = ref:b.parse()?; };
        Ok(Duration::ZERO)
    }
}

#[derive(Hash, Serialize, Deserialize)]
struct AddCToA;
#[typetag::serde]
impl Activity for AddCToA {
    fn run(&self, mut ops: Ops) -> peregrine::Result<Duration> {
        ops += op! { ref mut: a += ref:c; };
        Ok(Duration::ZERO)
    }
}

fn main() -> peregrine::Result<()> {
    let session = Session::new();

    let plan_start = Time::now()?.to_time_scale(TimeScale::TAI);
    let mut plan = session.new_plan::<Perf>(
        plan_start,
        initial_conditions! {
            a: 0,
            b: "".to_string(),
            c: 0,
        },
    );

    plan.reserve_activity_capacity(30_000_000);

    let mut cursor = plan_start + Duration::from_microseconds(1.0);

    for _ in 0..10_000_000 {
        plan.insert(cursor, IncrementA)?;
        plan.insert(cursor, IncrementC)?;
        cursor += 1.seconds();
        plan.insert(cursor, ConvertAToB)?;
        cursor += 1.seconds();
        plan.insert(cursor, ConvertBToA)?;
        cursor += 1.seconds();
    }

    plan.insert(cursor + 1.seconds(), AddCToA)?;

    println!("built");

    let start = plan_start + Duration::from_seconds(30_000_000.0 - 10.0);
    let result = plan.view::<a>(start..start + Duration::from_seconds(20.0))?;

    dbg!(result);

    Ok(())
}
