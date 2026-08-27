use std::fmt;

const DEFAULT_NAMESPACE: &str = "minecraft";

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Identifier {
    namespace: String,
    path: String,
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
                namespace: namespace.to_owned(),
                path: path.to_owned(),
            })
    }

    pub(crate) fn namespace(&self) -> &str {
        &self.namespace
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.namespace, self.path)
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
