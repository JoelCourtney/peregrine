use peregrine::{node, run};

#[test]
fn test_basic() {
    let result = node! {
        let x = i!(42);
        let y = i!(10);
        x * y + i!(100)
    };

    assert_eq!(run(result), 520);
}
