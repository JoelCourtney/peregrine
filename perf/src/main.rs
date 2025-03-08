use peregrine::reexports::hifitime::{TimeScale, TimeUnits};
use peregrine::{Duration, Session, Time, impl_activity, initial_conditions, model, resource};

model! {
    pub Perf(a, b, c)
}

resource!(a: u32);
resource!(ref b: String);
resource!(c: u32);

#[derive(Hash)]
struct IncrementA;
impl_activity! { for IncrementA
    @(start) {
        ref mut: a += 1;
    }
    Duration::ZERO
}

#[derive(Hash)]
struct IncrementC;
impl_activity! { for IncrementC
    @(start) {
        ref mut: c += 1;
    }
    Duration::ZERO
}

#[derive(Hash)]
struct ConvertAToB;
impl_activity! { for ConvertAToB
    @(start) {
        mut:b = ref:a.to_string();
    }
    Duration::ZERO
}

#[derive(Hash)]
struct ConvertBToA;
impl_activity! { for ConvertBToA
    @(start) {
        mut:a = ref:b.parse()?;
    }
    Duration::ZERO
}

#[derive(Hash)]
struct AddCToA;
impl_activity! ( for AddCToA
    @(start) {
        ref mut: a += ref:c;
    }
    Duration::ZERO
);

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
