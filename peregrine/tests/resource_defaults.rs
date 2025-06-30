use hifitime::Epoch;
use peregrine::internal::macro_prelude::InitialConditions;
use peregrine::{Data, MaybeHash, Resource, Session, model, resource};
use serde::{Deserialize, Serialize};

resource! {
    pub test_resource: u32 = 42;
}

#[test]
fn test_resource_default_value() {
    let default_val = test_resource::initial_condition();
    assert_eq!(default_val, Some(42));
}

resource! {
    pub no_default_resource: u32;
}

#[test]
fn test_resource_no_default_returns_none() {
    let default_val = no_default_resource::initial_condition();
    assert_eq!(default_val, None);
}

// Test with complex default expression
resource! {
    pub complex_resource: String = "Hello World".to_string();
}

#[test]
fn test_complex_default_value() {
    let default_val = complex_resource::initial_condition();
    assert_eq!(default_val, Some("Hello World".to_string()));
}

// Test with custom struct
#[derive(Debug, Clone, PartialEq, Data, MaybeHash, Serialize, Deserialize)]
pub struct CustomData {
    pub value: i32,
    pub name: String,
}

resource! {
    pub custom_resource: CustomData = CustomData {
        value: 100,
        name: "test".to_string(),
    };
}

#[test]
fn test_custom_struct_default() {
    let default_val = custom_resource::initial_condition().unwrap();
    assert_eq!(default_val.value, 100);
    assert_eq!(default_val.name, "test");
}

// Test model macro with default values
model! {
    pub TestModel {
        pub resource_with_inish_condish: u32 = 123;
        pub resource_with_default: f64;
        pub resource_without_default: CustomData;
    }
}

#[test]
fn test_model_resource_defaults() {
    // Test that the resource with default can provide initial condition
    let default_val = resource_with_inish_condish::initial_condition();
    assert_eq!(default_val, Some(123));
}

#[test]
fn test_model_resource_no_default_returns_none() {
    // Test that the resource without default returns None
    let default_val = resource_with_default::initial_condition();
    assert_eq!(default_val, None);
}

#[test]
fn test_plan_creation_fails_without_initial_conditions() {
    // Test that plan creation fails when no defaults or initial conditions provided
    let session = Session::new();
    let initial_conditions = InitialConditions::new(); // Empty initial conditions
    let result = session.new_plan::<TestModel>(Epoch::from_tai_seconds(0.0), initial_conditions);
    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(error_msg.contains("resource_without_default"));
        assert!(error_msg.contains("No initial condition provided"));
    } else {
        panic!("expected plan creation to fail");
    }
}

#[test]
fn test_plan_creation_with_explicit_initial_conditions() {
    // Test that explicit initial conditions override defaults
    let session = Session::new();
    let initial_conditions =
        InitialConditions::new().insert::<resource_without_default>(CustomData {
            value: 5,
            name: "hi".to_string(),
        });
    let result = session.new_plan::<TestModel>(Epoch::from_tai_seconds(0.0), initial_conditions);
    assert!(result.is_ok());
}
