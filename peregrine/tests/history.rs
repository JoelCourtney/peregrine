use bincode::config::standard;
use peregrine::history::{DerefHistory, HistoryAdapter};
use peregrine::{History, Result, resource};

resource!(a: u32);
resource!(ref b: String);

#[test]
fn deref_history_valid_across_realloc() {
    let history = DerefHistory::<String>::default();

    // Chosen by button mashing :)
    let hash = 0b10110100100101001010;
    history.insert(hash, "Hello World!".to_string());
    let reference = history.get(hash).unwrap();
    assert_eq!("Hello World!", reference);

    // History default capacity is 1000.
    for _ in 0..2_000 {
        history.insert(rand::random(), "its a string".to_string());
    }

    assert_eq!("Hello World!", reference);
}

#[test]
fn history_serde() -> Result<()> {
    let mut history = History::default();
    history.init::<a>();
    history.init::<b>();

    history.insert::<a>(0, 5);
    history.insert::<a>(1, 6);
    history.insert::<b>(10, "string".to_string());
    history.insert::<b>(11, "another string".to_string());

    let serialized = bincode::serde::encode_to_vec(history, standard())?;
    let deserialized: History = bincode::serde::decode_from_slice(&serialized, standard())?.0;

    assert_eq!(5, deserialized.get::<a>(0).unwrap());
    assert_eq!(6, deserialized.get::<a>(1).unwrap());

    assert_eq!("string", deserialized.get::<b>(10).unwrap());
    assert_eq!("another string", deserialized.get::<b>(11).unwrap());

    assert_eq!(None, deserialized.get::<a>(100));

    Ok(())
}
