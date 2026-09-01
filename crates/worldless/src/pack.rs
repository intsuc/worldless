use std::path::{Path, PathBuf};

/// A resource kind accepted by an in-memory data pack.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResourceKind {
    Function,
    FunctionTag,
    ContextIntProvider,
    ContextIntProviderTag,
    ContextFloatProvider,
    ContextFloatProviderTag,
    Predicate,
    PredicateTag,
}

/// One target-ready logical resource in an in-memory data pack.
#[derive(Debug, Eq, PartialEq)]
pub struct MemoryResource {
    kind: ResourceKind,
    id: String,
    source: String,
}

impl MemoryResource {
    pub fn new(kind: ResourceKind, id: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
            source: source.into(),
        }
    }

    pub(crate) fn into_parts(self) -> (ResourceKind, String, String) {
        (self.kind, self.id, self.source)
    }
}

/// One data-pack input in a statically ordered pack stack.
#[derive(Debug, Eq, PartialEq)]
pub struct Pack {
    source: PackSource,
}

impl Pack {
    /// Uses an expanded directory data pack, including its `pack.mcmeta`.
    pub fn directory(path: impl AsRef<Path>) -> Self {
        Self {
            source: PackSource::Directory(path.as_ref().to_owned()),
        }
    }

    /// Uses target-ready logical resources without `pack.mcmeta` or file I/O.
    pub fn memory(resources: impl IntoIterator<Item = MemoryResource>) -> Self {
        Self {
            source: PackSource::Memory(resources.into_iter().collect()),
        }
    }

    pub(crate) fn into_source(self) -> PackSource {
        self.source
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PackSource {
    Directory(PathBuf),
    Memory(Vec<MemoryResource>),
}
