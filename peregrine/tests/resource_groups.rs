use peregrine::{Resource, resource};

// Test basic resource group with shared default
resource! {
    pub heater_*_active: bool = false; {a, b, c}
}

#[test]
fn test_resource_group_with_shared_default() {
    // Check that all three resources are generated with the shared default
    assert_eq!(heater_a_active::initial_condition(), Some(false));
    assert_eq!(heater_b_active::initial_condition(), Some(false));
    assert_eq!(heater_c_active::initial_condition(), Some(false));

    // Check that the resources have different IDs
    assert_ne!(heater_a_active::ID, heater_b_active::ID);
    assert_ne!(heater_b_active::ID, heater_c_active::ID);
    assert_ne!(heater_a_active::ID, heater_c_active::ID);

    // Check that the resources have correct labels
    assert_eq!(heater_a_active::LABEL, "heater_a_active");
    assert_eq!(heater_b_active::LABEL, "heater_b_active");
    assert_eq!(heater_c_active::LABEL, "heater_c_active");
}

// Test resource group with individual defaults
resource! {
    pub sensor_*_calibrated: bool; {
        temperature: true,
        pressure: false,
        humidity: true
    }
}

#[test]
fn test_resource_group_with_individual_defaults() {
    // Check that each resource has its individual default
    assert_eq!(
        sensor_temperature_calibrated::initial_condition(),
        Some(true)
    );
    assert_eq!(sensor_pressure_calibrated::initial_condition(), Some(false));
    assert_eq!(sensor_humidity_calibrated::initial_condition(), Some(true));

    // Check that the resources have different IDs
    assert_ne!(
        sensor_temperature_calibrated::ID,
        sensor_pressure_calibrated::ID
    );
    assert_ne!(
        sensor_pressure_calibrated::ID,
        sensor_humidity_calibrated::ID
    );
    assert_ne!(
        sensor_temperature_calibrated::ID,
        sensor_humidity_calibrated::ID
    );

    // Check labels
    assert_eq!(
        sensor_temperature_calibrated::LABEL,
        "sensor_temperature_calibrated"
    );
    assert_eq!(
        sensor_pressure_calibrated::LABEL,
        "sensor_pressure_calibrated"
    );
    assert_eq!(
        sensor_humidity_calibrated::LABEL,
        "sensor_humidity_calibrated"
    );
}

// Test resource group without any defaults (individual only)
resource! {
    pub valve_*_position: f32; {
        inlet: 0.0,
        outlet: 1.0,
        bypass: 0.5
    }
}

#[test]
fn test_resource_group_different_individual_defaults() {
    // Check individual defaults with different values
    assert_eq!(valve_inlet_position::initial_condition(), Some(0.0));
    assert_eq!(valve_outlet_position::initial_condition(), Some(1.0));
    assert_eq!(valve_bypass_position::initial_condition(), Some(0.5));
}

// Test asterisk at the beginning
resource! {
    pub *_pump_enabled: bool = true; {primary, secondary, backup}
}

#[test]
fn test_resource_group_asterisk_at_beginning() {
    assert_eq!(primary_pump_enabled::initial_condition(), Some(true));
    assert_eq!(secondary_pump_enabled::initial_condition(), Some(true));
    assert_eq!(backup_pump_enabled::initial_condition(), Some(true));

    assert_eq!(primary_pump_enabled::LABEL, "primary_pump_enabled");
    assert_eq!(secondary_pump_enabled::LABEL, "secondary_pump_enabled");
    assert_eq!(backup_pump_enabled::LABEL, "backup_pump_enabled");
}

// Test asterisk at the end
resource! {
    pub thruster_*: f32 = 0.0; {x, y, z}
}

#[test]
fn test_resource_group_asterisk_at_end() {
    assert_eq!(thruster_x::initial_condition(), Some(0.0));
    assert_eq!(thruster_y::initial_condition(), Some(0.0));
    assert_eq!(thruster_z::initial_condition(), Some(0.0));

    assert_eq!(thruster_x::LABEL, "thruster_x");
    assert_eq!(thruster_y::LABEL, "thruster_y");
    assert_eq!(thruster_z::LABEL, "thruster_z");
}

// Test multiple groups and single resources in one call
resource! {
    // Single resource
    pub single_flag: bool = false;

    // First group with shared default
    pub engine_*_temp: f32 = 0.0; {main, backup}

    // Second group with individual defaults
    pub motor_*_speed: i32; {
        left: 100,
        right: 150
    }

    // Another single resource
    pub counter: u32 = 42;

    // Third group
    pub light_*_brightness: f32 = 1.0; {red, green, blue}
}

#[test]
fn test_multiple_groups_in_one_call() {
    // Test single resources
    assert_eq!(single_flag::initial_condition(), Some(false));
    assert_eq!(counter::initial_condition(), Some(42));

    // Test first group (shared default)
    assert_eq!(engine_main_temp::initial_condition(), Some(0.0));
    assert_eq!(engine_backup_temp::initial_condition(), Some(0.0));
    assert_eq!(engine_main_temp::LABEL, "engine_main_temp");
    assert_eq!(engine_backup_temp::LABEL, "engine_backup_temp");

    // Test second group (individual defaults)
    assert_eq!(motor_left_speed::initial_condition(), Some(100));
    assert_eq!(motor_right_speed::initial_condition(), Some(150));
    assert_eq!(motor_left_speed::LABEL, "motor_left_speed");
    assert_eq!(motor_right_speed::LABEL, "motor_right_speed");

    // Test third group (shared default)
    assert_eq!(light_red_brightness::initial_condition(), Some(1.0));
    assert_eq!(light_green_brightness::initial_condition(), Some(1.0));
    assert_eq!(light_blue_brightness::initial_condition(), Some(1.0));
    assert_eq!(light_red_brightness::LABEL, "light_red_brightness");
    assert_eq!(light_green_brightness::LABEL, "light_green_brightness");
    assert_eq!(light_blue_brightness::LABEL, "light_blue_brightness");

    // Verify all resources have unique IDs
    assert_ne!(single_flag::ID, counter::ID);
    assert_ne!(engine_main_temp::ID, engine_backup_temp::ID);
    assert_ne!(motor_left_speed::ID, motor_right_speed::ID);
    assert_ne!(light_red_brightness::ID, light_green_brightness::ID);
    assert_ne!(light_green_brightness::ID, light_blue_brightness::ID);
    assert_ne!(light_red_brightness::ID, light_blue_brightness::ID);
}
