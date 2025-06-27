mod util;

use anyhow::Result;
use peregrine::internal::history::History;
use peregrine::*;
use std::sync::atomic::Ordering;
use util::*;

#[test]
fn cache_across_runs() -> Result<()> {
    let session = Session::new();
    let mut plan = init_plan(&session);

    let (node, counter) = EvalCounter::new();

    plan.insert(seconds(0), IncrementA)?;
    plan.insert(seconds(1), node)?;
    plan.insert(seconds(2), SetBToA)?;
    plan.insert(seconds(3), IncrementA)?;

    assert_eq!(0, counter.load(Ordering::SeqCst));

    assert_eq!(2, plan.sample::<a>(seconds(4))?);
    assert_eq!(1, counter.load(Ordering::SeqCst));

    assert_eq!(1, plan.sample::<b>(seconds(4))?);
    assert_eq!(1, plan.sample::<b>(seconds(4))?);
    assert_eq!(1, plan.sample::<b>(seconds(4))?);
    assert_eq!(1, counter.load(Ordering::SeqCst));

    Ok(())
}

#[test]
fn cache_within_single_run() -> Result<()> {
    let session = Session::new();
    let mut plan = init_plan(&session);

    let (node, counter) = EvalCounter::new();

    plan.insert(seconds(0), IncrementA)?;
    plan.insert(seconds(1), node)?;
    plan.insert(seconds(2), SetBToA)?;
    plan.insert(seconds(3), IncrementA)?;
    plan.insert(seconds(4), AddBToA)?;

    assert_eq!(0, counter.load(Ordering::SeqCst));

    assert_eq!(3, plan.sample::<a>(seconds(4))?);
    assert_eq!(1, counter.load(Ordering::SeqCst));

    Ok(())
}

#[test]
fn load_cache_from_history() -> Result<()> {
    let session = Session::new();
    let mut plan = init_plan(&session);
    let (node, counter) = EvalCounter::new();

    plan.insert(seconds(0), IncrementA)?;
    plan.insert(seconds(1), node)?;
    plan.insert(seconds(2), SetBToA)?;

    assert_eq!(0, counter.load(Ordering::SeqCst));
    assert_eq!(1, plan.sample::<b>(seconds(2))?);
    assert_eq!(1, counter.load(Ordering::SeqCst));

    drop(plan);
    let session = Session::from(History::from(session.into_history().into_inner()));
    let mut plan = init_plan(&session);
    let (node, counter) = EvalCounter::new();

    plan.insert(seconds(0), IncrementA)?;
    plan.insert(seconds(1), node)?;
    plan.insert(seconds(2), IncrementA)?;

    assert_eq!(0, counter.load(Ordering::SeqCst));
    assert_eq!(2, plan.sample::<a>(seconds(2))?);
    assert_eq!(0, counter.load(Ordering::SeqCst));

    Ok(())
}

#[test]
fn load_cache_after_rollbacks_no_sim() -> Result<()> {
    let session = Session::new();
    let mut plan = init_plan(&session);

    let (node1, counter1) = EvalCounter::new();
    let (node2, counter2) = EvalCounter::new();

    plan.insert(seconds(0), IncrementA)?;
    plan.insert(seconds(1), node1)?;
    let id = plan.insert(seconds(2), IncrementA)?;
    plan.insert(seconds(3), node2)?;
    plan.insert(seconds(4), IncrementA)?;

    assert_eq!(0, counter1.load(Ordering::SeqCst));
    assert_eq!(0, counter2.load(Ordering::SeqCst));
    assert_eq!(3, plan.sample::<a>(seconds(4))?);
    assert_eq!(1, counter1.load(Ordering::SeqCst));
    assert_eq!(1, counter2.load(Ordering::SeqCst));

    plan.remove(id)?;

    plan.insert(seconds(2), IncrementA)?;

    assert_eq!(3, plan.sample::<a>(seconds(4))?);
    assert_eq!(1, counter1.load(Ordering::SeqCst));
    assert_eq!(1, counter2.load(Ordering::SeqCst));

    Ok(())
}

#[test]
fn load_cache_after_rollbacks_sim_in_between() -> Result<()> {
    let session = Session::new();
    let mut plan = init_plan(&session);

    let (node1, counter1) = EvalCounter::new();
    let (node2, counter2) = EvalCounter::new();

    plan.insert(seconds(0), IncrementA)?;
    plan.insert(seconds(1), node1)?;
    let id = plan.insert(seconds(2), IncrementA)?;
    plan.insert(seconds(3), node2)?;
    plan.insert(seconds(4), IncrementA)?;

    assert_eq!(0, counter1.load(Ordering::SeqCst));
    assert_eq!(0, counter2.load(Ordering::SeqCst));
    assert_eq!(3, plan.sample::<a>(seconds(4))?);
    assert_eq!(1, counter1.load(Ordering::SeqCst));
    assert_eq!(1, counter2.load(Ordering::SeqCst));

    plan.remove(id)?;

    assert_eq!(2, plan.sample::<a>(seconds(4))?);

    plan.insert(seconds(2), IncrementA)?;

    assert_eq!(3, plan.sample::<a>(seconds(4))?);
    assert_eq!(1, counter1.load(Ordering::SeqCst));
    assert_eq!(1, counter2.load(Ordering::SeqCst));

    Ok(())
}
