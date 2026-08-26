use std::any::Any;
use std::fmt;
use std::rc::Rc;

/// Text carried by Brigadier diagnostics and suggestion tooltips.
pub trait Message: Any + fmt::Debug + fmt::Display {
    fn string(&self) -> String;

    fn equals(&self, other: &dyn Message) -> bool;

    /// Java-compatible `hashCode`; override this whenever [`Self::equals`] is structural.
    fn hash_code(&self) -> i32 {
        let address = self as *const Self as *const () as usize;
        let folded = if usize::BITS > 32 {
            address ^ (address >> 32)
        } else {
            address
        };
        (folded as i32) & i32::MAX
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
