use peregrine::{node, run};

#[test]
fn test_basic() {
    let result = node! {
        let x = i!(42usize);
        let y = i!(10);
        x * y + 100
    };

    assert_eq!(run(result), 520);
}
