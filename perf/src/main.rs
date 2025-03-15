use peregrine::activity::Ops;
use peregrine::reexports::hifitime::{TimeScale, TimeUnits};
use peregrine::reexports::peregrine_macros::op;
use peregrine::{Activity, Duration, Session, Time, initial_conditions, model};

model! {
    pub Perf {
        a: u32,
        b: String,
        c: u32
    }
}

#[derive(Hash)]
struct IncrementA;

impl Activity for IncrementA {
    fn run(&self, mut ops: Ops) -> peregrine::Result<Duration> {
        ops += op! { ref mut: a += 1; };
        Ok(Duration::ZERO)
    }
}

#[derive(Hash)]
struct IncrementC;
impl Activity for IncrementC {
    fn run(&self, mut ops: Ops) -> peregrine::Result<Duration> {
        ops += op! { ref mut: c += 1; };
        Ok(Duration::ZERO)
    }
}

#[derive(Hash)]
struct ConvertAToB;
impl Activity for ConvertAToB {
    fn run(&self, mut ops: Ops) -> peregrine::Result<Duration> {
        ops += op! { mut:b = ref:a.to_string(); };
        Ok(Duration::ZERO)
    }
}

#[derive(Hash)]
struct ConvertBToA;
impl Activity for ConvertBToA {
    fn run(&self, mut ops: Ops) -> peregrine::Result<Duration> {
        ops += op! { mut:a = ref:b.parse()?; };
        Ok(Duration::ZERO)
    }
}

#[derive(Hash)]
struct AddCToA;
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
