//! Brigadier-compatible command parsing for the worldless VM.
//!
//! Behavior tracks Mojang Brigadier 1.3.11 at revision
//! `9ba4f13c0fe82b07c08c2dc2d8043f075ffd0d98`. String positions are Java UTF-16
//! code-unit offsets; the `*_utf16` APIs preserve unpaired surrogates. Nodes,
//! builders, sources, and callbacks use [`std::rc::Rc`] so Java reference identity
//! can be retained by the VM without imposing thread-safety bounds.

mod java_case;
mod java_hash_set;

pub mod arguments;
pub mod builder;
pub mod context;
pub mod dispatcher;
pub mod exceptions;
pub mod message;
pub mod reader;
pub mod suggestion;
pub mod tree;

pub use builder::SingleRedirectModifier;
pub use context::{CommandContext, ResultConsumer};
pub use dispatcher::{CommandDispatcher, ParseResults};
pub use message::{LiteralMessage, Message, MessageRef};
pub use reader::{ImmutableStringReader, StringReader};
pub use tree::{AmbiguityConsumer, Command, Node, RedirectModifier, SINGLE_SUCCESS};
