use std::any::Any;
use std::fmt;
use std::rc::Rc;

/// Text carried by Brigadier diagnostics and suggestion tooltips.
pub trait Message: Any + fmt::Debug + fmt::Display {
    fn string(&self) -> String;

    fn equals(&self, other: &dyn Message) -> bool;

    /// Deterministic default permitted by `Object.hashCode`; override it when a
    /// message type specifies a concrete hash function.
    fn hash_code(&self) -> i32 {
        0
    }

    fn as_any(&self) -> &dyn Any;
}

pub type MessageRef = Rc<dyn Message>;

#[derive(Debug)]
pub struct LiteralMessage(String);

impl LiteralMessage {
    pub fn new(string: impl Into<String>) -> Self {
        Self(string.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Message for LiteralMessage {
    fn string(&self) -> String {
        self.0.clone()
    }

    fn equals(&self, other: &dyn Message) -> bool {
        std::ptr::eq(self as &dyn Message, other)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl fmt::Display for LiteralMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hash_code_is_deterministic() {
        assert_eq!(LiteralMessage::new("first").hash_code(), 0);
        assert_eq!(LiteralMessage::new("second").hash_code(), 0);
    }
}
