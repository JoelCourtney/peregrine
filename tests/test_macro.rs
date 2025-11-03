use peregrine::{op, run, world::World};

#[test]
fn test_basic() {
    let x = 42;
    let result = op! {
        let y = i!(10);
        x * y + i!(100)
    };

    assert_eq!(run(&World::new(), result), Ok(520));
}
