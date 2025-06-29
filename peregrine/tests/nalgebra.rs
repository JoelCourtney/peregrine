#![cfg(feature = "nalgebra")]

use anyhow::Result;
use nalgebra::{
    DMatrix, DVector, Matrix2, Matrix2x3, Matrix3, Matrix3x2, Matrix3x4, Matrix4, Matrix4x3,
    Quaternion, Rotation2, Rotation3, UnitComplex, UnitQuaternion, Vector2, Vector3, Vector4,
};
use peregrine::*;
use peregrine_macros::op;
use serde::{Deserialize, Serialize};
use std::hash::Hash;

// Test models with various nalgebra types
model! {
    StaticMatrixModel {
        matrix_2x2: Matrix2<f64>,
        matrix_3x3: Matrix3<f64>,
        matrix_4x4: Matrix4<f64>,
        matrix_3x2: Matrix3x2<f64>,
        matrix_2x3: Matrix2x3<f64>,
        matrix_3x4: Matrix3x4<f64>,
        matrix_4x3: Matrix4x3<f64>,
        vector_2: Vector2<f64>,
        vector_3: Vector3<f64>,
        vector_4: Vector4<f64>,
    }
}

model! {
    DynamicMatrixModel {
        dynamic_matrix: DMatrix<f64>,
        dynamic_vector: DVector<f64>,
    }
}

model! {
    QuaternionModel {
        quaternion: Quaternion<f64>,
        unit_quaternion: UnitQuaternion<f64>,
    }
}

model! {
    RotationModel {
        rotation_2d: Rotation2<f64>,
        rotation_3d: Rotation3<f64>,
        unit_complex: UnitComplex<f64>,
    }
}

// Test activities for matrix operations
#[derive(Hash, Serialize, Deserialize)]
pub struct MatrixOperations;

#[typetag::serde]
impl Activity for MatrixOperations {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        // Test matrix operations
        ops += op! {
            ref mut: matrix_2x2 = ref:matrix_2x2 * ref:matrix_2x2;
            ref mut: matrix_3x3 = ref:matrix_3x3 * ref:matrix_3x3;
            ref mut: vector_2 = ref:matrix_2x2 * ref:vector_2;
            ref mut: vector_3 = ref:matrix_3x3 * ref:vector_3;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Hash, Serialize, Deserialize)]
pub struct DynamicMatrixOperations;

#[typetag::serde]
impl Activity for DynamicMatrixOperations {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        // Test dynamic matrix operations
        ops += op! {
            ref mut: dynamic_matrix = &ref:dynamic_matrix * &ref:dynamic_matrix;
            ref mut: dynamic_vector = &ref:dynamic_matrix * &ref:dynamic_vector;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Hash, Serialize, Deserialize)]
pub struct QuaternionOperations;

#[typetag::serde]
impl Activity for QuaternionOperations {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        // Test quaternion operations
        ops += op! {
            ref mut: quaternion = ref:quaternion * ref:quaternion;
            ref mut: unit_quaternion = ref:unit_quaternion * ref:unit_quaternion;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Hash, Serialize, Deserialize)]
pub struct RotationOperations;

#[typetag::serde]
impl Activity for RotationOperations {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        // Test rotation operations
        ops += op! {
            ref mut: rotation_2d = ref:rotation_2d * ref:rotation_2d;
            ref mut: rotation_3d = ref:rotation_3d * ref:rotation_3d;
            ref mut: unit_complex = ref:unit_complex * ref:unit_complex;
        };

        Ok(Duration::ZERO)
    }
}

// Helper functions for creating test data
fn create_test_matrix_2x2() -> Matrix2<f64> {
    Matrix2::new(1.0, 2.0, 3.0, 4.0)
}

fn create_test_matrix_3x3() -> Matrix3<f64> {
    Matrix3::new(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0)
}

fn create_test_matrix_4x4() -> Matrix4<f64> {
    Matrix4::new(
        1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
    )
}

fn create_test_dynamic_matrix() -> DMatrix<f64> {
    DMatrix::from_row_slice(3, 3, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0])
}

fn create_test_quaternion() -> Quaternion<f64> {
    Quaternion::new(1.0, 2.0, 3.0, 4.0)
}

fn create_test_unit_quaternion() -> UnitQuaternion<f64> {
    UnitQuaternion::identity()
}

fn create_test_rotation_2d() -> Rotation2<f64> {
    Rotation2::new(std::f64::consts::FRAC_PI_4)
}

fn create_test_rotation_3d() -> Rotation3<f64> {
    Rotation3::identity()
}

// Test functions
#[test]
fn test_static_matrix_operations() -> Result<()> {
    let session = Session::new();
    let mut plan = session.new_plan::<StaticMatrixModel>(
        Time::from_tai_seconds(0.0),
        initial_conditions! {
            matrix_2x2: create_test_matrix_2x2(),
            matrix_3x3: create_test_matrix_3x3(),
            matrix_4x4: create_test_matrix_4x4(),
            matrix_3x2: Matrix3x2::new(1.0, 2.0, 3.0, 4.0, 5.0, 6.0),
            matrix_2x3: Matrix2x3::new(1.0, 2.0, 3.0, 4.0, 5.0, 6.0),
            matrix_3x4: Matrix3x4::new(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0),
            matrix_4x3: Matrix4x3::new(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0),
            vector_2: Vector2::new(1.0, 2.0),
            vector_3: Vector3::new(1.0, 2.0, 3.0),
            vector_4: Vector4::new(1.0, 2.0, 3.0, 4.0),
        }
    );

    plan.insert(Time::from_tai_seconds(1.0), MatrixOperations)?;
    plan.sample::<vector_4>(Time::from_tai_seconds(2.0))?;

    Ok(())
}

#[test]
fn test_dynamic_matrix_operations() -> Result<()> {
    let session = Session::new();
    let mut plan = session.new_plan::<DynamicMatrixModel>(
        Time::from_tai_seconds(0.0),
        initial_conditions! {
            dynamic_matrix: create_test_dynamic_matrix(),
            dynamic_vector: DVector::from_row_slice(&[1.0, 2.0, 3.0]),
        },
    );

    plan.insert(Time::from_tai_seconds(1.0), DynamicMatrixOperations)?;
    plan.sample::<dynamic_vector>(Time::from_tai_seconds(2.0))?;

    Ok(())
}

#[test]
fn test_quaternion_operations() -> Result<()> {
    let session = Session::new();
    let mut plan = session.new_plan::<QuaternionModel>(
        Time::from_tai_seconds(0.0),
        initial_conditions! {
            quaternion: create_test_quaternion(),
            unit_quaternion: create_test_unit_quaternion(),
        },
    );

    plan.insert(Time::from_tai_seconds(1.0), QuaternionOperations)?;
    plan.sample::<unit_quaternion>(Time::from_tai_seconds(2.0))?;

    Ok(())
}

#[test]
fn test_rotation_operations() -> Result<()> {
    let session = Session::new();
    let mut plan = session.new_plan::<RotationModel>(
        Time::from_tai_seconds(0.0),
        initial_conditions! {
            rotation_2d: create_test_rotation_2d(),
            rotation_3d: create_test_rotation_3d(),
            unit_complex: UnitComplex::new(std::f64::consts::FRAC_PI_4),
        },
    );

    plan.insert(Time::from_tai_seconds(1.0), RotationOperations)?;
    plan.sample::<unit_complex>(Time::from_tai_seconds(2.0))?;

    Ok(())
}

#[test]
fn test_matrix_data_trait() {
    let matrix = create_test_matrix_2x2();
    let time = Time::from_tai_seconds(0.0);

    // Test Data trait implementation
    let read = matrix.to_read(time);
    let from_read = Matrix2::<f64>::from_read(read, time);
    let sample = Matrix2::<f64>::sample(read, time);

    assert_eq!(matrix, from_read);
    assert_eq!(matrix, sample);
}

#[test]
fn test_quaternion_data_trait() {
    let quaternion = create_test_quaternion();
    let time = Time::from_tai_seconds(0.0);

    // Test Data trait implementation
    let read = quaternion.to_read(time);
    let from_read = Quaternion::<f64>::from_read(read, time);
    let sample = Quaternion::<f64>::sample(read, time);

    assert_eq!(quaternion, from_read);
    assert_eq!(quaternion, sample);
}

#[test]
fn test_rotation_data_trait() {
    let rotation = create_test_rotation_2d();
    let time = Time::from_tai_seconds(0.0);

    // Test Data trait implementation
    let read = rotation.to_read(time);
    let from_read = Rotation2::<f64>::from_read(read, time);
    let sample = Rotation2::<f64>::sample(read, time);

    assert_eq!(rotation, from_read);
    assert_eq!(rotation, sample);
}

#[test]
fn test_dynamic_matrix_data_trait() {
    let matrix = create_test_dynamic_matrix();
    let time = Time::from_tai_seconds(0.0);

    // Test Data trait implementation for dynamic matrices
    let read = matrix.to_read(time);
    let from_read = DMatrix::<f64>::from_read(read, time);
    let sample = DMatrix::<f64>::sample(read, time);

    assert_eq!(matrix, from_read);
    assert_eq!(matrix, sample);
}

#[test]
fn test_unit_quaternion() {
    let quaternion = UnitQuaternion::identity();
    let time = Time::from_tai_seconds(0.0);

    let read = quaternion.to_read(time);
    let from_read = UnitQuaternion::<f64>::from_read(read, time);

    assert_eq!(quaternion, from_read);
    assert!(quaternion.is_hashable());
}

#[test]
fn test_identity_rotation() {
    let rotation = Rotation2::<f64>::identity();
    let time = Time::from_tai_seconds(0.0);

    let read = rotation.to_read(time);
    let from_read = Rotation2::<f64>::from_read(read, time);

    assert_eq!(rotation, from_read);
    assert!(rotation.is_hashable());
}
