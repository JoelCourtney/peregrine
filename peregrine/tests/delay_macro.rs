//! Integration test for the delay! macro.
//! This test checks that delay! inserts a dynamic delay node and that the plan simulates the delay correctly.

mod util;

use peregrine::*;
use peregrine_macros::{delay, op};
use util::*;

use hifitime::Duration;
use peregrine::anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Hash, Serialize, Deserialize)]
pub struct StaticDelay;

#[typetag::serde]
impl Activity for StaticDelay {
    fn run<'o>(&'o self, mut ops: Ops<'_, 'o>) -> Result<Duration> {
        // Increment a, then wait for a dynamic delay, then increment a again
        ops += op! { m: a += 1; };
        // Wait for a delay of 5 seconds, but the actual delay node is dynamic
        ops.wait(delay! { Duration::from_seconds(5.0) => Duration::from_seconds(5.0) });
        ops += op! { m: a += 1; };
        Ok(Duration::ZERO)
    }
}

#[test]
fn test_basic_delay() -> Result<()> {
    let session = Session::new();
    let mut plan = init_plan(&session);
    plan.insert(seconds(2), StaticDelay)?;
    assert_eq!(0, plan.sample::<a>(seconds(1))?);
    assert_eq!(1, plan.sample::<a>(seconds(4))?);
    assert_eq!(2, plan.sample::<a>(seconds(9))?);
    Ok(())
}

#[derive(Hash, Serialize, Deserialize)]
pub struct DoubleDelay;

#[typetag::serde]
impl Activity for DoubleDelay {
    fn run<'o>(&'o self, mut ops: Ops<'_, 'o>) -> Result<Duration> {
        ops.wait(delay! { Duration::from_seconds(10.0) => r: elapsed });
        ops += op! { m: a += 1; };
        Ok(Duration::ZERO)
    }
}

#[test]
fn test_double_delay() -> Result<()> {
    let session = Session::new();
    let mut plan = init_plan(&session);
    plan.insert(seconds(2), DoubleDelay)?;
    // assert_eq!(0, plan.sample::<a>(seconds(1))?);
    // assert_eq!(0, plan.sample::<a>(seconds(3))?);
    assert_eq!(1, plan.sample::<a>(seconds(7))?);
    Ok(())
}
