use worldless::{ExecutionContext, Position, Rotation};

pub fn context() -> ExecutionContext {
    ExecutionContext::new(Position::new(0.0, 0.0, 0.0), Rotation::new(0.0, 0.0))
}
