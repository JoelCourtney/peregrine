mod util;

use hifitime::TimeUnits;
use peregrine::anyhow::Result;
use peregrine::{
    Activity, Duration, Ops, OpsReceiver, Resource, Session, delay, initial_conditions, model, op,
};
use serde::{Deserialize, Serialize};
use util::{AB, a, b};

use crate::util::{IncrementA, seconds};

#[derive(Hash, Serialize, Deserialize)]
pub struct IncrementX;

#[typetag::serde]
impl Activity for IncrementX {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        ops += op! {
            ref mut: x += 1;
        };

        Ok(Duration::ZERO)
    }
}

model! {
    pub ReactTest {
        ..AB,

        react(a) set::<a, b>(1)
    }
}

model! {
    pub ReactAllTest {
        x: u32,
        y: u32,
        counter: u32,

        react(*) increment_counter::<counter>()
    }
}

fn set<READ: Resource<Data = u32>, WRITE: Resource<Data = u32>>(mut ops: Ops, add: u32) {
    ops.wait(0.0001.seconds());
    ops.wait(delay! {
        10.seconds() => (ref: READ as i64).seconds()
    });
    ops += op! {
        ref mut: WRITE = ref: READ + add;
    };
}

fn increment_counter<COUNTER: Resource<Data = u32>>(mut ops: Ops) {
    ops.wait(0.0001.seconds());
    ops += op! {
        ref mut: COUNTER += 1;
    };
}

#[test]
fn test_react_insertion() -> Result<()> {
    let session = Session::new();
    let mut plan =
        session.new_plan::<ReactTest>(seconds(-1), initial_conditions! { a: 0, b: 0 })?;

    plan.insert(seconds(0), IncrementA)?;

    assert_eq!(1, plan.sample::<a>(seconds(1))?);
    assert_eq!(0, plan.sample::<b>(seconds(1))?);
    assert_eq!(2, plan.sample::<b>(seconds(3))?);

    Ok(())
}

#[test]
fn test_react_removal() -> Result<()> {
    let session = Session::new();
    let mut plan =
        session.new_plan::<ReactTest>(seconds(-1), initial_conditions! { a: 0, b: 0 })?;

    let id = plan.insert(seconds(0), IncrementA)?;

    assert_eq!(1, plan.sample::<a>(seconds(1))?);
    assert_eq!(0, plan.sample::<b>(seconds(1))?);
    assert_eq!(2, plan.sample::<b>(seconds(3))?);

    plan.remove(id)?;

    assert_eq!(0, plan.sample::<a>(seconds(1))?);
    assert_eq!(0, plan.sample::<b>(seconds(1))?);
    assert_eq!(0, plan.sample::<b>(seconds(3))?);

    Ok(())
}

#[test]
fn test_react_all_insertion() -> Result<()> {
    let session = Session::new();
    let mut plan = session
        .new_plan::<ReactAllTest>(seconds(-1), initial_conditions! { x: 0, y: 0, counter: 0 })?;

    plan.insert(seconds(0), IncrementX)?;

    // IncrementX changes resource 'x', which should trigger react(*)
    // The reactive daemon increments 'counter' in response to the change in 'x'
    assert_eq!(1, plan.sample::<x>(seconds(1))?);
    assert_eq!(0, plan.sample::<y>(seconds(1))?);
    assert_eq!(1, plan.sample::<counter>(seconds(1))?); // counter gets incremented once for 'x' change

    Ok(())
}

#[test]
fn test_react_all_removal() -> Result<()> {
    let session = Session::new();
    let mut plan = session
        .new_plan::<ReactAllTest>(seconds(-1), initial_conditions! { x: 0, y: 0, counter: 0 })?;

    let id = plan.insert(seconds(0), IncrementX)?;

    assert_eq!(1, plan.sample::<x>(seconds(1))?);
    assert_eq!(0, plan.sample::<y>(seconds(1))?);
    assert_eq!(1, plan.sample::<counter>(seconds(1))?);

    plan.remove(id)?;

    assert_eq!(0, plan.sample::<x>(seconds(1))?);
    assert_eq!(0, plan.sample::<y>(seconds(1))?);
    assert_eq!(0, plan.sample::<counter>(seconds(1))?);

    Ok(())
}
