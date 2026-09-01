use std::f64::consts::PI;

/// The invocation-local spatial values used by supported command execution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExecutionContext {
    position: Position,
    rotation: Rotation,
}

impl ExecutionContext {
    /// Creates an execution context from an explicit position and rotation.
    pub const fn new(position: Position, rotation: Rotation) -> Self {
        Self { position, rotation }
    }

    /// Returns the current position.
    pub const fn position(self) -> Position {
        self.position
    }

    /// Returns the current rotation.
    pub const fn rotation(self) -> Rotation {
        self.rotation
    }
}

/// A resolved command-source position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    x: f64,
    y: f64,
    z: f64,
}

impl Position {
    /// Creates a position from its three coordinates.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub const fn x(self) -> f64 {
        self.x
    }

    pub const fn y(self) -> f64 {
        self.y
    }

    pub const fn z(self) -> f64 {
        self.z
    }
}

/// A resolved command-source rotation in Minecraft command order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    yaw: f32,
    pitch: f32,
}

impl Rotation {
    /// Creates a rotation from yaw followed by pitch.
    pub const fn new(yaw: f32, pitch: f32) -> Self {
        Self { yaw, pitch }
    }

    pub const fn yaw(self) -> f32 {
        self.yaw
    }

    pub const fn pitch(self) -> f32 {
        self.pitch
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WorldCoordinate {
    pub(crate) relative: bool,
    pub(crate) value: f64,
}

impl WorldCoordinate {
    fn resolve_f64(self, original: f64) -> f64 {
        if self.relative {
            self.value + original
        } else {
            self.value
        }
    }

    fn resolve_f32(self, original: f32) -> f32 {
        if self.relative {
            (self.value + f64::from(original)) as f32
        } else {
            self.value as f32
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PositionCoordinates {
    World {
        x: WorldCoordinate,
        y: WorldCoordinate,
        z: WorldCoordinate,
    },
    Local {
        left: f64,
        up: f64,
        forwards: f64,
    },
}

impl PositionCoordinates {
    fn resolve(self, context: ExecutionContext) -> Position {
        match self {
            Self::World { x, y, z } => Position::new(
                x.resolve_f64(context.position.x),
                y.resolve_f64(context.position.y),
                z.resolve_f64(context.position.z),
            ),
            Self::Local { left, up, forwards } => {
                let degrees_to_radians = (PI / 180.0) as f32;
                let yaw = context.rotation.yaw;
                let pitch = context.rotation.pitch;
                let yaw_cos = mth_cos(f64::from((yaw + 90.0) * degrees_to_radians));
                let yaw_sin = mth_sin(f64::from((yaw + 90.0) * degrees_to_radians));
                let pitch_cos = mth_cos(f64::from(-pitch * degrees_to_radians));
                let pitch_sin = mth_sin(f64::from(-pitch * degrees_to_radians));
                let up_cos = mth_cos(f64::from((-pitch + 90.0) * degrees_to_radians));
                let up_sin = mth_sin(f64::from((-pitch + 90.0) * degrees_to_radians));

                let forward = Position::new(
                    f64::from(yaw_cos * pitch_cos),
                    f64::from(pitch_sin),
                    f64::from(yaw_sin * pitch_cos),
                );
                let up_vector = Position::new(
                    f64::from(yaw_cos * up_cos),
                    f64::from(up_sin),
                    f64::from(yaw_sin * up_cos),
                );
                let left_vector = cross(forward, up_vector, -1.0);
                let x_offset = forward.x * forwards + up_vector.x * up + left_vector.x * left;
                let y_offset = forward.y * forwards + up_vector.y * up + left_vector.y * left;
                let z_offset = forward.z * forwards + up_vector.z * up + left_vector.z * left;
                Position::new(
                    context.position.x + x_offset,
                    context.position.y + y_offset,
                    context.position.z + z_offset,
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RotationCoordinates {
    pub(crate) yaw: WorldCoordinate,
    pub(crate) pitch: WorldCoordinate,
}

impl RotationCoordinates {
    fn resolve(self, context: ExecutionContext) -> Rotation {
        Rotation::new(
            self.yaw.resolve_f32(context.rotation.yaw),
            self.pitch.resolve_f32(context.rotation.pitch),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Axes {
    pub(crate) x: bool,
    pub(crate) y: bool,
    pub(crate) z: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ContextTransform {
    Positioned(PositionCoordinates),
    Rotated(RotationCoordinates),
    Facing(PositionCoordinates),
    Align(Axes),
    Anchored,
}

impl ContextTransform {
    pub(crate) fn apply(self, context: &mut ExecutionContext) {
        match self {
            Self::Positioned(coordinates) => context.position = coordinates.resolve(*context),
            Self::Rotated(coordinates) => context.rotation = coordinates.resolve(*context),
            Self::Facing(coordinates) => {
                let target = coordinates.resolve(*context);
                let x_delta = target.x - context.position.x;
                let y_delta = target.y - context.position.y;
                let z_delta = target.z - context.position.z;
                let horizontal = libm::sqrt(x_delta * x_delta + z_delta * z_delta);
                let pi = f64::from(PI as f32);
                let pitch = (-(mth_atan2(y_delta, horizontal) * 180.0 / pi)) as f32;
                let yaw = (mth_atan2(z_delta, x_delta) * 180.0 / pi) as f32 - 90.0;
                context.rotation = Rotation::new(wrap_degrees(yaw), wrap_degrees(pitch));
            }
            Self::Align(axes) => {
                context.position = Position::new(
                    if axes.x {
                        minecraft_floor(context.position.x)
                    } else {
                        context.position.x
                    },
                    if axes.y {
                        minecraft_floor(context.position.y)
                    } else {
                        context.position.y
                    },
                    if axes.z {
                        minecraft_floor(context.position.z)
                    } else {
                        context.position.z
                    },
                );
            }
            // With no source entity, both Minecraft anchors resolve to the
            // source position and are indistinguishable to supported behavior.
            Self::Anchored => {}
        }
    }
}

fn cross(left: Position, right: Position, scale: f64) -> Position {
    Position::new(
        (left.y * right.z - left.z * right.y) * scale,
        (left.z * right.x - left.x * right.z) * scale,
        (left.x * right.y - left.y * right.x) * scale,
    )
}

fn minecraft_floor(value: f64) -> f64 {
    (value.floor() as i32) as f64
}

fn wrap_degrees(angle: f32) -> f32 {
    let mut normalized = angle % 360.0;
    if normalized >= 180.0 {
        normalized -= 360.0;
    }
    if normalized < -180.0 {
        normalized += 360.0;
    }
    normalized
}

pub(crate) fn mth_sin(value: f64) -> f32 {
    let index = ((value * 10_430.378_350_470_453) as i64 & 65_535) as u16;
    sine_table_value(index)
}

pub(crate) fn mth_cos(value: f64) -> f32 {
    let index = ((value * 10_430.378_350_470_453 + 16_384.0) as i64 & 65_535) as u16;
    sine_table_value(index)
}

fn sine_table_value(index: u16) -> f32 {
    libm::sin(f64::from(index) * PI * 2.0 / 65_536.0) as f32
}

fn mth_atan2(mut y: f64, mut x: f64) -> f64 {
    let squared_length = x * x + y * y;
    if squared_length.is_nan() {
        return f64::NAN;
    }

    let negative_y = y < 0.0;
    if negative_y {
        y = -y;
    }
    let negative_x = x < 0.0;
    if negative_x {
        x = -x;
    }
    let steep = y > x;
    if steep {
        std::mem::swap(&mut x, &mut y);
    }

    let inverse_length = fast_inverse_sqrt(squared_length);
    x *= inverse_length;
    y *= inverse_length;
    let fraction_bias = f64::from_bits(4_805_340_802_404_319_232);
    let biased_y = fraction_bias + y;
    let index = biased_y.to_bits() as u32 as usize;
    let fraction = index as f64 / 256.0;
    let phi = libm::asin(fraction);
    let cosine = libm::cos(phi);
    let table_y = biased_y - fraction_bias;
    let delta = y * cosine - x * table_y;
    let correction = (6.0 + delta * delta) * delta * (1.0 / 6.0);
    let mut theta = phi + correction;
    if steep {
        theta = PI / 2.0 - theta;
    }
    if negative_x {
        theta = PI - theta;
    }
    if negative_y {
        theta = -theta;
    }
    theta
}

fn fast_inverse_sqrt(value: f64) -> f64 {
    let half = 0.5 * value;
    let bits = 6_910_469_410_427_058_090_i64.wrapping_sub((value.to_bits() as i64) >> 1);
    let approximation = f64::from_bits(bits as u64);
    approximation * (1.5 - half * approximation * approximation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_and_rotation_coordinates_resolve_after_previous_transforms() {
        let mut context =
            ExecutionContext::new(Position::new(10.0, 20.0, 30.0), Rotation::new(45.0, -10.0));
        ContextTransform::Positioned(PositionCoordinates::World {
            x: WorldCoordinate {
                relative: true,
                value: 1.0,
            },
            y: WorldCoordinate {
                relative: false,
                value: 2.0,
            },
            z: WorldCoordinate {
                relative: true,
                value: -3.0,
            },
        })
        .apply(&mut context);
        ContextTransform::Rotated(RotationCoordinates {
            yaw: WorldCoordinate {
                relative: true,
                value: 15.0,
            },
            pitch: WorldCoordinate {
                relative: false,
                value: 5.0,
            },
        })
        .apply(&mut context);

        assert_eq!(context.position(), Position::new(11.0, 2.0, 27.0));
        assert_eq!(context.rotation(), Rotation::new(60.0, 5.0));
    }

    #[test]
    fn align_uses_minecrafts_int_floor_conversion() {
        let mut context = ExecutionContext::new(
            Position::new(f64::NAN, -1.25, f64::MAX),
            Rotation::new(0.0, 0.0),
        );
        ContextTransform::Align(Axes {
            x: true,
            y: true,
            z: true,
        })
        .apply(&mut context);
        assert_eq!(
            context.position(),
            Position::new(0.0, -2.0, f64::from(i32::MAX))
        );
    }
}
