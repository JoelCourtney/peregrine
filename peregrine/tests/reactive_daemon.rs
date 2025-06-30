mod util;

use hifitime::TimeUnits;
use peregrine::anyhow::Result;
use peregrine::{
    Activity, Duration, Ops, OpsReceiver, Resource, Session, delay, initial_conditions, model, op,
};
use serde::{Deserialize, Serialize};
use util::{AB, a, b};

use crate::util::{IncrementA, seconds};

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

        react(*) increment_counter()
    }
}

#[derive(Hash, Serialize, Deserialize)]
pub struct IncrementXOrY {
    which: String,
}

#[typetag::serde]
impl Activity for IncrementXOrY {
    fn run<'o>(&'o self, mut ops: Ops<'_, 'o>) -> Result<Duration> {
        let which = &*self.which;
        ops += op! {
            if which == "x" {
                m:x += 1;
            } else if which == "y" {
                m:y += 1;
            }
        };

        Ok(Duration::ZERO)
    }
}

fn set<READ: Resource<Data = u32>, WRITE: Resource<Data = u32>>(mut ops: Ops, add: u32) {
    ops.wait(0.0001.seconds());
    ops.wait(delay! {
        10.seconds() => (r: READ as i64).seconds()
    });
    ops += op! {
        m: WRITE = r: READ + add;
    };
}

fn increment_counter(mut ops: Ops) {
    ops.wait(0.0001.seconds());
    ops += op! {
        m: counter += 1;
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

    plan.insert(
        seconds(0),
        IncrementXOrY {
            which: "x".to_string(),
        },
    )?;
    plan.insert(
        seconds(2),
        IncrementXOrY {
            which: "y".to_string(),
        },
    )?;

    assert_eq!(1, plan.sample::<x>(seconds(1))?);
    assert_eq!(0, plan.sample::<y>(seconds(1))?);
    assert_eq!(1, plan.sample::<counter>(seconds(1))?);

    assert_eq!(1, plan.sample::<x>(seconds(3))?);
    assert_eq!(1, plan.sample::<y>(seconds(3))?);
    assert_eq!(2, plan.sample::<counter>(seconds(3))?);

    Ok(())
}

#[test]
fn test_react_all_removal() -> Result<()> {
    let session = Session::new();
    let mut plan = session
        .new_plan::<ReactAllTest>(seconds(-1), initial_conditions! { x: 0, y: 0, counter: 0 })?;

    let id = plan.insert(
        seconds(0),
        IncrementXOrY {
            which: "x".to_string(),
        },
    )?;

    assert_eq!(1, plan.sample::<x>(seconds(1))?);
    assert_eq!(0, plan.sample::<y>(seconds(1))?);
    assert_eq!(1, plan.sample::<counter>(seconds(1))?);

    plan.remove(id)?;

    assert_eq!(0, plan.sample::<x>(seconds(1))?);
    assert_eq!(0, plan.sample::<y>(seconds(1))?);
    assert_eq!(0, plan.sample::<counter>(seconds(1))?);

    Ok(())
}
