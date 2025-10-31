use peregrine::{op, run, world::World};

#[test]
fn test_basic() {
    let result = op! {
        let x = i!(42);
        let y = i!(10);
        x * y + i!(100)
    };

    assert_eq!(run(&World::new(), result), 520);
}
