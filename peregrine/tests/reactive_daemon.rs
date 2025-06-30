mod util;

use hifitime::TimeUnits;
use peregrine::anyhow::Result;
use peregrine::{Ops, OpsReceiver, Resource, Session, delay, initial_conditions, model, op};
use util::{AB, a, b};

use crate::util::{IncrementA, seconds};

model! {
    pub ReactTest {
        ..AB,

        react(a) set::<a, b>(1)
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
