use std::{
    collections::hash_map::DefaultHasher,
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
};

const DEFAULT_NAMESPACE: &str = "minecraft";

#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) struct Identifier {
    namespace: IdentifierPart,
    path: IdentifierPart,
}

#[derive(Clone, Debug)]
pub(crate) struct IdentifierPart {
    value: Arc<str>,
    hash: u64,
}

impl IdentifierPart {
    pub(crate) fn new(value: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        Self {
            value: Arc::from(value),
            hash: hasher.finish(),
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.value
    }
}

impl PartialEq for IdentifierPart {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.value, &other.value) || self.value == other.value
    }
}

impl Eq for IdentifierPart {}

impl Hash for IdentifierPart {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl Identifier {
    pub(crate) fn parse(input: &str) -> Option<Self> {
        let (namespace, path) = match input.split_once(':') {
            Some(("", path)) => (DEFAULT_NAMESPACE, path),
            Some((namespace, path)) => (namespace, path),
            None => (DEFAULT_NAMESPACE, input),
        };
        Self::from_parts(namespace, path)
    }

    pub(crate) fn from_parts(namespace: &str, path: &str) -> Option<Self> {
        is_valid_namespace(namespace)
            .then_some(())
            .filter(|()| is_valid_path(path))
            .map(|()| Self {
                namespace: IdentifierPart::new(namespace),
                path: IdentifierPart::new(path),
            })
    }

    pub(crate) fn namespace(&self) -> &str {
        self.namespace.as_str()
    }

    pub(crate) fn path(&self) -> &str {
        self.path.as_str()
    }

    pub(crate) fn namespace_key(&self) -> &IdentifierPart {
        &self.namespace
    }

    pub(crate) fn path_key(&self) -> &IdentifierPart {
        &self.path
    }

    pub(crate) fn with_path(&self, path: &str) -> Option<Self> {
        is_valid_path(path).then(|| Self {
            namespace: self.namespace.clone(),
            path: IdentifierPart::new(path),
        })
    }

    pub(crate) fn into_parts(self) -> (IdentifierPart, IdentifierPart) {
        (self.namespace, self.path)
    }
}

impl fmt::Debug for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Identifier")
            .field("namespace", &self.namespace())
            .field("path", &self.path())
            .finish()
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.namespace(), self.path())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FunctionReference {
    Function(Identifier),
    Tag(Identifier),
}

impl FunctionReference {
    pub(crate) fn parse(input: &str) -> Option<Self> {
        input.strip_prefix('#').map_or_else(
            || Identifier::parse(input).map(Self::Function),
            |id| Identifier::parse(id).map(Self::Tag),
        )
    }
}

impl fmt::Display for FunctionReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Function(id) => id.fmt(formatter),
            Self::Tag(id) => write!(formatter, "#{id}"),
        }
    }
}

pub(crate) fn is_allowed_in_identifier(unit: u16) -> bool {
    matches!(
        unit,
        0x30..=0x39 | 0x61..=0x7a | 0x5f | 0x3a | 0x2f | 0x2e | 0x2d
    )
}

fn is_valid_namespace(namespace: &str) -> bool {
    namespace != ".."
        && namespace.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-')
        })
}

fn is_valid_path(path: &str) -> bool {
    path.bytes().all(|byte| {
        byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || matches!(byte, b'_' | b'/' | b'.' | b'-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_match_minecraft_boundaries() {
        assert_eq!(
            Identifier::parse("foo").unwrap().to_string(),
            "minecraft:foo"
        );
        assert_eq!(
            Identifier::parse(":foo").unwrap().to_string(),
            "minecraft:foo"
        );
        assert_eq!(Identifier::parse("").unwrap().to_string(), "minecraft:");
        assert!(Identifier::parse("a:b:c").is_none());
        assert!(Identifier::parse("UPPER:path").is_none());
        assert!(Identifier::parse("..:path").is_none());
        assert!(Identifier::parse("a:../path").is_some());
    }
}
