use bincode::config::standard;
use hifitime::Duration;
use peregrine::macro_prelude::duration_to_epoch;
use peregrine::{History, Result, Time, resource};

resource!(a: u32);
resource!(b: String);

const TIME: Time = duration_to_epoch(Duration::ZERO);

#[test]
fn history_serde() -> Result<()> {
    let mut history = History::default();
    history.init::<a>();
    history.init::<b>();

    history.insert::<a>(0, 5, TIME);
    history.insert::<a>(1, 6, TIME);
    history.insert::<b>(10, "string".to_string(), TIME);
    history.insert::<b>(11, "another string".to_string(), TIME);

    let serialized = bincode::serde::encode_to_vec(history, standard())?;
    let deserialized: History = bincode::serde::decode_from_slice(&serialized, standard())?.0;

    assert_eq!(5, deserialized.get::<a>(0, TIME).unwrap());
    assert_eq!(6, deserialized.get::<a>(1, TIME).unwrap());

    assert_eq!("string", deserialized.get::<b>(10, TIME).unwrap());
    assert_eq!("another string", deserialized.get::<b>(11, TIME).unwrap());

    assert_eq!(None, deserialized.get::<a>(100, TIME));

    Ok(())
}
