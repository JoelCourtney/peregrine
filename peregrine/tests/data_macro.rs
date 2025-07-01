use peregrine::hifitime::TimeUnits;
use peregrine::{Data, Linear, MaybeHash, Time};
use serde::{Deserialize, Serialize};

// Test basic struct with evolution
#[derive(Data, MaybeHash, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: for<'a> Data<'a>"))]
#[serde(bound(serialize = "T: for<'a> Data<'a>"))]
struct GenericTestStruct<T: for<'a> Data<'a>> {
    value: T,
}

// Test basic struct with evolution
#[derive(Data, MaybeHash, Clone, Serialize, Deserialize)]
struct TestStruct {
    value: Linear,
    count: u32,
}

// Test struct with custom sample type
#[derive(Data, MaybeHash, Clone, Serialize, Deserialize)]
#[sample = "CustomSampleStructSample"]
struct CustomSampleStruct {
    value: Linear,
    flag: bool,
}

// Test struct with Self sample
#[derive(Data, MaybeHash, Clone, Serialize, Deserialize)]
#[sample = "Self"]
struct SelfSampleStruct {
    value: Linear,
    name: String,
}

// Test struct with unnamed fields
#[derive(Data, MaybeHash, Clone, Serialize, Deserialize)]
struct UnnamedStruct(Linear, u32);

// Test enum with evolution
#[derive(Data, MaybeHash, Clone, Serialize, Deserialize)]
enum TestEnum {
    Unit,
    Single(Linear),
    Multiple { value: Linear, count: u32 },
}

// Test enum with custom sample type
#[derive(Data, MaybeHash, Clone, Serialize, Deserialize)]
#[sample = "CustomSampleEnumSample"]
enum CustomSampleEnum {
    Unit,
    Single(Linear),
    Multiple { value: Linear, flag: bool },
}

// Test enum with Self sample
#[derive(Data, MaybeHash, Clone, Serialize, Deserialize)]
#[sample = "Self"]
enum SelfSampleEnum {
    Unit,
    Single(Linear),
    Multiple { value: Linear, name: String },
}

// Test fieldless struct
#[derive(Data, Copy, Clone, MaybeHash, Serialize, Deserialize)]
struct FieldlessStruct;

// Test fieldless enum
#[derive(Data, Copy, Clone, MaybeHash, Serialize, Deserialize)]
enum FieldlessEnum {
    Unit,
    Another,
}

// Test public struct
#[derive(Data, MaybeHash, Clone, Serialize, Deserialize)]
pub struct PublicStruct {
    pub value: Linear,
    pub count: u32,
}

#[test]
fn test_struct_basic() {
    let original = TestStruct {
        value: Linear::new(1.seconds(), 10.0, 2.0),
        count: 42,
    };

    let written = Time::from_et_seconds(1000.0);
    let read = original.to_read(written);

    // Verify Read type structure - Linear::Read is (Time, Linear)
    assert_eq!(read.value.1.value, 10.0);
    assert_eq!(read.value.1.higher_coefficients[0], 2.0);
    assert_eq!(read.count, 42);

    // Test evolution in from_read
    let now = written + 2.seconds();
    let evolved = TestStruct::from_read(read, now);

    // Linear should have evolved: value + slope * time
    // slope = 2.0, time = 2.0, so new value should be 10.0 + 2.0 * 2.0 = 14.0
    assert!((evolved.value.value - 14.0).abs() < 1e-10);
    assert_eq!(evolved.count, 42); // count should not evolve

    // Test sampling
    let sample = TestStruct::sample(read, now);
    assert!((sample.value.value - 14.0).abs() < 1e-10);
    assert_eq!(sample.count, 42);
}

#[test]
fn test_struct_custom_sample() {
    let original = CustomSampleStruct {
        value: Linear::new(1.seconds(), 5.0, 1.5),
        flag: true,
    };

    let written = Time::from_et_seconds(2000.0);
    let read = original.to_read(written);
    let now = written + 3.seconds();

    // Test that sample type is different from original
    let sample = CustomSampleStruct::sample(read, now);

    // Value should have evolved: 5.0 + 1.5 * 3.0 = 9.5
    assert!((sample.value.value - 9.5).abs() < 1e-10);
    assert!(sample.flag);

    // Verify that sample is of the custom type, not the original
    // This is implicit in the type system, but we can verify the behavior
    let evolved = CustomSampleStruct::from_read(read, now);
    assert!((evolved.value.value - 9.5).abs() < 1e-10);
    assert!(evolved.flag);
}

#[test]
fn test_struct_self_sample() {
    let original = SelfSampleStruct {
        value: Linear::new(1.seconds(), 3.0, 0.5),
        name: "test".to_string(),
    };

    let written = Time::from_et_seconds(3000.0);
    let read = original.to_read(written);
    let now = written + 4.seconds();

    // Test that sample returns the same type as original
    let sample = SelfSampleStruct::sample(read, now);

    // Value should have evolved: 3.0 + 0.5 * 4.0 = 5.0
    assert!((sample.value.value - 5.0).abs() < 1e-10);
    assert_eq!(sample.name, "test");

    // Verify that sample is the same type as original
    let evolved = SelfSampleStruct::from_read(read, now);
    assert!((evolved.value.value - 5.0).abs() < 1e-10);
    assert_eq!(evolved.name, "test");
}

#[test]
fn test_struct_unnamed() {
    let original = UnnamedStruct(Linear::new(1.seconds(), 7.0, 1.0), 100);

    let written = Time::from_et_seconds(4000.0);
    let read = original.to_read(written);
    let now = written + 1.5.seconds();

    // Test evolution
    let evolved = UnnamedStruct::from_read(read, now);
    // Value should have evolved: 7.0 + 1.0 * 1.5 = 8.5
    assert!((evolved.0.value - 8.5).abs() < 1e-10);
    assert_eq!(evolved.1, 100);

    // Test sampling
    let sample = UnnamedStruct::sample(read, now);
    assert!((sample.0.value - 8.5).abs() < 1e-10);
    assert_eq!(sample.1, 100);
}

#[test]
fn test_enum_basic() {
    let original = TestEnum::Multiple {
        value: Linear::new(1.seconds(), 2.0, 3.0),
        count: 7,
    };

    let written = Time::from_et_seconds(5000.0);
    let read = original.to_read(written);
    let now = written + 2.5.seconds();

    // Test evolution
    let evolved = TestEnum::from_read(read, now);
    match evolved {
        TestEnum::Multiple { value, count } => {
            // Value should have evolved: 2.0 + 3.0 * 2.5 = 9.5
            assert!((value.value - 9.5).abs() < 1e-10);
            assert_eq!(count, 7);
        }
        _ => panic!("Expected Multiple variant"),
    }

    // Test sampling - use the generated sample type
    let sample = TestEnum::sample(read, now);
    match sample {
        TestEnumSample::Multiple { value, count } => {
            assert!((value.value - 9.5).abs() < 1e-10);
            assert_eq!(count, 7);
        }
        _ => panic!("Expected Multiple variant"),
    }
}

#[test]
fn test_enum_unit() {
    let original = TestEnum::Unit;

    let written = Time::from_et_seconds(6000.0);
    let read = original.to_read(written);
    let now = written + 1.seconds();

    // Unit variants should not evolve
    let evolved = TestEnum::from_read(read, now);
    match evolved {
        TestEnum::Unit => {}
        _ => panic!("Expected Unit variant"),
    }

    let sample = TestEnum::sample(read, now);
    match sample {
        TestEnumSample::Unit => {}
        _ => panic!("Expected Unit variant"),
    }
}

#[test]
fn test_enum_single() {
    let original = TestEnum::Single(Linear::new(1.seconds(), 1.0, 2.0));

    let written = Time::from_et_seconds(7000.0);
    let read = original.to_read(written);
    let now = written + 1.0.seconds();

    // Test evolution
    let evolved = TestEnum::from_read(read, now);
    match evolved {
        TestEnum::Single(value) => {
            // Value should have evolved: 1.0 + 2.0 * 1.0 = 3.0
            assert!((value.value - 3.0).abs() < 1e-10);
        }
        _ => panic!("Expected Single variant"),
    }

    // Test sampling
    let sample = TestEnum::sample(read, now);
    match sample {
        TestEnumSample::Single(value) => {
            assert!((value.value - 3.0).abs() < 1e-10);
        }
        _ => panic!("Expected Single variant"),
    }
}

#[test]
fn test_enum_custom_sample() {
    let original = CustomSampleEnum::Multiple {
        value: Linear::new(1.seconds(), 4.0, 1.0),
        flag: false,
    };

    let written = Time::from_et_seconds(8000.0);
    let read = original.to_read(written);
    let now = written + 2.0.seconds();

    // Test that sample type is different from original
    let sample = CustomSampleEnum::sample(read, now);
    match sample {
        CustomSampleEnumSample::Multiple { value, flag } => {
            // Value should have evolved: 4.0 + 1.0 * 2.0 = 6.0
            assert!((value.value - 6.0).abs() < 1e-10);
            assert!(!flag);
        }
        _ => panic!("Expected Multiple variant"),
    }
}

#[test]
fn test_enum_self_sample() {
    let original = SelfSampleEnum::Multiple {
        value: Linear::new(1.seconds(), 6.0, 0.5),
        name: "enum_test".to_string(),
    };

    let written = Time::from_et_seconds(9000.0);
    let read = original.to_read(written);
    let now = written + 3.0.seconds();

    // Test that sample returns the same type as original
    let sample = SelfSampleEnum::sample(read, now);
    match sample {
        SelfSampleEnumSample::Multiple { value, name } => {
            // Value should have evolved: 6.0 + 0.5 * 3.0 = 7.5
            assert!((value.value - 7.5).abs() < 1e-10);
            assert_eq!(name, "enum_test");
        }
        _ => panic!("Expected Multiple variant"),
    }
}

#[test]
fn test_fieldless_struct() {
    let original = FieldlessStruct;

    let written = Time::from_et_seconds(10000.0);
    let read = original.to_read(written);
    let now = written + 1.seconds();

    // Fieldless types should use simple implementation
    let evolved = FieldlessStruct::from_read(read, now);
    assert!(matches!(evolved, FieldlessStruct));

    let sample = FieldlessStruct::sample(read, now);
    assert!(matches!(sample, FieldlessStruct));
}

#[test]
fn test_fieldless_enum() {
    let original = FieldlessEnum::Unit;

    let written = Time::from_et_seconds(11000.0);
    let read = original.to_read(written);
    let now = written + 1.seconds();

    // Fieldless types should use simple implementation
    let evolved = FieldlessEnum::from_read(read, now);
    assert!(matches!(evolved, FieldlessEnum::Unit));

    let sample = FieldlessEnum::sample(read, now);
    assert!(matches!(sample, FieldlessEnum::Unit));
}

#[test]
fn test_public_struct() {
    let original = PublicStruct {
        value: Linear::new(1.seconds(), 8.0, 2.0),
        count: 15,
    };

    let written = Time::from_et_seconds(12000.0);
    let read = original.to_read(written);
    let now = written + 1.0.seconds();

    // Test that public visibility is preserved
    let evolved = PublicStruct::from_read(read, now);
    // Value should have evolved: 8.0 + 2.0 * 1.0 = 10.0
    assert!((evolved.value.value - 10.0).abs() < 1e-10);
    assert_eq!(evolved.count, 15);

    let sample = PublicStruct::sample(read, now);
    assert!((sample.value.value - 10.0).abs() < 1e-10);
    assert_eq!(sample.count, 15);
}

#[test]
fn test_multiple_evolution_steps() {
    let original = TestStruct {
        value: Linear::new(1.seconds(), 0.0, 1.0), // starts at 0, slope of 1
        count: 1,
    };

    let written = Time::from_et_seconds(13000.0);
    let read = original.to_read(written);

    // Test multiple evolution steps
    let step1 = written + 1.seconds();
    let evolved1 = TestStruct::from_read(read, step1);
    assert!((evolved1.value.value - 1.0).abs() < 1e-10);

    let step2 = written + 2.seconds();
    let evolved2 = TestStruct::from_read(read, step2);
    assert!((evolved2.value.value - 2.0).abs() < 1e-10);

    let step3 = written + 3.seconds();
    let evolved3 = TestStruct::from_read(read, step3);
    assert!((evolved3.value.value - 3.0).abs() < 1e-10);

    // Verify that count doesn't evolve
    assert_eq!(evolved1.count, 1);
    assert_eq!(evolved2.count, 1);
    assert_eq!(evolved3.count, 1);
}

#[test]
fn test_sample_consistency() {
    let original = TestStruct {
        value: Linear::new(1.seconds(), 5.0, 1.0),
        count: 10,
    };

    let written = Time::from_et_seconds(14000.0);
    let read = original.to_read(written);
    let now = written + 2.0.seconds();

    // Sample and from_read should produce consistent results
    let sample = TestStruct::sample(read, now);
    let evolved = TestStruct::from_read(read, now);

    assert!((sample.value.value - evolved.value.value).abs() < 1e-10);
    assert_eq!(sample.count, evolved.count);
}
