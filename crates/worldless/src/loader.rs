use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, HashSet},
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use serde_json::{Map, Value};
use worldless_brigadier::{
    Command, CommandDispatcher, LiteralMessage, SINGLE_SUCCESS, StringReader,
    arguments::{
        ArgumentType, BoolArgumentType, DoubleArgumentType, FloatArgumentType, IntegerArgumentType,
        StringArgumentType,
    },
    builder::{ArgumentBuilder, LiteralArgumentBuilder, RequiredArgumentBuilder},
    context::{CommandContext, ContextError},
    exceptions::{
        BUILT_IN_EXCEPTIONS, BuiltInExceptionProvider, CommandSyntaxException,
        SimpleCommandExceptionType,
    },
};

use crate::{
    execution_context::{
        Axes, ContextTransform, PositionCoordinates, RotationCoordinates, WorldCoordinate,
    },
    macro_function::{Function, FunctionBuilder, MAX_COMMAND_LENGTH},
    nbt::{CompoundTag, JavaString, NbtPath, Tag, parse_compound, parse_path, parse_tag},
    number_provider::{
        LootRegistry, NumberProviderReference, RegistryResource, RegistryValidationError,
        parse_inline_tag as parse_inline_number_provider, parse_json as parse_number_provider_json,
    },
    pack::{Pack, PackSource, ResourceKind},
    predicate::{
        PredicateReference, parse_inline_tag as parse_inline_predicate,
        parse_json as parse_predicate_json,
    },
    program::{
        Command as CompiledCommand, ComputeCommand, ComputeMode, DataCommand, DataModifyOperation,
        DataSource, DataStringSubstring, DoubleRange, FunctionArguments, Instruction, IntegerRange,
        Modifier, PredicateCondition, Program, RandomCommand, RandomSequenceSettings,
        ScoreComparison, ScoreCondition, ScoreHolderSet, ScorePredicate, ScoreReference,
        ScoreboardCommand, ScoreboardOperation, StopwatchCommand, StopwatchCondition,
        StorageCondition, StorageNumberType, StoreKind,
    },
    resource::{FunctionReference, Identifier, is_allowed_in_identifier},
    resource_json,
};

const TARGET_PACK_FORMAT: PackFormat = PackFormat {
    major: 118,
    minor: 0,
};
const LAST_PRE_MINOR_DATA_PACK_FORMAT: i32 = 81;

/// The source of a selected resource in a composed pack stack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceOrigin {
    Directory(PathBuf),
    Memory { pack: usize, id: String },
}

impl fmt::Display for ResourceOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Directory(path) => path.display().fmt(formatter),
            Self::Memory { pack, id } => {
                write!(formatter, "in-memory pack {pack} resource `{id}`")
            }
        }
    }
}

/// An error encountered while composing and compiling data packs.
#[derive(Debug)]
pub enum LoadError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    InvalidPack {
        path: PathBuf,
        reason: String,
    },
    UnsupportedPack {
        path: PathBuf,
        feature: &'static str,
    },
    InvalidMemoryResourceIdentifier {
        pack: usize,
        kind: ResourceKind,
        input: String,
    },
    DuplicateMemoryResource {
        pack: usize,
        kind: ResourceKind,
        id: String,
    },
    InvalidFunction {
        origin: ResourceOrigin,
        line: usize,
        reason: String,
    },
    InvalidFunctionTag {
        origin: ResourceOrigin,
        reason: String,
    },
    InvalidNumberProvider {
        origin: ResourceOrigin,
        reason: String,
    },
    InvalidNumberProviderTag {
        origin: ResourceOrigin,
        reason: String,
    },
    InvalidPredicate {
        origin: ResourceOrigin,
        reason: String,
    },
    InvalidPredicateTag {
        origin: ResourceOrigin,
        reason: String,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "failed to access {}: {source}", path.display())
            }
            Self::InvalidPack { path, reason } => {
                write!(
                    formatter,
                    "invalid data pack at {}: {reason}",
                    path.display()
                )
            }
            Self::UnsupportedPack { path, feature } => write!(
                formatter,
                "data pack at {} uses unsupported {feature}",
                path.display()
            ),
            Self::InvalidMemoryResourceIdentifier { pack, kind, input } => write!(
                formatter,
                "invalid {} identifier `{input}` in in-memory pack {pack}",
                resource_kind_name(*kind)
            ),
            Self::DuplicateMemoryResource { pack, kind, id } => write!(
                formatter,
                "duplicate {} `{id}` in in-memory pack {pack}",
                resource_kind_name(*kind)
            ),
            Self::InvalidFunction {
                origin,
                line,
                reason,
            } => write!(
                formatter,
                "invalid function {origin} at line {line}: {reason}"
            ),
            Self::InvalidFunctionTag { origin, reason } => {
                write!(formatter, "invalid function tag {origin}: {reason}")
            }
            Self::InvalidNumberProvider { origin, reason } => {
                write!(formatter, "invalid number provider {origin}: {reason}")
            }
            Self::InvalidNumberProviderTag { origin, reason } => {
                write!(formatter, "invalid number provider tag {origin}: {reason}")
            }
            Self::InvalidPredicate { origin, reason } => {
                write!(formatter, "invalid predicate {origin}: {reason}")
            }
            Self::InvalidPredicateTag { origin, reason } => {
                write!(formatter, "invalid predicate tag {origin}: {reason}")
            }
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

const RESOURCE_KINDS: [ResourceKind; 6] = [
    ResourceKind::Function,
    ResourceKind::FunctionTag,
    ResourceKind::NumberProvider,
    ResourceKind::NumberProviderTag,
    ResourceKind::Predicate,
    ResourceKind::PredicateTag,
];

#[derive(Clone, Copy)]
struct ResourceKindInfo {
    name: &'static str,
    directory: &'static str,
    extension: &'static str,
}

fn resource_kind_info(kind: ResourceKind) -> ResourceKindInfo {
    match kind {
        ResourceKind::Function => ResourceKindInfo {
            name: "function",
            directory: "function",
            extension: ".mcfunction",
        },
        ResourceKind::FunctionTag => ResourceKindInfo {
            name: "function tag",
            directory: "tags/function",
            extension: ".json",
        },
        ResourceKind::NumberProvider => ResourceKindInfo {
            name: "number provider",
            directory: "number_provider",
            extension: ".json",
        },
        ResourceKind::NumberProviderTag => ResourceKindInfo {
            name: "number provider tag",
            directory: "tags/number_provider",
            extension: ".json",
        },
        ResourceKind::Predicate => ResourceKindInfo {
            name: "predicate",
            directory: "predicate",
            extension: ".json",
        },
        ResourceKind::PredicateTag => ResourceKindInfo {
            name: "predicate tag",
            directory: "tags/predicate",
            extension: ".json",
        },
    }
}

fn resource_kind_name(kind: ResourceKind) -> &'static str {
    resource_kind_info(kind).name
}

#[derive(Debug, Default)]
struct ResourceInventory {
    stacks: HashMap<ResourceKind, HashMap<Identifier, Vec<SourcedResource>>>,
}

impl ResourceInventory {
    fn insert(&mut self, kind: ResourceKind, id: Identifier, resource: SourcedResource) {
        self.stacks
            .entry(kind)
            .or_default()
            .entry(id)
            .or_default()
            .push(resource);
    }

    fn sorted_stacks(&self, kind: ResourceKind) -> Vec<(&Identifier, &Vec<SourcedResource>)> {
        let mut stacks = self
            .stacks
            .get(&kind)
            .into_iter()
            .flat_map(HashMap::iter)
            .collect::<Vec<_>>();
        stacks.sort_by_key(|(id, _)| id.to_string());
        stacks
    }
}

#[derive(Debug)]
struct SourcedResource {
    origin: ResourceOrigin,
    contents: ResourceContents,
}

impl SourcedResource {
    fn contents(&self) -> Result<Cow<'_, str>, LoadError> {
        match &self.contents {
            ResourceContents::Directory(path) => read_to_string(path).map(Cow::Owned),
            ResourceContents::Memory(source) => Ok(Cow::Borrowed(source)),
        }
    }
}

#[derive(Debug)]
enum ResourceContents {
    Directory(PathBuf),
    Memory(String),
}

pub(crate) fn load_packs(packs: impl IntoIterator<Item = Pack>) -> Result<Program, LoadError> {
    let mut inventory = ResourceInventory::default();
    for (pack_index, pack) in packs.into_iter().enumerate() {
        match pack.into_source() {
            PackSource::Directory(root) => inventory_directory_pack(&mut inventory, &root)?,
            PackSource::Memory(resources) => {
                inventory_memory_pack(&mut inventory, pack_index, resources)?
            }
        }
    }
    compile_inventory(&inventory)
}

fn inventory_directory_pack(
    inventory: &mut ResourceInventory,
    root: &Path,
) -> Result<(), LoadError> {
    reject_symbolic_links(root)?;
    let root_metadata = metadata(root)?;
    if !root_metadata.is_dir() {
        return Err(invalid_pack(root, "the pack path is not a directory"));
    }
    validate_pack_metadata(root)?;

    let namespace_dirs = child_directories(&root.join("data"))?;
    for namespace_dir in namespace_dirs {
        let Some(namespace) = namespace_dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if Identifier::from_parts(namespace, "").is_none() {
            continue;
        }

        for kind in RESOURCE_KINDS {
            let info = resource_kind_info(kind);
            let resource_root = namespace_dir.join(info.directory);
            for path in regular_files_recursive(&resource_root)? {
                let Some(relative) = resource_path(&resource_root, &path) else {
                    continue;
                };
                let Some(id_path) = relative.strip_suffix(info.extension) else {
                    continue;
                };
                let full_resource_path = format!("{}/{relative}", info.directory);
                if Identifier::from_parts(namespace, &full_resource_path).is_none() {
                    continue;
                }
                let id = Identifier::from_parts(namespace, id_path)
                    .expect("removing a valid resource suffix preserves its identifier");
                inventory.insert(
                    kind,
                    id,
                    SourcedResource {
                        origin: ResourceOrigin::Directory(path.clone()),
                        contents: ResourceContents::Directory(path),
                    },
                );
            }
        }
    }
    Ok(())
}

fn inventory_memory_pack(
    inventory: &mut ResourceInventory,
    pack: usize,
    resources: Vec<crate::pack::MemoryResource>,
) -> Result<(), LoadError> {
    let mut seen = HashSet::new();
    for resource in resources {
        let (kind, raw_id, source) = resource.into_parts();
        let id = Identifier::parse(&raw_id).ok_or_else(|| {
            LoadError::InvalidMemoryResourceIdentifier {
                pack,
                kind,
                input: raw_id.clone(),
            }
        })?;
        if !seen.insert((kind, id.clone())) {
            return Err(LoadError::DuplicateMemoryResource {
                pack,
                kind,
                id: id.to_string(),
            });
        }
        let normalized_id = id.to_string();
        inventory.insert(
            kind,
            id,
            SourcedResource {
                origin: ResourceOrigin::Memory {
                    pack,
                    id: normalized_id,
                },
                contents: ResourceContents::Memory(source),
            },
        );
    }
    Ok(())
}

fn compile_inventory(inventory: &ResourceInventory) -> Result<Program, LoadError> {
    let mut number_providers = HashMap::new();
    let mut number_provider_origins = HashMap::new();
    for (id, stack) in inventory.sorted_stacks(ResourceKind::NumberProvider) {
        let resource = stack
            .last()
            .expect("every inventoried resource stack is nonempty");
        let source = resource.contents()?;
        let provider = parse_number_provider_json(&source).map_err(|reason| {
            LoadError::InvalidNumberProvider {
                origin: resource.origin.clone(),
                reason,
            }
        })?;
        number_provider_origins.insert(id.clone(), resource.origin.clone());
        number_providers.insert(id.clone(), provider);
    }

    let unresolved_number_provider_tags =
        compose_resource_tags(inventory, ResourceKind::NumberProviderTag)?;
    let mut predicates = HashMap::new();
    let mut predicate_origins = HashMap::new();
    for (id, stack) in inventory.sorted_stacks(ResourceKind::Predicate) {
        let resource = stack
            .last()
            .expect("every inventoried resource stack is nonempty");
        let source = resource.contents()?;
        let predicate =
            parse_predicate_json(&source).map_err(|reason| LoadError::InvalidPredicate {
                origin: resource.origin.clone(),
                reason,
            })?;
        predicate_origins.insert(id.clone(), resource.origin.clone());
        predicates.insert(id.clone(), predicate);
    }
    let unresolved_predicate_tags = compose_resource_tags(inventory, ResourceKind::PredicateTag)?;

    let empty_loot_registry = LootRegistry::empty();
    let mut provider_ids = empty_loot_registry.number_provider_ids();
    provider_ids.extend(number_providers.keys().cloned());
    let number_provider_tags = resolve_resource_tags(
        &provider_ids,
        &unresolved_number_provider_tags,
        "number provider",
        "number provider tag",
    )
    .map_err(|error| LoadError::InvalidNumberProviderTag {
        origin: error.origin,
        reason: error.reason,
    })?;
    let mut predicate_ids = empty_loot_registry.predicate_ids();
    predicate_ids.extend(predicates.keys().cloned());
    let predicate_tags = resolve_resource_tags(
        &predicate_ids,
        &unresolved_predicate_tags,
        "predicate",
        "predicate tag",
    )
    .map_err(|error| LoadError::InvalidPredicateTag {
        origin: error.origin,
        reason: error.reason,
    })?;
    let loot_registry = Arc::new(
        LootRegistry::new(
            number_providers,
            number_provider_tags,
            predicates,
            predicate_tags,
        )
        .map_err(
            |RegistryValidationError { resource, reason }| match resource {
                RegistryResource::NumberProvider(id) => LoadError::InvalidNumberProvider {
                    origin: number_provider_origins
                        .get(&id)
                        .expect("validation errors are attributed to selected resources")
                        .clone(),
                    reason,
                },
                RegistryResource::Predicate(id) => LoadError::InvalidPredicate {
                    origin: predicate_origins
                        .get(&id)
                        .expect("validation errors are attributed to selected resources")
                        .clone(),
                    reason,
                },
            },
        )?,
    );

    let compiler = CommandCompiler::with_loot_registry(Arc::clone(&loot_registry));
    let mut functions = HashMap::new();
    for (id, stack) in inventory.sorted_stacks(ResourceKind::Function) {
        let resource = stack
            .last()
            .expect("every inventoried resource stack is nonempty");
        let source = resource.contents()?;
        let function =
            parse_function(&source, &compiler).map_err(|error| LoadError::InvalidFunction {
                origin: resource.origin.clone(),
                line: error.line,
                reason: error.reason,
            })?;
        functions.insert(id.clone(), function);
    }

    let unresolved_function_tags = compose_resource_tags(inventory, ResourceKind::FunctionTag)?;
    let function_ids = functions.keys().cloned().collect();
    let function_tags = resolve_resource_tags(
        &function_ids,
        &unresolved_function_tags,
        "function",
        "function tag",
    )
    .map_err(|error| LoadError::InvalidFunctionTag {
        origin: error.origin,
        reason: error.reason,
    })?;
    Ok(Program::new(functions, function_tags, loot_registry))
}

fn compose_resource_tags(
    inventory: &ResourceInventory,
    kind: ResourceKind,
) -> Result<HashMap<Identifier, UnresolvedResourceTag>, LoadError> {
    let mut tags = HashMap::new();
    for (id, stack) in inventory.sorted_stacks(kind) {
        let mut entries = Vec::new();
        for resource in stack {
            let source = resource.contents()?;
            let parsed = parse_resource_tag(&source, &resource.origin)
                .map_err(|reason| invalid_tag_resource(kind, resource.origin.clone(), reason))?;
            if parsed.replace {
                entries.clear();
            }
            entries.extend(parsed.entries);
        }
        tags.insert(id.clone(), UnresolvedResourceTag { entries });
    }
    Ok(tags)
}

fn invalid_tag_resource(kind: ResourceKind, origin: ResourceOrigin, reason: String) -> LoadError {
    match kind {
        ResourceKind::FunctionTag => LoadError::InvalidFunctionTag { origin, reason },
        ResourceKind::NumberProviderTag => LoadError::InvalidNumberProviderTag { origin, reason },
        ResourceKind::PredicateTag => LoadError::InvalidPredicateTag { origin, reason },
        ResourceKind::Function | ResourceKind::NumberProvider | ResourceKind::Predicate => {
            unreachable!("only tag resource kinds are composed as stacks")
        }
    }
}

#[derive(Debug)]
struct UnresolvedResourceTag {
    entries: Vec<ResourceTagEntry>,
}

#[derive(Debug)]
struct ParsedResourceTag {
    replace: bool,
    entries: Vec<ResourceTagEntry>,
}

#[derive(Debug)]
struct ResourceTagEntry {
    reference: ResourceTagReference,
    required: bool,
    origin: ResourceOrigin,
}

#[derive(Debug)]
enum ResourceTagReference {
    Element(Identifier),
    Tag(Identifier),
}

#[derive(Debug)]
struct ResourceTagResolutionError {
    origin: ResourceOrigin,
    reason: String,
}

fn parse_resource_tag(
    contents: &str,
    origin: &ResourceOrigin,
) -> Result<ParsedResourceTag, String> {
    let value = resource_json::parse(contents)?;
    let object = value
        .as_object()
        .ok_or_else(|| "the root value must be an object".to_owned())?;
    let replace = match resource_json::field(object, "replace") {
        Some(Value::Bool(replace)) => *replace,
        Some(_) => return Err("`replace` must be a boolean".to_owned()),
        None => false,
    };
    let values = resource_json::field(object, "values")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing array field `values`".to_owned())?;
    let entries = values
        .iter()
        .enumerate()
        .map(|(index, value)| parse_resource_tag_entry(index, value, origin))
        .collect::<Result<_, _>>()?;
    Ok(ParsedResourceTag { replace, entries })
}

fn parse_resource_tag_entry(
    index: usize,
    value: &Value,
    origin: &ResourceOrigin,
) -> Result<ResourceTagEntry, String> {
    let (raw_id, required) = match value {
        Value::String(id) => (resource_json::decode_string(id).to_string_lossy(), true),
        Value::Object(object) => {
            let id = resource_json::field(object, "id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("`values[{index}].id` must be a string"))?;
            let required = match resource_json::field(object, "required") {
                Some(Value::Bool(required)) => *required,
                Some(_) => {
                    return Err(format!("`values[{index}].required` must be a boolean"));
                }
                None => true,
            };
            (resource_json::decode_string(id).to_string_lossy(), required)
        }
        _ => {
            return Err(format!("`values[{index}]` must be a string or an object"));
        }
    };
    let reference = raw_id
        .strip_prefix('#')
        .map_or_else(
            || Identifier::parse(&raw_id).map(ResourceTagReference::Element),
            |id| Identifier::parse(id).map(ResourceTagReference::Tag),
        )
        .ok_or_else(|| format!("`values[{index}]` has invalid identifier `{raw_id}`"))?;
    Ok(ResourceTagEntry {
        reference,
        required,
        origin: origin.clone(),
    })
}

fn resolve_resource_tags(
    elements: &HashSet<Identifier>,
    tags: &HashMap<Identifier, UnresolvedResourceTag>,
    element_kind: &str,
    tag_kind: &str,
) -> Result<HashMap<Identifier, Vec<Identifier>>, ResourceTagResolutionError> {
    let mut tag_ids = tags.keys().cloned().collect::<Vec<_>>();
    tag_ids.sort_by_key(ToString::to_string);

    let mut dependencies = tags
        .keys()
        .cloned()
        .map(|id| (id, Vec::new()))
        .collect::<HashMap<_, _>>();
    for tag_id in &tag_ids {
        let tag = tags
            .get(tag_id)
            .expect("the tag identifier came from the tag map");
        for entry in &tag.entries {
            match &entry.reference {
                ResourceTagReference::Element(element) if entry.required => {
                    if !elements.contains(element) {
                        return Err(invalid_resource_tag_reference(
                            &entry.origin,
                            format!("required {element_kind} `{element}` does not exist"),
                        ));
                    }
                }
                ResourceTagReference::Tag(dependency) if entry.required => {
                    if !tags.contains_key(dependency) {
                        return Err(invalid_resource_tag_reference(
                            &entry.origin,
                            format!("required {tag_kind} `#{dependency}` does not exist"),
                        ));
                    }
                    if creates_tag_cycle(&dependencies, tag_id, dependency) {
                        return Err(invalid_resource_tag_reference(
                            &entry.origin,
                            format!("required {tag_kind} reference `#{dependency}` is cyclic"),
                        ));
                    }
                    dependencies
                        .get_mut(tag_id)
                        .expect("every tag owns a dependency list")
                        .push(dependency.clone());
                }
                ResourceTagReference::Element(_) | ResourceTagReference::Tag(_) => {}
            }
        }
    }

    let mut accepted_optional_tags = HashSet::new();
    for tag_id in &tag_ids {
        let tag = tags
            .get(tag_id)
            .expect("the tag identifier came from the tag map");
        for (index, entry) in tag.entries.iter().enumerate() {
            let ResourceTagEntry {
                reference: ResourceTagReference::Tag(dependency),
                required: false,
                ..
            } = entry
            else {
                continue;
            };
            if !tags.contains_key(dependency)
                || creates_tag_cycle(&dependencies, tag_id, dependency)
            {
                continue;
            }
            dependencies
                .get_mut(tag_id)
                .expect("every tag owns a dependency list")
                .push(dependency.clone());
            accepted_optional_tags.insert((tag_id.clone(), index));
        }
    }

    let mut remaining_dependencies = dependencies
        .iter()
        .map(|(id, dependencies)| (id.clone(), dependencies.len()))
        .collect::<HashMap<_, _>>();
    let mut dependents = tags
        .keys()
        .cloned()
        .map(|id| (id, Vec::new()))
        .collect::<HashMap<_, _>>();
    for tag_id in &tag_ids {
        for dependency in dependencies
            .get(tag_id)
            .expect("every tag owns a dependency list")
        {
            dependents
                .get_mut(dependency)
                .expect("every dependency is an existing tag")
                .push(tag_id.clone());
        }
    }

    let mut ready = tag_ids
        .iter()
        .filter(|id| remaining_dependencies.get(*id) == Some(&0))
        .cloned()
        .collect::<Vec<_>>();
    let mut resolved = HashMap::new();
    let mut next = 0;
    while let Some(tag_id) = ready.get(next).cloned() {
        next += 1;
        let values =
            flatten_resource_tag(&tag_id, elements, tags, &accepted_optional_tags, &resolved);
        resolved.insert(tag_id.clone(), values);

        for dependent in dependents
            .get(&tag_id)
            .expect("every tag owns a dependents list")
        {
            let remaining = remaining_dependencies
                .get_mut(dependent)
                .expect("every dependent is an existing tag");
            *remaining -= 1;
            if *remaining == 0 {
                ready.push(dependent.clone());
            }
        }
    }
    assert_eq!(
        resolved.len(),
        tags.len(),
        "the accepted resource tag dependency graph is acyclic"
    );
    Ok(resolved)
}

fn creates_tag_cycle(
    dependencies: &HashMap<Identifier, Vec<Identifier>>,
    tag: &Identifier,
    dependency: &Identifier,
) -> bool {
    if tag == dependency {
        return true;
    }
    let mut pending = vec![dependency];
    let mut visited = HashSet::new();
    while let Some(current) = pending.pop() {
        if current == tag {
            return true;
        }
        if visited.insert(current) {
            pending.extend(dependencies.get(current).into_iter().flatten());
        }
    }
    false
}

fn flatten_resource_tag(
    id: &Identifier,
    elements: &HashSet<Identifier>,
    tags: &HashMap<Identifier, UnresolvedResourceTag>,
    accepted_optional_tags: &HashSet<(Identifier, usize)>,
    resolved: &HashMap<Identifier, Vec<Identifier>>,
) -> Vec<Identifier> {
    let mut values = Vec::new();
    let mut present = HashSet::new();
    for (index, entry) in tags
        .get(id)
        .expect("only collected tags are flattened")
        .entries
        .iter()
        .enumerate()
    {
        match &entry.reference {
            ResourceTagReference::Element(element) => {
                if elements.contains(element) && present.insert(element.clone()) {
                    values.push(element.clone());
                }
            }
            ResourceTagReference::Tag(tag)
                if entry.required || accepted_optional_tags.contains(&(id.clone(), index)) =>
            {
                for element in resolved
                    .get(tag)
                    .expect("accepted tag dependencies are flattened first")
                {
                    if present.insert(element.clone()) {
                        values.push(element.clone());
                    }
                }
            }
            ResourceTagReference::Tag(_) => {}
        }
    }
    values
}

fn invalid_resource_tag_reference(
    origin: &ResourceOrigin,
    reason: impl Into<String>,
) -> ResourceTagResolutionError {
    ResourceTagResolutionError {
        origin: origin.clone(),
        reason: reason.into(),
    }
}

fn validate_pack_metadata(root: &Path) -> Result<(), LoadError> {
    let path = root.join("pack.mcmeta");
    let contents = read_to_string(&path)?;
    let value: Value = serde_json::from_str(&contents)
        .map_err(|error| invalid_pack(&path, format!("invalid JSON: {error}")))?;
    let root_object = value
        .as_object()
        .ok_or_else(|| invalid_pack(&path, "the root value must be an object"))?;

    for (field, feature) in [
        ("features", "feature flags"),
        ("filter", "pack filters"),
        ("overlays", "resource overlays"),
    ] {
        if root_object.contains_key(field) {
            return Err(LoadError::UnsupportedPack { path, feature });
        }
    }

    let pack = root_object
        .get("pack")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid_pack(&path, "missing object field `pack`"))?;
    match pack.get("description") {
        Some(Value::String(_)) => {}
        Some(_) => {
            return Err(LoadError::UnsupportedPack {
                path,
                feature: "structured pack descriptions",
            });
        }
        None => return Err(invalid_pack(&path, "missing field `pack.description`")),
    }

    validate_pack_format(&path, pack)
}

fn validate_pack_format(path: &Path, pack: &Map<String, Value>) -> Result<(), LoadError> {
    let min_value = optional_field(pack, "min_format");
    let max_value = optional_field(pack, "max_format");
    if min_value.is_some() != max_value.is_some() {
        return Err(invalid_pack(
            path,
            "`min_format` and `max_format` must both be present",
        ));
    }
    let (Some(min_value), Some(max_value)) = (min_value, max_value) else {
        return Err(invalid_pack(
            path,
            "pack format 118 requires `min_format` and `max_format`",
        ));
    };
    let min = parse_pack_format(path, "min_format", min_value, 0)?;
    let max = parse_pack_format(path, "max_format", max_value, i32::MAX)?;
    if min > max {
        return Err(invalid_pack(
            path,
            format!("`min_format` {min} is greater than `max_format` {max}"),
        ));
    }
    let supported = optional_field(pack, "supported_formats")
        .map(|value| parse_integer_range(path, "supported_formats", value))
        .transpose()?;
    let main = optional_field(pack, "pack_format")
        .map(|value| parse_i32(path, "pack_format", value))
        .transpose()?;

    if min.major > LAST_PRE_MINOR_DATA_PACK_FORMAT {
        if supported.is_some() {
            return Err(invalid_pack(
                path,
                "`supported_formats` is not valid for pack formats 82 and newer",
            ));
        }
    } else {
        let (supported_min, supported_max) = supported.ok_or_else(|| {
            invalid_pack(
                path,
                "ranges including pack formats 81 and older require `supported_formats`",
            )
        })?;
        if supported_min != min.major {
            return Err(invalid_pack(
                path,
                "`supported_formats` and `min_format` have different lower bounds",
            ));
        }
        if supported_max != max.major && supported_max != LAST_PRE_MINOR_DATA_PACK_FORMAT {
            return Err(invalid_pack(
                path,
                "`supported_formats` and `max_format` have incompatible upper bounds",
            ));
        }
        if main.is_none() {
            return Err(invalid_pack(
                path,
                "ranges including pack formats 81 and older require `pack_format`",
            ));
        }
    }
    if let Some(main) = main {
        if main < min.major || main > max.major {
            return Err(invalid_pack(
                path,
                format!("`pack_format` {main} is outside the declared major range"),
            ));
        }
        if main < 15 {
            return Err(invalid_pack(
                path,
                "multi-version packs cannot have `pack_format` below 15",
            ));
        }
    }
    if !(min..=max).contains(&TARGET_PACK_FORMAT) {
        return Err(invalid_pack(
            path,
            format!(
                "declared range {min} through {max} does not include required format {TARGET_PACK_FORMAT}"
            ),
        ));
    }
    Ok(())
}

fn optional_field<'a>(pack: &'a Map<String, Value>, field: &str) -> Option<&'a Value> {
    pack.get(field).filter(|value| !value.is_null())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PackFormat {
    major: i32,
    minor: i32,
}

impl fmt::Display for PackFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.minor == i32::MAX {
            write!(formatter, "{}.*", self.major)
        } else {
            write!(formatter, "{}.{}", self.major, self.minor)
        }
    }
}

fn parse_pack_format(
    path: &Path,
    field: &str,
    value: &Value,
    default_minor: i32,
) -> Result<PackFormat, LoadError> {
    let components = match value {
        Value::Array(values) if (1..=256).contains(&values.len()) => values,
        Value::Array(_) => {
            return Err(invalid_pack(
                path,
                format!("`{field}` must contain between 1 and 256 numbers"),
            ));
        }
        _ => {
            let major = parse_nonnegative_i32(path, field, value)?;
            return Ok(PackFormat {
                major,
                minor: default_minor,
            });
        }
    };
    let parsed = components
        .iter()
        .enumerate()
        .map(|(index, component)| {
            parse_nonnegative_i32(path, &format!("{field}[{index}]"), component)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PackFormat {
        major: parsed[0],
        minor: parsed.get(1).copied().unwrap_or(default_minor),
    })
}

fn parse_nonnegative_i32(path: &Path, field: &str, value: &Value) -> Result<i32, LoadError> {
    let value = parse_i32(path, field, value)?;
    if value < 0 {
        Err(invalid_pack(
            path,
            format!("`{field}` must be a non-negative 32-bit integer"),
        ))
    } else {
        Ok(value)
    }
}

fn parse_i32(path: &Path, field: &str, value: &Value) -> Result<i32, LoadError> {
    let number = value
        .as_number()
        .ok_or_else(|| invalid_pack(path, format!("`{field}` must be a number")))?;
    java_number_to_i32(number)
        .map_err(|reason| invalid_pack(path, format!("invalid `{field}`: {reason}")))
}

pub(crate) fn java_number_to_i32(number: &serde_json::Number) -> Result<i32, String> {
    if let Some(value) = number.as_i64() {
        return Ok(value as i32);
    }
    if let Some(value) = number.as_u64() {
        return Ok(value as i32);
    }

    let number = number.to_string();
    if number.len() > 10_000 {
        return Err("number representation exceeds 10000 characters".to_owned());
    }
    let (negative, unsigned) = match number.strip_prefix('-') {
        Some(unsigned) => (true, unsigned),
        None => (false, number.as_str()),
    };
    let (mantissa, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, 0), |(mantissa, exponent)| {
            (mantissa, parse_decimal_exponent(exponent))
        });
    let fraction_length = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let fraction_length = i64::try_from(fraction_length).unwrap_or(i64::MAX);
    let scale = fraction_length.saturating_sub(exponent);
    if scale.unsigned_abs() >= 10_000 {
        return Err("number scale must be between -9999 and 9999".to_owned());
    }
    let digits = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .collect::<Vec<_>>();
    let shift = exponent.saturating_sub(fraction_length);

    let significant_digits = if shift < 0 {
        let dropped = shift.unsigned_abs();
        if dropped >= digits.len() as u64 {
            0
        } else {
            digits.len() - dropped as usize
        }
    } else {
        digits.len()
    };
    let mut narrowed = digits[..significant_digits]
        .iter()
        .fold(0_u32, |value, digit| {
            value
                .wrapping_mul(10)
                .wrapping_add(u32::from(*digit - b'0'))
        });
    if shift > 0 {
        if shift >= 32 {
            narrowed = 0;
        } else {
            for _ in 0..shift {
                narrowed = narrowed.wrapping_mul(10);
            }
        }
    }
    if negative {
        narrowed = 0_u32.wrapping_sub(narrowed);
    }
    Ok(narrowed as i32)
}

fn parse_decimal_exponent(exponent: &str) -> i64 {
    let (negative, digits) = match exponent.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, exponent.strip_prefix('+').unwrap_or(exponent)),
    };
    let magnitude = digits.bytes().fold(0_i64, |value, digit| {
        value
            .saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'))
    });
    if negative { -magnitude } else { magnitude }
}

fn parse_integer_range(path: &Path, field: &str, value: &Value) -> Result<(i32, i32), LoadError> {
    let range = match value {
        Value::Array(values) if values.len() == 2 => (
            parse_i32(path, &format!("{field}[0]"), &values[0])?,
            parse_i32(path, &format!("{field}[1]"), &values[1])?,
        ),
        Value::Object(object) => {
            let min = object.get("min_inclusive").ok_or_else(|| {
                invalid_pack(path, format!("`{field}.min_inclusive` is required"))
            })?;
            let max = object.get("max_inclusive").ok_or_else(|| {
                invalid_pack(path, format!("`{field}.max_inclusive` is required"))
            })?;
            (
                parse_i32(path, &format!("{field}.min_inclusive"), min)?,
                parse_i32(path, &format!("{field}.max_inclusive"), max)?,
            )
        }
        Value::Array(_) => {
            return Err(invalid_pack(
                path,
                format!("`{field}` array must contain exactly two numbers"),
            ));
        }
        _ => {
            let value = parse_i32(path, field, value)?;
            (value, value)
        }
    };
    if range.0 > range.1 {
        Err(invalid_pack(
            path,
            format!("`{field}` lower bound is greater than its upper bound"),
        ))
    } else {
        Ok(range)
    }
}

#[derive(Debug)]
struct FunctionParseError {
    line: usize,
    reason: String,
}

fn parse_function(
    contents: &str,
    compiler: &CommandCompiler,
) -> Result<Function, FunctionParseError> {
    let lines = java_lines(contents);
    let mut builder = FunctionBuilder::new();
    let mut index = 0;
    while index < lines.len() {
        let line_number = index + 1;
        let mut line = java_trim(lines[index]).to_owned();
        let mut command_length = utf16_length(&line);
        if line.ends_with('\\') {
            loop {
                line.pop();
                command_length -= 1;
                index += 1;
                if index == lines.len() {
                    return Err(invalid_function(
                        line_number,
                        "line continuation at end of file",
                    ));
                }
                let continued = java_trim(lines[index]);
                line.push_str(continued);
                command_length += utf16_length(continued);
                check_command_length(line_number, command_length)?;
                if !line.ends_with('\\') {
                    break;
                }
            }
        }
        check_command_length(line_number, command_length)?;

        if line.is_empty() || line.starts_with('#') {
            index += 1;
            continue;
        }
        if line.starts_with('/') {
            let reason = if line.starts_with("//") {
                "commands cannot start with `/`; comments must start with `#`"
            } else {
                "commands in functions must not start with `/`"
            };
            return Err(invalid_function(line_number, reason));
        }
        if let Some(macro_command) = line.strip_prefix('$') {
            builder
                .add_macro(macro_command)
                .map_err(|reason| invalid_function(line_number, reason))?;
            index += 1;
            continue;
        }

        let instruction = compiler
            .compile(&line)
            .map_err(|reason| invalid_function(line_number, reason))?;
        builder.add_command(instruction);
        index += 1;
    }
    Ok(builder.build())
}

fn check_command_length(line: usize, length: usize) -> Result<(), FunctionParseError> {
    if length > MAX_COMMAND_LENGTH {
        Err(invalid_function(
            line,
            format!("command is {length} UTF-16 code units; maximum is {MAX_COMMAND_LENGTH}"),
        ))
    } else {
        Ok(())
    }
}

fn utf16_length(value: &str) -> usize {
    value.encode_utf16().count()
}

fn java_trim(value: &str) -> &str {
    value.trim_matches(|character| character <= '\u{20}')
}

fn java_lines(contents: &str) -> Vec<&str> {
    let bytes = contents.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if matches!(bytes[index], b'\r' | b'\n') {
            lines.push(&contents[start..index]);
            if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            start = index + 1;
        }
        index += 1;
    }
    if start < contents.len() {
        lines.push(&contents[start..]);
    }
    lines
}

#[derive(Clone)]
struct LoweringSource {
    sink: Rc<RefCell<Option<Instruction>>>,
    modifiers: Vec<Modifier>,
}

impl LoweringSource {
    fn with_modifier(&self, modifier: Modifier) -> Self {
        let mut modifiers = self.modifiers.clone();
        modifiers.push(modifier);
        Self {
            sink: Rc::clone(&self.sink),
            modifiers,
        }
    }

    fn record(&self, command: CompiledCommand) -> Result<i32, CommandSyntaxException> {
        let mut sink = self.sink.borrow_mut();
        let instruction = Instruction {
            modifiers: self.modifiers.clone(),
            command,
        };
        if sink.replace(instruction).is_some() {
            return Err(syntax_error("command produced more than one instruction"));
        }
        Ok(SINGLE_SUCCESS)
    }
}

pub(crate) struct CommandCompiler {
    dispatcher: CommandDispatcher<LoweringSource>,
}

impl CommandCompiler {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_loot_registry(Arc::new(LootRegistry::empty()))
    }

    pub(crate) fn with_loot_registry(loot_registry: Arc<LootRegistry>) -> Self {
        let dispatcher = CommandDispatcher::new();

        let function_without_arguments: Command<LoweringSource> = Rc::new(|context| {
            context.source().record(CompiledCommand::Function {
                reference: function_reference(context),
                arguments: None,
            })
        });
        let function_with_compound: Command<LoweringSource> = Rc::new(|context| {
            context.source().record(CompiledCommand::Function {
                reference: function_reference(context),
                arguments: Some(FunctionArguments::Compound(compound_tag(
                    context,
                    "arguments",
                ))),
            })
        });
        let function_with_storage: Command<LoweringSource> = Rc::new(|context| {
            context.source().record(CompiledCommand::Function {
                reference: function_reference(context),
                arguments: Some(FunctionArguments::Storage {
                    storage: identifier(context, "source"),
                    path: None,
                }),
            })
        });
        let function_with_storage_path: Command<LoweringSource> = Rc::new(|context| {
            context.source().record(CompiledCommand::Function {
                reference: function_reference(context),
                arguments: Some(FunctionArguments::Storage {
                    storage: identifier(context, "source"),
                    path: Some(nbt_path(context, "path")),
                }),
            })
        });
        let storage_source = RequiredArgumentBuilder::argument("source", IdentifierArgument)
            .executes(function_with_storage)
            .then(
                RequiredArgumentBuilder::argument("path", NbtPathArgument)
                    .executes(function_with_storage_path),
            )
            .expect("a storage source can contain an NBT path");
        let function_name = RequiredArgumentBuilder::argument("name", FunctionArgument)
            .executes(function_without_arguments)
            .then(
                RequiredArgumentBuilder::argument("arguments", CompoundTagArgument)
                    .executes(function_with_compound),
            )
            .expect("a function name can contain a compound argument")
            .then(
                LiteralArgumentBuilder::literal("with")
                    .then(
                        LiteralArgumentBuilder::literal("storage")
                            .then(storage_source)
                            .expect("the storage argument has a distinct child name"),
                    )
                    .expect("the with literal can contain the storage literal"),
            )
            .expect("a function name can contain an argument source");
        dispatcher
            .register(
                LiteralArgumentBuilder::literal("function")
                    .then(function_name)
                    .expect("a literal can contain an argument"),
            )
            .expect("the command tree contains no conflicting function literal");

        let return_value: Command<LoweringSource> = Rc::new(|context| {
            let value = IntegerArgumentType::get_integer(context, "value")
                .expect("the return executor is attached below its integer argument");
            context.source().record(CompiledCommand::Return {
                success: true,
                value,
            })
        });
        let return_fail: Command<LoweringSource> = Rc::new(|context| {
            context.source().record(CompiledCommand::Return {
                success: false,
                value: 0,
            })
        });
        let return_run = LiteralArgumentBuilder::literal("run")
            .redirect_with_modifier(
                dispatcher.root(),
                Rc::new(|context| Ok(Rc::new(context.source().with_modifier(Modifier::ReturnRun)))),
            )
            .expect("the return run literal has no children");
        let return_command = LiteralArgumentBuilder::literal("return")
            .then(
                RequiredArgumentBuilder::argument("value", IntegerArgumentType::integer())
                    .executes(return_value),
            )
            .expect("the return literal can contain an integer argument")
            .then(LiteralArgumentBuilder::literal("fail").executes(return_fail))
            .expect("the return literal can contain the fail literal")
            .then(return_run)
            .expect("the return literal can contain the run redirect");
        dispatcher
            .register(return_command)
            .expect("the command tree contains no conflicting return literal");

        let seed: Command<LoweringSource> =
            Rc::new(|context| context.source().record(CompiledCommand::Seed));
        dispatcher
            .register(LiteralArgumentBuilder::literal("seed").executes(seed))
            .expect("the command tree contains no conflicting seed literal");

        let add_objective: Command<LoweringSource> = Rc::new(|context| {
            let objective = command_string(context, "objective");
            context.source().record(CompiledCommand::Scoreboard(
                ScoreboardCommand::AddObjective { objective },
            ))
        });
        let list_objectives: Command<LoweringSource> = Rc::new(|context| {
            context.source().record(CompiledCommand::Scoreboard(
                ScoreboardCommand::ListObjectives,
            ))
        });
        let remove_objective: Command<LoweringSource> = Rc::new(|context| {
            let objective = command_string(context, "objective");
            context.source().record(CompiledCommand::Scoreboard(
                ScoreboardCommand::RemoveObjective { objective },
            ))
        });
        let list_players: Command<LoweringSource> = Rc::new(|context| {
            context
                .source()
                .record(CompiledCommand::Scoreboard(ScoreboardCommand::ListPlayers))
        });
        let list_player_scores: Command<LoweringSource> = Rc::new(|context| {
            let holder = score_holders(context, "target");
            context.source().record(CompiledCommand::Scoreboard(
                ScoreboardCommand::ListPlayerScores { holder },
            ))
        });
        let set_score: Command<LoweringSource> = Rc::new(|context| {
            let holders = score_holders(context, "targets");
            let objective = command_string(context, "objective");
            let value = IntegerArgumentType::get_integer(context, "score")
                .expect("the set executor is attached below its integer argument");
            context
                .source()
                .record(CompiledCommand::Scoreboard(ScoreboardCommand::SetScore {
                    holders,
                    objective,
                    value,
                }))
        });
        let get_score: Command<LoweringSource> = Rc::new(|context| {
            let holder = score_holders(context, "target");
            let objective = command_string(context, "objective");
            context
                .source()
                .record(CompiledCommand::Scoreboard(ScoreboardCommand::GetScore {
                    holder,
                    objective,
                }))
        });
        let add_score: Command<LoweringSource> = Rc::new(|context| {
            let holders = score_holders(context, "targets");
            let objective = command_string(context, "objective");
            let value = IntegerArgumentType::get_integer(context, "score")
                .expect("the add executor is attached below its integer argument");
            context
                .source()
                .record(CompiledCommand::Scoreboard(ScoreboardCommand::AddScore {
                    holders,
                    objective,
                    value,
                }))
        });
        let remove_score: Command<LoweringSource> = Rc::new(|context| {
            let holders = score_holders(context, "targets");
            let objective = command_string(context, "objective");
            let value = IntegerArgumentType::get_integer(context, "score")
                .expect("the remove executor is attached below its integer argument");
            context.source().record(CompiledCommand::Scoreboard(
                ScoreboardCommand::RemoveScore {
                    holders,
                    objective,
                    value,
                },
            ))
        });
        let reset_scores: Command<LoweringSource> = Rc::new(|context| {
            let holders = score_holders(context, "targets");
            context.source().record(CompiledCommand::Scoreboard(
                ScoreboardCommand::ResetScores {
                    holders,
                    objective: None,
                },
            ))
        });
        let reset_score: Command<LoweringSource> = Rc::new(|context| {
            let holders = score_holders(context, "targets");
            let objective = command_string(context, "objective");
            context.source().record(CompiledCommand::Scoreboard(
                ScoreboardCommand::ResetScores {
                    holders,
                    objective: Some(objective),
                },
            ))
        });
        let operate_score: Command<LoweringSource> = Rc::new(|context| {
            let targets = score_holders(context, "targets");
            let target_objective = command_string(context, "targetObjective");
            let operation = scoreboard_operation(context, "operation");
            let sources = score_holders(context, "source");
            let source_objective = command_string(context, "sourceObjective");
            context
                .source()
                .record(CompiledCommand::Scoreboard(ScoreboardCommand::Operation {
                    targets,
                    target_objective,
                    operation,
                    sources,
                    source_objective,
                }))
        });
        let scoreboard = LiteralArgumentBuilder::literal("scoreboard")
            .then(
                LiteralArgumentBuilder::literal("objectives")
                    .then(LiteralArgumentBuilder::literal("list").executes(list_objectives))
                    .expect("the objectives literal can contain the list literal")
                    .then(
                        LiteralArgumentBuilder::literal("add")
                            .then(
                                RequiredArgumentBuilder::argument(
                                    "objective",
                                    StringArgumentType::word(),
                                )
                                .then(
                                    LiteralArgumentBuilder::literal("dummy")
                                        .executes(add_objective),
                                )
                                .expect("an objective name can contain the dummy criterion"),
                            )
                            .expect("the objectives add literal can contain an objective name"),
                    )
                    .expect("the objectives literal can contain the add literal")
                    .then(
                        LiteralArgumentBuilder::literal("remove")
                            .then(
                                RequiredArgumentBuilder::argument(
                                    "objective",
                                    StringArgumentType::word(),
                                )
                                .executes(remove_objective),
                            )
                            .expect("the objectives remove literal can contain an objective name"),
                    )
                    .expect("the objectives literal can contain the remove literal"),
            )
            .expect("the scoreboard literal can contain objectives commands")
            .then(
                LiteralArgumentBuilder::literal("players")
                    .then(
                        LiteralArgumentBuilder::literal("list")
                            .executes(list_players)
                            .then(
                                RequiredArgumentBuilder::argument("target", ScoreHolderArgument)
                                    .executes(list_player_scores),
                            )
                            .expect("the players list literal can contain a score holder"),
                    )
                    .expect("the players literal can contain the list literal")
                    .then(
                        LiteralArgumentBuilder::literal("set")
                            .then(
                                RequiredArgumentBuilder::argument("targets", ScoreHolderArgument)
                                    .then(
                                        RequiredArgumentBuilder::argument(
                                            "objective",
                                            StringArgumentType::word(),
                                        )
                                        .then(
                                            RequiredArgumentBuilder::argument(
                                                "score",
                                                IntegerArgumentType::integer(),
                                            )
                                            .executes(set_score),
                                        )
                                        .expect("an objective can contain a score"),
                                    )
                                    .expect("a score holder can contain an objective"),
                            )
                            .expect("the players set literal can contain a score holder"),
                    )
                    .expect("the players literal can contain the set literal")
                    .then(
                        LiteralArgumentBuilder::literal("get")
                            .then(
                                RequiredArgumentBuilder::argument("target", ScoreHolderArgument)
                                    .then(
                                        RequiredArgumentBuilder::argument(
                                            "objective",
                                            StringArgumentType::word(),
                                        )
                                        .executes(get_score),
                                    )
                                    .expect("a score holder can contain an objective"),
                            )
                            .expect("the players get literal can contain a score holder"),
                    )
                    .expect("the players literal can contain the get literal")
                    .then(score_delta_branch("add", add_score))
                    .expect("the players literal can contain the add literal")
                    .then(score_delta_branch("remove", remove_score))
                    .expect("the players literal can contain the remove literal")
                    .then(
                        LiteralArgumentBuilder::literal("reset")
                            .then(
                                RequiredArgumentBuilder::argument("targets", ScoreHolderArgument)
                                    .executes(reset_scores)
                                    .then(
                                        RequiredArgumentBuilder::argument(
                                            "objective",
                                            StringArgumentType::word(),
                                        )
                                        .executes(reset_score),
                                    )
                                    .expect("a reset score holder can contain an objective"),
                            )
                            .expect("the players reset literal can contain a score holder"),
                    )
                    .expect("the players literal can contain the reset literal")
                    .then(
                        LiteralArgumentBuilder::literal("operation")
                            .then(
                                RequiredArgumentBuilder::argument("targets", ScoreHolderArgument)
                                    .then(
                                        RequiredArgumentBuilder::argument(
                                            "targetObjective",
                                            StringArgumentType::word(),
                                        )
                                        .then(
                                            RequiredArgumentBuilder::argument(
                                                "operation",
                                                ScoreboardOperationArgument,
                                            )
                                            .then(
                                                RequiredArgumentBuilder::argument(
                                                    "source",
                                                    ScoreHolderArgument,
                                                )
                                                .then(
                                                    RequiredArgumentBuilder::argument(
                                                        "sourceObjective",
                                                        StringArgumentType::word(),
                                                    )
                                                    .executes(operate_score),
                                                )
                                                .expect(
                                                    "an operation source can contain an objective",
                                                ),
                                            )
                                            .expect(
                                                "an operation can contain a source score holder",
                                            ),
                                        )
                                        .expect("a target objective can contain an operation"),
                                    )
                                    .expect("an operation target can contain an objective"),
                            )
                            .expect("the operation literal can contain a target score holder"),
                    )
                    .expect("the players literal can contain the operation literal"),
            )
            .expect("the scoreboard literal can contain players commands");
        dispatcher
            .register(scoreboard)
            .expect("the command tree contains no conflicting scoreboard literal");

        dispatcher
            .register(compute_command_branch(Arc::clone(&loot_registry)))
            .expect("the command tree contains no conflicting compute literal");

        dispatcher
            .register(random_command_branch())
            .expect("the command tree contains no conflicting random literal");

        dispatcher
            .register(stopwatch_command_branch())
            .expect("the command tree contains no conflicting stopwatch literal");

        dispatcher
            .register(data_command_branch(Arc::clone(&loot_registry)))
            .expect("the command tree contains no conflicting data literal");

        let execute = dispatcher
            .register(LiteralArgumentBuilder::literal("execute"))
            .expect("the command tree contains no conflicting execute literal");
        let execute_command = LiteralArgumentBuilder::literal("execute")
            .then(
                LiteralArgumentBuilder::literal("run")
                    .redirect(dispatcher.root())
                    .expect("the execute run literal has no children"),
            )
            .expect("the execute literal can contain the run literal")
            .then(positioned_branch(execute.clone()))
            .expect("the execute literal can contain positioned")
            .then(rotated_branch(execute.clone()))
            .expect("the execute literal can contain rotated")
            .then(facing_branch(execute.clone()))
            .expect("the execute literal can contain facing")
            .then(align_branch(execute.clone()))
            .expect("the execute literal can contain align")
            .then(anchored_branch(execute.clone()))
            .expect("the execute literal can contain anchored")
            .then(
                LiteralArgumentBuilder::literal("store")
                    .then(store_branch("result", StoreKind::Result, execute.clone()))
                    .expect("the store literal can contain the result literal")
                    .then(store_branch("success", StoreKind::Success, execute.clone()))
                    .expect("the store literal can contain the success literal"),
            )
            .expect("the execute literal can contain the store literal")
            .then(execute_condition_branch(
                "if",
                true,
                execute.clone(),
                Arc::clone(&loot_registry),
            ))
            .expect("the execute literal can contain the if literal")
            .then(execute_condition_branch(
                "unless",
                false,
                execute,
                loot_registry,
            ))
            .expect("the execute literal can contain the unless literal");
        dispatcher
            .register(execute_command)
            .expect("the command tree contains no conflicting execute literal");

        Self { dispatcher }
    }

    pub(crate) fn compile(&self, command: &str) -> Result<Instruction, String> {
        self.compile_reader(StringReader::new(command))
    }

    pub(crate) fn compile_utf16(&self, command: Vec<u16>) -> Result<Instruction, String> {
        self.compile_reader(StringReader::from_utf16(command))
    }

    fn compile_reader(&self, command: StringReader) -> Result<Instruction, String> {
        let sink = Rc::new(RefCell::new(None));
        self.dispatcher
            .execute_reader(
                command,
                LoweringSource {
                    sink: Rc::clone(&sink),
                    modifiers: Vec::new(),
                },
            )
            .map_err(|error| error.to_string())?;
        let instruction = sink.borrow_mut().take();
        instruction.ok_or_else(|| "command did not produce an instruction".to_owned())
    }
}

fn positioned_branch(
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal("positioned")
        .then(
            RequiredArgumentBuilder::argument("pos", PositionArgument)
                .redirect_with_modifier(
                    execute,
                    Rc::new(|context| {
                        Ok(Rc::new(context.source().with_modifier(
                            Modifier::ContextTransform(ContextTransform::Positioned(
                                position_coordinates(context, "pos"),
                            )),
                        )))
                    }),
                )
                .expect("a complete positioned transform has no children"),
        )
        .expect("positioned can contain a position")
}

fn rotated_branch(
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal("rotated")
        .then(
            RequiredArgumentBuilder::argument("rot", RotationArgument)
                .redirect_with_modifier(
                    execute,
                    Rc::new(|context| {
                        Ok(Rc::new(context.source().with_modifier(
                            Modifier::ContextTransform(ContextTransform::Rotated(
                                rotation_coordinates(context, "rot"),
                            )),
                        )))
                    }),
                )
                .expect("a complete rotated transform has no children"),
        )
        .expect("rotated can contain a rotation")
}

fn facing_branch(
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal("facing")
        .then(
            RequiredArgumentBuilder::argument("pos", PositionArgument)
                .redirect_with_modifier(
                    execute,
                    Rc::new(|context| {
                        Ok(Rc::new(context.source().with_modifier(
                            Modifier::ContextTransform(ContextTransform::Facing(
                                position_coordinates(context, "pos"),
                            )),
                        )))
                    }),
                )
                .expect("a complete facing transform has no children"),
        )
        .expect("facing can contain a position")
}

fn align_branch(
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal("align")
        .then(
            RequiredArgumentBuilder::argument("axes", AxesArgument)
                .redirect_with_modifier(
                    execute,
                    Rc::new(|context| {
                        Ok(Rc::new(context.source().with_modifier(
                            Modifier::ContextTransform(ContextTransform::Align(axes(
                                context, "axes",
                            ))),
                        )))
                    }),
                )
                .expect("a complete align transform has no children"),
        )
        .expect("align can contain axes")
}

fn anchored_branch(
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    let anchor = |literal| {
        LiteralArgumentBuilder::literal(literal)
            .redirect_with_modifier(
                execute.clone(),
                Rc::new(|context| {
                    Ok(Rc::new(context.source().with_modifier(
                        Modifier::ContextTransform(ContextTransform::Anchored),
                    )))
                }),
            )
            .expect("a complete anchored transform has no children")
    };
    LiteralArgumentBuilder::literal("anchored")
        .then(anchor("feet"))
        .expect("anchored can contain feet")
        .then(anchor("eyes"))
        .expect("anchored can contain eyes")
}

fn compute_command_branch(
    loot_registry: Arc<LootRegistry>,
) -> LiteralArgumentBuilder<LoweringSource> {
    let float: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Compute(ComputeCommand {
                provider: number_provider(context, "provider"),
                mode: ComputeMode::Float { scale: 1.0 },
            }))
    });
    let scaled: Command<LoweringSource> = Rc::new(|context| {
        let scale = FloatArgumentType::get_float(context, "scale")
            .expect("scaled compute is attached below its scale argument");
        context
            .source()
            .record(CompiledCommand::Compute(ComputeCommand {
                provider: number_provider(context, "provider"),
                mode: ComputeMode::Float { scale },
            }))
    });
    let integer: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Compute(ComputeCommand {
                provider: number_provider(context, "provider"),
                mode: ComputeMode::Integer,
            }))
    });
    let provider =
        RequiredArgumentBuilder::argument("provider", NumberProviderArgument::new(loot_registry))
            .executes(float)
            .then(
                RequiredArgumentBuilder::argument("scale", FloatArgumentType::float())
                    .executes(scaled),
            )
            .expect("a compute provider can contain a scale")
            .then(LiteralArgumentBuilder::literal("integer").executes(integer))
            .expect("a compute provider can contain the integer literal");

    LiteralArgumentBuilder::literal("compute")
        .then(
            LiteralArgumentBuilder::literal("default")
                .then(provider)
                .expect("the default context can contain a provider"),
        )
        .expect("compute can contain the default context")
}

fn random_command_branch() -> LiteralArgumentBuilder<LoweringSource> {
    let value: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Random(RandomCommand::Value {
                range: integer_range(context, "range"),
                sequence: None,
            }))
    });
    let named_value: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Random(RandomCommand::Value {
                range: integer_range(context, "range"),
                sequence: Some(identifier(context, "sequence")),
            }))
    });
    let reset_all: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Random(RandomCommand::ResetAll {
                settings: None,
            }))
    });
    let reset_all_with_settings: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Random(RandomCommand::ResetAll {
                settings: Some(random_sequence_settings(context)),
            }))
    });
    let reset_sequence: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Random(RandomCommand::Reset {
                sequence: identifier(context, "sequence"),
                settings: None,
            }))
    });
    let reset_sequence_with_settings: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Random(RandomCommand::Reset {
                sequence: identifier(context, "sequence"),
                settings: Some(random_sequence_settings(context)),
            }))
    });
    let unsupported_roll: Command<LoweringSource> = Rc::new(|_| {
        Err(syntax_error(
            "random roll is outside Worldless scope because it broadcasts to live players",
        ))
    });

    let value_branch = LiteralArgumentBuilder::literal("value")
        .then(
            RequiredArgumentBuilder::argument("range", IntegerRangeArgument)
                .executes(value)
                .then(
                    RequiredArgumentBuilder::argument("sequence", IdentifierArgument)
                        .executes(named_value),
                )
                .expect("a random range can contain a sequence identifier"),
        )
        .expect("the random value literal can contain an integer range");
    let reset_branch = LiteralArgumentBuilder::literal("reset")
        .then(random_reset_settings_branch(
            LiteralArgumentBuilder::literal("*").executes(reset_all),
            reset_all_with_settings,
        ))
        .expect("the random reset literal can contain the all target")
        .then(random_reset_settings_branch(
            RequiredArgumentBuilder::argument("sequence", IdentifierArgument)
                .executes(reset_sequence),
            reset_sequence_with_settings,
        ))
        .expect("the random reset literal can contain a sequence target");
    let roll_branch = LiteralArgumentBuilder::literal("roll")
        .executes(Rc::clone(&unsupported_roll))
        .then(
            RequiredArgumentBuilder::argument("arguments", StringArgumentType::greedy_string())
                .executes(unsupported_roll),
        )
        .expect("the unsupported roll literal can consume its arguments");

    LiteralArgumentBuilder::literal("random")
        .then(value_branch)
        .expect("the random literal can contain value")
        .then(reset_branch)
        .expect("the random literal can contain reset")
        .then(roll_branch)
        .expect("the random literal can reject roll")
}

fn stopwatch_command_branch() -> LiteralArgumentBuilder<LoweringSource> {
    let create: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Stopwatch(StopwatchCommand::Create {
                id: identifier(context, "id"),
            }))
    });
    let query: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Stopwatch(StopwatchCommand::Query {
                id: identifier(context, "id"),
                scale: 1.0,
            }))
    });
    let scaled_query: Command<LoweringSource> = Rc::new(|context| {
        let scale = DoubleArgumentType::get_double(context, "scale")
            .expect("the scaled stopwatch query is attached below its scale argument");
        context
            .source()
            .record(CompiledCommand::Stopwatch(StopwatchCommand::Query {
                id: identifier(context, "id"),
                scale,
            }))
    });
    let restart: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Stopwatch(StopwatchCommand::Restart {
                id: identifier(context, "id"),
            }))
    });
    let remove: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Stopwatch(StopwatchCommand::Remove {
                id: identifier(context, "id"),
            }))
    });

    LiteralArgumentBuilder::literal("stopwatch")
        .then(
            LiteralArgumentBuilder::literal("create")
                .then(RequiredArgumentBuilder::argument("id", IdentifierArgument).executes(create))
                .expect("stopwatch create can contain an identifier"),
        )
        .expect("stopwatch can contain create")
        .then(
            LiteralArgumentBuilder::literal("query")
                .then(
                    RequiredArgumentBuilder::argument("id", IdentifierArgument)
                        .executes(query)
                        .then(
                            RequiredArgumentBuilder::argument(
                                "scale",
                                DoubleArgumentType::double(),
                            )
                            .executes(scaled_query),
                        )
                        .expect("a stopwatch query identifier can contain a scale"),
                )
                .expect("stopwatch query can contain an identifier"),
        )
        .expect("stopwatch can contain query")
        .then(
            LiteralArgumentBuilder::literal("restart")
                .then(RequiredArgumentBuilder::argument("id", IdentifierArgument).executes(restart))
                .expect("stopwatch restart can contain an identifier"),
        )
        .expect("stopwatch can contain restart")
        .then(
            LiteralArgumentBuilder::literal("remove")
                .then(RequiredArgumentBuilder::argument("id", IdentifierArgument).executes(remove))
                .expect("stopwatch remove can contain an identifier"),
        )
        .expect("stopwatch can contain remove")
}

fn random_reset_settings_branch(
    target: impl Into<ArgumentBuilder<LoweringSource>>,
    executor: Command<LoweringSource>,
) -> ArgumentBuilder<LoweringSource> {
    target
        .into()
        .then(
            RequiredArgumentBuilder::argument("seed", IntegerArgumentType::integer())
                .executes(Rc::clone(&executor))
                .then(
                    RequiredArgumentBuilder::argument("includeWorldSeed", BoolArgumentType::bool())
                        .executes(Rc::clone(&executor))
                        .then(
                            RequiredArgumentBuilder::argument(
                                "includeSequenceId",
                                BoolArgumentType::bool(),
                            )
                            .executes(executor),
                        )
                        .expect("includeWorldSeed can contain includeSequenceId"),
                )
                .expect("a random seed can contain includeWorldSeed"),
        )
        .expect("a random reset target can contain settings")
}

fn random_sequence_settings(context: &CommandContext<LoweringSource>) -> RandomSequenceSettings {
    let salt = IntegerArgumentType::get_integer(context, "seed")
        .expect("random reset settings are attached below their seed argument");
    let mut settings = RandomSequenceSettings::minecraft_default(salt);
    settings.include_world_seed =
        optional_random_bool(context, "includeWorldSeed", settings.include_world_seed);
    settings.include_sequence_id =
        optional_random_bool(context, "includeSequenceId", settings.include_sequence_id);
    settings
}

fn optional_random_bool(
    context: &CommandContext<LoweringSource>,
    name: &str,
    default: bool,
) -> bool {
    match BoolArgumentType::get_bool(context, name) {
        Ok(value) => value,
        Err(ContextError::MissingArgument(_)) => default,
        Err(error) => panic!("invalid parsed random boolean argument: {error}"),
    }
}

type ModifyOperationFactory = fn(&CommandContext<LoweringSource>) -> DataModifyOperation;

fn data_command_branch(loot_registry: Arc<LootRegistry>) -> LiteralArgumentBuilder<LoweringSource> {
    let merge: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Data(DataCommand::Merge {
                storage: identifier(context, "target"),
                value: compound_tag(context, "nbt"),
            }))
    });
    let get: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Data(DataCommand::Get {
                storage: identifier(context, "target"),
            }))
    });
    let get_path: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Data(DataCommand::GetPath {
                storage: identifier(context, "target"),
                path: nbt_path(context, "path"),
                scale: None,
            }))
    });
    let get_scaled: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Data(DataCommand::GetPath {
                storage: identifier(context, "target"),
                path: nbt_path(context, "path"),
                scale: Some(
                    DoubleArgumentType::get_double(context, "scale")
                        .expect("scaled data get is attached below its scale argument"),
                ),
            }))
    });
    let remove: Command<LoweringSource> = Rc::new(|context| {
        context
            .source()
            .record(CompiledCommand::Data(DataCommand::Remove {
                storage: identifier(context, "target"),
                path: nbt_path(context, "path"),
            }))
    });

    LiteralArgumentBuilder::literal("data")
        .then(
            LiteralArgumentBuilder::literal("merge")
                .then(
                    LiteralArgumentBuilder::literal("storage")
                        .then(
                            RequiredArgumentBuilder::argument("target", IdentifierArgument)
                                .then(
                                    RequiredArgumentBuilder::argument("nbt", CompoundTagArgument)
                                        .executes(merge),
                                )
                                .expect("a merge target can contain a compound tag"),
                        )
                        .expect("storage merge can contain a target"),
                )
                .expect("data merge can contain storage"),
        )
        .expect("data can contain merge")
        .then(
            LiteralArgumentBuilder::literal("get")
                .then(
                    LiteralArgumentBuilder::literal("storage")
                        .then(
                            RequiredArgumentBuilder::argument("target", IdentifierArgument)
                                .executes(get)
                                .then(
                                    RequiredArgumentBuilder::argument("path", NbtPathArgument)
                                        .executes(get_path)
                                        .then(
                                            RequiredArgumentBuilder::argument(
                                                "scale",
                                                DoubleArgumentType::double(),
                                            )
                                            .executes(get_scaled),
                                        )
                                        .expect("a data path can contain a scale"),
                                )
                                .expect("a data target can contain a path"),
                        )
                        .expect("storage get can contain a target"),
                )
                .expect("data get can contain storage"),
        )
        .expect("data can contain get")
        .then(
            LiteralArgumentBuilder::literal("remove")
                .then(
                    LiteralArgumentBuilder::literal("storage")
                        .then(
                            RequiredArgumentBuilder::argument("target", IdentifierArgument)
                                .then(
                                    RequiredArgumentBuilder::argument("path", NbtPathArgument)
                                        .executes(remove),
                                )
                                .expect("a remove target can contain a path"),
                        )
                        .expect("storage remove can contain a target"),
                )
                .expect("data remove can contain storage"),
        )
        .expect("data can contain remove")
        .then(data_modify_branch(loot_registry))
        .expect("data can contain modify")
}

fn data_modify_branch(loot_registry: Arc<LootRegistry>) -> LiteralArgumentBuilder<LoweringSource> {
    let target_path = RequiredArgumentBuilder::argument("targetPath", NbtPathArgument)
        .then(modify_insert_branch(Arc::clone(&loot_registry)))
        .expect("a modify target path can contain insert")
        .then(modify_operation_branch(
            "prepend",
            |_| DataModifyOperation::Insert(0),
            Arc::clone(&loot_registry),
        ))
        .expect("a modify target path can contain prepend")
        .then(modify_operation_branch(
            "append",
            |_| DataModifyOperation::Insert(-1),
            Arc::clone(&loot_registry),
        ))
        .expect("a modify target path can contain append")
        .then(modify_operation_branch(
            "set",
            |_| DataModifyOperation::Set,
            Arc::clone(&loot_registry),
        ))
        .expect("a modify target path can contain set")
        .then(modify_operation_branch(
            "merge",
            |_| DataModifyOperation::Merge,
            loot_registry,
        ))
        .expect("a modify target path can contain merge");

    LiteralArgumentBuilder::literal("modify")
        .then(
            LiteralArgumentBuilder::literal("storage")
                .then(
                    RequiredArgumentBuilder::argument("target", IdentifierArgument)
                        .then(target_path)
                        .expect("a modify target can contain a path"),
                )
                .expect("storage modify can contain a target"),
        )
        .expect("data modify can contain storage")
}

fn modify_insert_branch(
    loot_registry: Arc<LootRegistry>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal("insert")
        .then(modify_sources(
            RequiredArgumentBuilder::argument("index", IntegerArgumentType::integer()),
            |context| {
                DataModifyOperation::Insert(
                    IntegerArgumentType::get_integer(context, "index")
                        .expect("insert sources are attached below the index argument"),
                )
            },
            loot_registry,
        ))
        .expect("insert can contain an index and source")
}

fn modify_operation_branch(
    literal: &'static str,
    operation: ModifyOperationFactory,
    loot_registry: Arc<LootRegistry>,
) -> LiteralArgumentBuilder<LoweringSource> {
    match modify_sources(
        LiteralArgumentBuilder::literal(literal),
        operation,
        loot_registry,
    ) {
        ArgumentBuilder::Literal(builder) => builder,
        ArgumentBuilder::Required(_) => unreachable!("a literal operation remains a literal"),
    }
}

fn modify_sources(
    parent: impl Into<ArgumentBuilder<LoweringSource>>,
    operation: ModifyOperationFactory,
    loot_registry: Arc<LootRegistry>,
) -> ArgumentBuilder<LoweringSource> {
    parent
        .into()
        .then(modify_value_source(operation))
        .expect("a modify operation can contain a literal value")
        .then(modify_storage_source("from", operation, false))
        .expect("a modify operation can contain a storage source")
        .then(modify_storage_source("string", operation, true))
        .expect("a modify operation can contain a string storage source")
        .then(modify_compute_source(operation, loot_registry))
        .expect("a modify operation can contain a computed source")
}

fn modify_compute_source(
    operation: ModifyOperationFactory,
    loot_registry: Arc<LootRegistry>,
) -> LiteralArgumentBuilder<LoweringSource> {
    let float: Command<LoweringSource> = Rc::new(move |context| {
        record_data_modify(
            context,
            operation(context),
            DataSource::Compute {
                provider: number_provider(context, "provider"),
                integer: false,
            },
        )
    });
    let integer: Command<LoweringSource> = Rc::new(move |context| {
        record_data_modify(
            context,
            operation(context),
            DataSource::Compute {
                provider: number_provider(context, "provider"),
                integer: true,
            },
        )
    });
    let provider =
        RequiredArgumentBuilder::argument("provider", NumberProviderArgument::new(loot_registry))
            .executes(float)
            .then(LiteralArgumentBuilder::literal("integer").executes(integer))
            .expect("a data compute provider can contain the integer literal");

    LiteralArgumentBuilder::literal("compute")
        .then(
            LiteralArgumentBuilder::literal("default")
                .then(provider)
                .expect("the default context can contain a provider"),
        )
        .expect("a computed data source can contain the default context")
}

fn modify_value_source(
    operation: ModifyOperationFactory,
) -> LiteralArgumentBuilder<LoweringSource> {
    let command: Command<LoweringSource> = Rc::new(move |context| {
        record_data_modify(
            context,
            operation(context),
            DataSource::Value(nbt_tag(context, "value")),
        )
    });
    LiteralArgumentBuilder::literal("value")
        .then(RequiredArgumentBuilder::argument("value", NbtTagArgument).executes(command))
        .expect("the value literal can contain an NBT value")
}

fn modify_storage_source(
    literal: &'static str,
    operation: ModifyOperationFactory,
    string: bool,
) -> LiteralArgumentBuilder<LoweringSource> {
    let root_command: Command<LoweringSource> = Rc::new(move |context| {
        record_data_modify(
            context,
            operation(context),
            storage_data_source(context, string, None, None),
        )
    });
    let path_command: Command<LoweringSource> = Rc::new(move |context| {
        record_data_modify(
            context,
            operation(context),
            storage_data_source(context, string, Some(nbt_path(context, "sourcePath")), None),
        )
    });
    let start_command: Command<LoweringSource> = Rc::new(move |context| {
        let start = IntegerArgumentType::get_integer(context, "start")
            .expect("string source is attached below its start argument");
        record_data_modify(
            context,
            operation(context),
            storage_data_source(
                context,
                string,
                Some(nbt_path(context, "sourcePath")),
                Some(DataStringSubstring { start, end: None }),
            ),
        )
    });
    let end_command: Command<LoweringSource> = Rc::new(move |context| {
        let start = IntegerArgumentType::get_integer(context, "start")
            .expect("string source is attached below its start argument");
        let end = IntegerArgumentType::get_integer(context, "end")
            .expect("string source is attached below its end argument");
        record_data_modify(
            context,
            operation(context),
            storage_data_source(
                context,
                string,
                Some(nbt_path(context, "sourcePath")),
                Some(DataStringSubstring {
                    start,
                    end: Some(end),
                }),
            ),
        )
    });

    let source_path = if string {
        RequiredArgumentBuilder::argument("sourcePath", NbtPathArgument)
            .executes(path_command)
            .then(
                RequiredArgumentBuilder::argument("start", IntegerArgumentType::integer())
                    .executes(start_command)
                    .then(
                        RequiredArgumentBuilder::argument("end", IntegerArgumentType::integer())
                            .executes(end_command),
                    )
                    .expect("a string start can contain an end"),
            )
            .expect("a string source path can contain a start")
    } else {
        RequiredArgumentBuilder::argument("sourcePath", NbtPathArgument).executes(path_command)
    };

    LiteralArgumentBuilder::literal(literal)
        .then(
            LiteralArgumentBuilder::literal("storage")
                .then(
                    RequiredArgumentBuilder::argument("source", IdentifierArgument)
                        .executes(root_command)
                        .then(source_path)
                        .expect("a storage source can contain a path"),
                )
                .expect("a source can contain storage"),
        )
        .expect("a modify operation can contain a source kind")
}

fn storage_data_source(
    context: &CommandContext<LoweringSource>,
    string: bool,
    path: Option<NbtPath>,
    substring: Option<DataStringSubstring>,
) -> DataSource {
    let storage = identifier(context, "source");
    if string {
        DataSource::String {
            storage,
            path,
            substring,
        }
    } else {
        debug_assert!(substring.is_none());
        DataSource::Storage { storage, path }
    }
}

fn record_data_modify(
    context: &CommandContext<LoweringSource>,
    operation: DataModifyOperation,
    source: DataSource,
) -> Result<i32, CommandSyntaxException> {
    context
        .source()
        .record(CompiledCommand::Data(DataCommand::Modify {
            storage: identifier(context, "target"),
            path: nbt_path(context, "targetPath"),
            operation,
            source,
        }))
}

fn score_delta_branch(
    literal: &'static str,
    command: Command<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal(literal)
        .then(
            RequiredArgumentBuilder::argument("targets", ScoreHolderArgument)
                .then(
                    RequiredArgumentBuilder::argument("objective", StringArgumentType::word())
                        .then(
                            RequiredArgumentBuilder::argument(
                                "score",
                                IntegerArgumentType::integer_range(0, i32::MAX),
                            )
                            .executes(command),
                        )
                        .expect("an objective can contain a non-negative score"),
                )
                .expect("a score holder can contain an objective"),
        )
        .expect("a score operation can contain a score holder")
}

fn execute_condition_branch(
    literal: &'static str,
    expected: bool,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
    loot_registry: Arc<LootRegistry>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal(literal)
        .then(
            LiteralArgumentBuilder::literal("score")
                .then(
                    RequiredArgumentBuilder::argument("target", ScoreHolderArgument)
                        .then(
                            RequiredArgumentBuilder::argument(
                                "targetObjective",
                                StringArgumentType::word(),
                            )
                            .then(score_comparison_condition_branch(
                                "=",
                                ScoreComparison::Equal,
                                expected,
                                execute.clone(),
                            ))
                            .expect("a target objective can contain equality comparison")
                            .then(score_comparison_condition_branch(
                                "<",
                                ScoreComparison::LessThan,
                                expected,
                                execute.clone(),
                            ))
                            .expect("a target objective can contain less-than comparison")
                            .then(score_comparison_condition_branch(
                                "<=",
                                ScoreComparison::LessThanOrEqual,
                                expected,
                                execute.clone(),
                            ))
                            .expect("a target objective can contain less-than-or-equal comparison")
                            .then(score_comparison_condition_branch(
                                ">",
                                ScoreComparison::GreaterThan,
                                expected,
                                execute.clone(),
                            ))
                            .expect("a target objective can contain greater-than comparison")
                            .then(score_comparison_condition_branch(
                                ">=",
                                ScoreComparison::GreaterThanOrEqual,
                                expected,
                                execute.clone(),
                            ))
                            .expect(
                                "a target objective can contain greater-than-or-equal comparison",
                            )
                            .then(score_matches_condition_branch(expected, execute.clone()))
                            .expect("a target objective can contain a range match"),
                        )
                        .expect("a target score holder can contain an objective"),
                )
                .expect("the score literal can contain a target score holder"),
        )
        .expect("a conditional can contain the score literal")
        .then(
            LiteralArgumentBuilder::literal("function")
                .then(
                    RequiredArgumentBuilder::argument("name", FunctionArgument)
                        .fork(
                            execute.clone(),
                            Rc::new(move |context: &CommandContext<LoweringSource>| {
                                let function = context.argument::<FunctionReference>("name").expect(
                                    "the function condition is attached below its name argument",
                                );
                                Ok(vec![Rc::new(context.source().with_modifier(
                                    Modifier::FunctionCondition {
                                        expected,
                                        function: (*function).clone(),
                                    },
                                ))])
                            }),
                        )
                        .expect("a function condition can redirect to execute"),
                )
                .expect("the function literal can contain a function name"),
        )
        .expect("a conditional can contain the function literal")
        .then(predicate_condition_branch(
            expected,
            execute.clone(),
            loot_registry,
        ))
        .expect("a conditional can contain the predicate literal")
        .then(stopwatch_condition_branch(expected, execute.clone()))
        .expect("a conditional can contain the stopwatch literal")
        .then(storage_data_condition_branch(expected, execute))
        .expect("a conditional can contain storage data")
}

fn stopwatch_condition_branch(
    expected: bool,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    let terminal: Command<LoweringSource> = Rc::new(move |context| {
        context
            .source()
            .record(CompiledCommand::StopwatchCondition(stopwatch_condition(
                context, expected,
            )))
    });
    let modifier = Rc::new(move |context: &CommandContext<LoweringSource>| {
        Ok(vec![Rc::new(context.source().with_modifier(
            Modifier::StopwatchCondition(stopwatch_condition(context, expected)),
        ))])
    });

    LiteralArgumentBuilder::literal("stopwatch")
        .then(
            RequiredArgumentBuilder::argument("id", IdentifierArgument)
                .then(
                    RequiredArgumentBuilder::argument("range", DoubleRangeArgument)
                        .executes(terminal)
                        .fork(execute, modifier)
                        .expect("a complete stopwatch condition can redirect to execute"),
                )
                .expect("a stopwatch condition identifier can contain a range"),
        )
        .expect("the stopwatch literal can contain an identifier")
}

fn stopwatch_condition(
    context: &CommandContext<LoweringSource>,
    expected: bool,
) -> StopwatchCondition {
    StopwatchCondition {
        expected,
        id: identifier(context, "id"),
        range: double_range(context, "range"),
    }
}

fn predicate_condition_branch(
    expected: bool,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
    loot_registry: Arc<LootRegistry>,
) -> LiteralArgumentBuilder<LoweringSource> {
    let terminal: Command<LoweringSource> = Rc::new(move |context| {
        context
            .source()
            .record(CompiledCommand::PredicateCondition(predicate_condition(
                context, expected,
            )))
    });
    let modifier = Rc::new(move |context: &CommandContext<LoweringSource>| {
        Ok(vec![Rc::new(context.source().with_modifier(
            Modifier::PredicateCondition(predicate_condition(context, expected)),
        ))])
    });

    LiteralArgumentBuilder::literal("predicate")
        .then(
            RequiredArgumentBuilder::argument("predicate", PredicateArgument::new(loot_registry))
                .executes(terminal)
                .fork(execute, modifier)
                .expect("a complete predicate condition can redirect to execute"),
        )
        .expect("the predicate literal can contain a predicate")
}

fn predicate_condition(
    context: &CommandContext<LoweringSource>,
    expected: bool,
) -> PredicateCondition {
    let predicate = context
        .argument::<PredicateReference>("predicate")
        .map(|predicate| (*predicate).clone())
        .expect("the predicate condition is attached below its predicate argument");
    PredicateCondition {
        expected,
        predicate,
    }
}

fn storage_data_condition_branch(
    expected: bool,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    let terminal: Command<LoweringSource> = Rc::new(move |context| {
        context
            .source()
            .record(CompiledCommand::StorageCondition(storage_condition(
                context, expected,
            )))
    });
    let modifier = Rc::new(move |context: &CommandContext<LoweringSource>| {
        Ok(vec![Rc::new(context.source().with_modifier(
            Modifier::StorageCondition(storage_condition(context, expected)),
        ))])
    });

    LiteralArgumentBuilder::literal("data")
        .then(
            LiteralArgumentBuilder::literal("storage")
                .then(
                    RequiredArgumentBuilder::argument("source", IdentifierArgument)
                        .then(
                            RequiredArgumentBuilder::argument("path", NbtPathArgument)
                                .executes(terminal)
                                .fork(execute, modifier)
                                .expect("a complete storage condition can redirect to execute"),
                        )
                        .expect("a storage condition source can contain a path"),
                )
                .expect("a data condition can contain storage"),
        )
        .expect("a conditional data literal can contain a provider")
}

fn storage_condition(context: &CommandContext<LoweringSource>, expected: bool) -> StorageCondition {
    StorageCondition {
        expected,
        storage: identifier(context, "source"),
        path: nbt_path(context, "path"),
    }
}

fn score_comparison_condition_branch(
    literal: &'static str,
    comparison: ScoreComparison,
    expected: bool,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    let terminal: Command<LoweringSource> = Rc::new(move |context| {
        context
            .source()
            .record(CompiledCommand::Condition(score_comparison_condition(
                context, expected, comparison,
            )))
    });
    let modifier = Rc::new(move |context: &CommandContext<LoweringSource>| {
        let condition = score_comparison_condition(context, expected, comparison);
        Ok(vec![Rc::new(
            context
                .source()
                .with_modifier(Modifier::Condition(condition)),
        )])
    });

    LiteralArgumentBuilder::literal(literal)
        .then(
            RequiredArgumentBuilder::argument("source", ScoreHolderArgument)
                .then(
                    RequiredArgumentBuilder::argument(
                        "sourceObjective",
                        StringArgumentType::word(),
                    )
                    .executes(terminal)
                    .fork(execute, modifier)
                    .expect("a complete comparison can redirect to execute"),
                )
                .expect("a comparison source can contain an objective"),
        )
        .expect("a comparison can contain a source score holder")
}

fn score_matches_condition_branch(
    expected: bool,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    let terminal: Command<LoweringSource> = Rc::new(move |context| {
        context
            .source()
            .record(CompiledCommand::Condition(score_matches_condition(
                context, expected,
            )))
    });
    let modifier = Rc::new(move |context: &CommandContext<LoweringSource>| {
        let condition = score_matches_condition(context, expected);
        Ok(vec![Rc::new(
            context
                .source()
                .with_modifier(Modifier::Condition(condition)),
        )])
    });

    LiteralArgumentBuilder::literal("matches")
        .then(
            RequiredArgumentBuilder::argument("range", IntegerRangeArgument)
                .executes(terminal)
                .fork(execute, modifier)
                .expect("a complete range match can redirect to execute"),
        )
        .expect("matches can contain an integer range")
}

fn score_comparison_condition(
    context: &CommandContext<LoweringSource>,
    expected: bool,
    comparison: ScoreComparison,
) -> ScoreCondition {
    ScoreCondition {
        expected,
        predicate: ScorePredicate::Compare {
            left: score_reference(context, "target", "targetObjective"),
            comparison,
            right: score_reference(context, "source", "sourceObjective"),
        },
    }
}

fn score_matches_condition(
    context: &CommandContext<LoweringSource>,
    expected: bool,
) -> ScoreCondition {
    ScoreCondition {
        expected,
        predicate: ScorePredicate::Matches {
            score: score_reference(context, "target", "targetObjective"),
            range: integer_range(context, "range"),
        },
    }
}

fn store_branch(
    literal: &'static str,
    kind: StoreKind,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal(literal)
        .then(
            LiteralArgumentBuilder::literal("score")
                .then(
                    RequiredArgumentBuilder::argument("targets", ScoreHolderArgument)
                        .then(
                            RequiredArgumentBuilder::argument(
                                "objective",
                                StringArgumentType::word(),
                            )
                            .redirect_with_modifier(
                                execute.clone(),
                                Rc::new(move |context| {
                                    let holders = score_holders(context, "targets");
                                    let objective = command_string(context, "objective");
                                    Ok(Rc::new(context.source().with_modifier(
                                        Modifier::StoreScore {
                                            kind,
                                            holders,
                                            objective,
                                        },
                                    )))
                                }),
                            )
                            .expect("the store objective has no children"),
                        )
                        .expect("a store score holder can contain an objective"),
                )
                .expect("the score literal can contain a score holder"),
        )
        .expect("a store mode can contain the score literal")
        .then(store_storage_branch(kind, execute))
        .expect("a store mode can contain storage")
}

fn store_storage_branch(
    kind: StoreKind,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    let path = RequiredArgumentBuilder::argument("path", NbtPathArgument)
        .then(store_storage_type_branch(
            "byte",
            StorageNumberType::Byte,
            kind,
            execute.clone(),
        ))
        .expect("a storage path can contain byte")
        .then(store_storage_type_branch(
            "short",
            StorageNumberType::Short,
            kind,
            execute.clone(),
        ))
        .expect("a storage path can contain short")
        .then(store_storage_type_branch(
            "int",
            StorageNumberType::Int,
            kind,
            execute.clone(),
        ))
        .expect("a storage path can contain int")
        .then(store_storage_type_branch(
            "long",
            StorageNumberType::Long,
            kind,
            execute.clone(),
        ))
        .expect("a storage path can contain long")
        .then(store_storage_type_branch(
            "float",
            StorageNumberType::Float,
            kind,
            execute.clone(),
        ))
        .expect("a storage path can contain float")
        .then(store_storage_type_branch(
            "double",
            StorageNumberType::Double,
            kind,
            execute,
        ))
        .expect("a storage path can contain double");

    LiteralArgumentBuilder::literal("storage")
        .then(
            RequiredArgumentBuilder::argument("target", IdentifierArgument)
                .then(path)
                .expect("a storage target can contain a path"),
        )
        .expect("storage can contain a target")
}

fn store_storage_type_branch(
    literal: &'static str,
    number_type: StorageNumberType,
    kind: StoreKind,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal(literal)
        .then(
            RequiredArgumentBuilder::argument("scale", DoubleArgumentType::double())
                .redirect_with_modifier(
                    execute,
                    Rc::new(move |context| {
                        let scale = DoubleArgumentType::get_double(context, "scale")
                            .expect("storage store is attached below its scale argument");
                        Ok(Rc::new(context.source().with_modifier(
                            Modifier::StoreStorage {
                                kind,
                                storage: identifier(context, "target"),
                                path: nbt_path(context, "path"),
                                number_type,
                                scale,
                            },
                        )))
                    }),
                )
                .expect("a complete storage store has no children"),
        )
        .expect("a storage number type can contain a scale")
}

fn command_string(context: &CommandContext<LoweringSource>, name: &str) -> String {
    StringArgumentType::get_string(context, name)
        .expect("the command executor is attached below the requested string argument")
}

fn function_reference(context: &CommandContext<LoweringSource>) -> FunctionReference {
    context
        .argument::<FunctionReference>("name")
        .map(|reference| (*reference).clone())
        .expect("the function executor is attached below its name argument")
}

fn number_provider(
    context: &CommandContext<LoweringSource>,
    name: &str,
) -> NumberProviderReference {
    context
        .argument::<NumberProviderReference>(name)
        .map(|provider| (*provider).clone())
        .expect("the command executor is attached below its number provider argument")
}

fn position_coordinates(
    context: &CommandContext<LoweringSource>,
    name: &str,
) -> PositionCoordinates {
    context
        .argument::<PositionCoordinates>(name)
        .map(|coordinates| *coordinates)
        .expect("the command executor is attached below its position argument")
}

fn rotation_coordinates(
    context: &CommandContext<LoweringSource>,
    name: &str,
) -> RotationCoordinates {
    context
        .argument::<RotationCoordinates>(name)
        .map(|coordinates| *coordinates)
        .expect("the command executor is attached below its rotation argument")
}

fn axes(context: &CommandContext<LoweringSource>, name: &str) -> Axes {
    context
        .argument::<Axes>(name)
        .map(|axes| *axes)
        .expect("the command executor is attached below its axes argument")
}

fn identifier(context: &CommandContext<LoweringSource>, name: &str) -> Identifier {
    context
        .argument::<Identifier>(name)
        .map(|identifier| (*identifier).clone())
        .expect("the command executor is attached below the requested resource identifier")
}

fn integer_range(context: &CommandContext<LoweringSource>, name: &str) -> IntegerRange {
    context
        .argument::<IntegerRange>(name)
        .map(|range| *range)
        .expect("the command executor is attached below the requested integer range")
}

fn double_range(context: &CommandContext<LoweringSource>, name: &str) -> DoubleRange {
    context
        .argument::<DoubleRange>(name)
        .map(|range| *range)
        .expect("the command executor is attached below the requested double range")
}

fn nbt_path(context: &CommandContext<LoweringSource>, name: &str) -> NbtPath {
    context
        .argument::<NbtPath>(name)
        .map(|path| (*path).clone())
        .expect("the command executor is attached below the requested NBT path")
}

fn nbt_tag(context: &CommandContext<LoweringSource>, name: &str) -> Tag {
    context
        .argument::<Tag>(name)
        .map(|tag| (*tag).clone())
        .expect("the command executor is attached below the requested NBT tag")
}

fn compound_tag(context: &CommandContext<LoweringSource>, name: &str) -> CompoundTag {
    context
        .argument::<CompoundTag>(name)
        .map(|tag| (*tag).clone())
        .expect("the command executor is attached below the requested compound tag")
}

fn score_holders(context: &CommandContext<LoweringSource>, name: &str) -> ScoreHolderSet {
    context
        .argument::<ScoreHolderSet>(name)
        .map(|holders| (*holders).clone())
        .expect("the command executor is attached below the requested score holders argument")
}

fn score_reference(
    context: &CommandContext<LoweringSource>,
    holder: &str,
    objective: &str,
) -> ScoreReference {
    ScoreReference {
        holder: score_holders(context, holder),
        objective: command_string(context, objective),
    }
}

fn scoreboard_operation(
    context: &CommandContext<LoweringSource>,
    name: &str,
) -> ScoreboardOperation {
    context
        .argument::<ScoreboardOperation>(name)
        .map(|operation| *operation)
        .expect("the scoreboard executor is attached below its operation argument")
}

#[derive(Clone, Copy)]
struct PositionArgument;

impl ArgumentType<LoweringSource> for PositionArgument {
    type Value = PositionCoordinates;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        if reader.can_read() && reader.peek() == b'^' as u16 {
            parse_local_coordinates(reader)
        } else {
            parse_world_position(reader)
        }
    }

    fn examples(&self) -> Vec<String> {
        ["0 0 0", "~ ~ ~", "^ ^ ^", "^1 ^ ^-5"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone, Copy)]
struct RotationArgument;

impl ArgumentType<LoweringSource> for RotationArgument {
    type Value = RotationCoordinates;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        if !reader.can_read() {
            return Err(coordinate_error(reader, "incomplete rotation"));
        }
        let yaw = parse_world_coordinate(reader, false)?;
        if !reader.can_read() || reader.peek() != b' ' as u16 {
            reader.set_cursor(start);
            return Err(coordinate_error(reader, "incomplete rotation"));
        }
        reader.skip();
        let pitch = parse_world_coordinate(reader, false)?;
        Ok(RotationCoordinates { yaw, pitch })
    }

    fn examples(&self) -> Vec<String> {
        ["0 0", "~ ~", "~-5 ~5"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone, Copy)]
struct AxesArgument;

impl ArgumentType<LoweringSource> for AxesArgument {
    type Value = Axes;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let mut axes = Axes {
            x: false,
            y: false,
            z: false,
        };
        while reader.can_read() && reader.peek() != b' ' as u16 {
            let selected = match reader.read() {
                value if value == b'x' as u16 => &mut axes.x,
                value if value == b'y' as u16 => &mut axes.y,
                value if value == b'z' as u16 => &mut axes.z,
                _ => return Err(coordinate_error(reader, "invalid axes")),
            };
            if *selected {
                return Err(coordinate_error(reader, "invalid axes"));
            }
            *selected = true;
        }
        Ok(axes)
    }

    fn examples(&self) -> Vec<String> {
        ["xyz", "x"].into_iter().map(str::to_owned).collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

fn parse_world_position(
    reader: &mut StringReader,
) -> Result<PositionCoordinates, CommandSyntaxException> {
    let start = reader.cursor();
    let x = parse_world_coordinate(reader, true)?;
    if !reader.can_read() || reader.peek() != b' ' as u16 {
        reader.set_cursor(start);
        return Err(coordinate_error(reader, "incomplete position"));
    }
    reader.skip();
    let y = parse_world_coordinate(reader, false)?;
    if !reader.can_read() || reader.peek() != b' ' as u16 {
        reader.set_cursor(start);
        return Err(coordinate_error(reader, "incomplete position"));
    }
    reader.skip();
    let z = parse_world_coordinate(reader, true)?;
    Ok(PositionCoordinates::World { x, y, z })
}

fn parse_local_coordinates(
    reader: &mut StringReader,
) -> Result<PositionCoordinates, CommandSyntaxException> {
    let start = reader.cursor();
    let left = parse_local_coordinate(reader, start)?;
    if !reader.can_read() || reader.peek() != b' ' as u16 {
        reader.set_cursor(start);
        return Err(coordinate_error(reader, "incomplete position"));
    }
    reader.skip();
    let up = parse_local_coordinate(reader, start)?;
    if !reader.can_read() || reader.peek() != b' ' as u16 {
        reader.set_cursor(start);
        return Err(coordinate_error(reader, "incomplete position"));
    }
    reader.skip();
    let forwards = parse_local_coordinate(reader, start)?;
    Ok(PositionCoordinates::Local { left, up, forwards })
}

fn parse_local_coordinate(
    reader: &mut StringReader,
    start: usize,
) -> Result<f64, CommandSyntaxException> {
    if !reader.can_read() {
        return Err(coordinate_error(reader, "expected coordinate"));
    }
    if reader.peek() != b'^' as u16 {
        reader.set_cursor(start);
        return Err(coordinate_error(reader, "mixed coordinate types"));
    }
    reader.skip();
    if reader.can_read() && reader.peek() != b' ' as u16 {
        reader.read_double()
    } else {
        Ok(0.0)
    }
}

fn parse_world_coordinate(
    reader: &mut StringReader,
    center: bool,
) -> Result<WorldCoordinate, CommandSyntaxException> {
    if reader.can_read() && reader.peek() == b'^' as u16 {
        return Err(coordinate_error(reader, "mixed coordinate types"));
    }
    if !reader.can_read() {
        return Err(coordinate_error(reader, "expected coordinate"));
    }

    let relative = if reader.peek() == b'~' as u16 {
        reader.skip();
        true
    } else {
        false
    };
    let start = reader.cursor();
    let mut value = if reader.can_read() && reader.peek() != b' ' as u16 {
        reader.read_double()?
    } else {
        0.0
    };
    let number = reader.substring(start, reader.cursor());
    if !relative && center && !number.contains('.') {
        value += 0.5;
    }
    Ok(WorldCoordinate { relative, value })
}

fn coordinate_error(reader: &StringReader, message: &'static str) -> CommandSyntaxException {
    SimpleCommandExceptionType::new(LiteralMessage::new(message)).create_with_context(reader)
}

#[derive(Clone, Copy)]
struct IdentifierArgument;

impl ArgumentType<LoweringSource> for IdentifierArgument {
    type Value = Identifier;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        while reader.can_read() && is_allowed_in_identifier(reader.peek()) {
            reader.skip();
        }
        let raw = reader.substring(start, reader.cursor());
        if let Some(identifier) = Identifier::parse(&raw) {
            Ok(identifier)
        } else {
            reader.set_cursor(start);
            Err(
                SimpleCommandExceptionType::new(LiteralMessage::new("invalid resource identifier"))
                    .create_with_context(reader),
            )
        }
    }

    fn examples(&self) -> Vec<String> {
        ["foo", "foo:bar", "012"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone)]
struct NumberProviderArgument {
    registry: Arc<LootRegistry>,
}

impl NumberProviderArgument {
    fn new(registry: Arc<LootRegistry>) -> Self {
        Self { registry }
    }
}

impl ArgumentType<LoweringSource> for NumberProviderArgument {
    type Value = NumberProviderReference;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        while reader.can_read() && is_allowed_in_identifier(reader.peek()) {
            reader.skip();
        }
        if reader.cursor() != start {
            let raw = reader.substring(start, reader.cursor());
            if let Some(identifier) = Identifier::parse(&raw) {
                if self.registry.contains_number_provider(&identifier) {
                    return Ok(NumberProviderReference::Named(identifier));
                }
                return Err(SimpleCommandExceptionType::new(LiteralMessage::new(format!(
                    "number provider `{identifier}` does not exist or is outside Worldless scope"
                )))
                .create_with_context(reader));
            }
            reader.set_cursor(start);
        }

        let value = parse_nbt_argument(reader, parse_tag)?;
        parse_inline_number_provider(&value, &self.registry)
            .map(|provider| NumberProviderReference::Inline(Box::new(provider)))
            .map_err(|reason| {
                SimpleCommandExceptionType::new(LiteralMessage::new(reason))
                    .create_with_context(reader)
            })
    }

    fn examples(&self) -> Vec<String> {
        ["foo", "foo:bar", "+1", "{type:constant,value:1}"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone)]
struct PredicateArgument {
    registry: Arc<LootRegistry>,
}

impl PredicateArgument {
    fn new(registry: Arc<LootRegistry>) -> Self {
        Self { registry }
    }
}

impl ArgumentType<LoweringSource> for PredicateArgument {
    type Value = PredicateReference;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        while reader.can_read() && is_allowed_in_identifier(reader.peek()) {
            reader.skip();
        }
        if reader.cursor() != start {
            let raw = reader.substring(start, reader.cursor());
            if let Some(identifier) = Identifier::parse(&raw) {
                if self.registry.contains_predicate(&identifier) {
                    return Ok(PredicateReference::Named(identifier));
                }
                return Err(SimpleCommandExceptionType::new(LiteralMessage::new(format!(
                    "predicate `{identifier}` does not exist or is outside Worldless scope"
                )))
                .create_with_context(reader));
            }
            reader.set_cursor(start);
        }

        let value = parse_nbt_argument(reader, parse_tag)?;
        parse_inline_predicate(&value, &self.registry)
            .map(|predicate| PredicateReference::Inline(Box::new(predicate)))
            .map_err(|reason| {
                SimpleCommandExceptionType::new(LiteralMessage::new(reason))
                    .create_with_context(reader)
            })
    }

    fn examples(&self) -> Vec<String> {
        ["foo", "foo:bar", "012", "{type:all_of,terms:[]}"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone, Copy)]
struct NbtTagArgument;

impl ArgumentType<LoweringSource> for NbtTagArgument {
    type Value = Tag;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        parse_nbt_argument(reader, parse_tag)
    }

    fn examples(&self) -> Vec<String> {
        ["0", "0b", "0L", "0.0", "\"foo\"", "{foo:bar}", "[0]"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone, Copy)]
struct CompoundTagArgument;

impl ArgumentType<LoweringSource> for CompoundTagArgument {
    type Value = CompoundTag;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        parse_nbt_argument(reader, parse_compound)
    }

    fn examples(&self) -> Vec<String> {
        vec!["{}".to_owned(), "{foo:bar}".to_owned()]
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone, Copy)]
struct NbtPathArgument;

impl ArgumentType<LoweringSource> for NbtPathArgument {
    type Value = NbtPath;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        parse_nbt_argument(reader, parse_path)
    }

    fn examples(&self) -> Vec<String> {
        ["foo", "foo.bar", "foo[0]", "[0]", "[]", "{foo:bar}"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

fn parse_nbt_argument<T>(
    reader: &mut StringReader,
    parse: impl FnOnce(&mut StringReader) -> Result<T, String>,
) -> Result<T, CommandSyntaxException> {
    let start = reader.cursor();
    parse(reader).map_err(|message| {
        reader.set_cursor(start);
        SimpleCommandExceptionType::new(LiteralMessage::new(message)).create_with_context(reader)
    })
}

#[derive(Clone, Copy)]
struct ScoreHolderArgument;

impl ArgumentType<LoweringSource> for ScoreHolderArgument {
    type Value = ScoreHolderSet;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        while reader.can_read() && reader.peek() != b' ' as u16 {
            reader.skip();
        }
        let holder = JavaString::from_units(reader.substring_utf16(start, reader.cursor()));
        if holder.units() == [u16::from(b'*')] {
            Ok(ScoreHolderSet::Wildcard)
        } else if holder.units().first() == Some(&u16::from(b'#')) {
            Ok(ScoreHolderSet::Named(holder))
        } else {
            reader.set_cursor(start);
            Err(SimpleCommandExceptionType::new(LiteralMessage::new(
                "only `*` or score holders whose names start with `#` are supported",
            ))
            .create_with_context(reader))
        }
    }

    fn examples(&self) -> Vec<String> {
        vec!["#value".to_owned(), "*".to_owned()]
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone, Copy)]
struct ScoreboardOperationArgument;

impl ArgumentType<LoweringSource> for ScoreboardOperationArgument {
    type Value = ScoreboardOperation;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        while reader.can_read() && reader.peek() != b' ' as u16 {
            reader.skip();
        }
        let operation = match reader.substring(start, reader.cursor()).as_str() {
            "=" => ScoreboardOperation::Assign,
            "+=" => ScoreboardOperation::Add,
            "-=" => ScoreboardOperation::Subtract,
            "*=" => ScoreboardOperation::Multiply,
            "/=" => ScoreboardOperation::Divide,
            "%=" => ScoreboardOperation::Modulo,
            "<" => ScoreboardOperation::Min,
            ">" => ScoreboardOperation::Max,
            "><" => ScoreboardOperation::Swap,
            _ => {
                reader.set_cursor(start);
                return Err(SimpleCommandExceptionType::new(LiteralMessage::new(
                    "invalid scoreboard operation",
                ))
                .create_with_context(reader));
            }
        };
        Ok(operation)
    }

    fn examples(&self) -> Vec<String> {
        ["=", "+=", "><"].into_iter().map(str::to_owned).collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone, Copy)]
struct IntegerRangeArgument;

impl ArgumentType<LoweringSource> for IntegerRangeArgument {
    type Value = IntegerRange;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        let parsed = (|| {
            let min = parse_integer_range_bound(reader)?;
            let max = if reader.can_read_n(2)
                && reader.peek() == b'.' as u16
                && reader.peek_offset(1) == b'.' as u16
            {
                reader.skip();
                reader.skip();
                parse_integer_range_bound(reader)?
            } else {
                min
            };

            if min.is_none() && max.is_none() {
                return Err("empty integer range");
            }
            if min.zip(max).is_some_and(|(min, max)| min > max) {
                return Err("swapped integer range bounds");
            }
            Ok(IntegerRange { min, max })
        })();

        parsed.map_err(|message| {
            reader.set_cursor(start);
            SimpleCommandExceptionType::new(LiteralMessage::new(message))
                .create_with_context(reader)
        })
    }

    fn examples(&self) -> Vec<String> {
        ["0..5", "0", "-5", "-100..", "..100"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone, Copy)]
struct DoubleRangeArgument;

enum DoubleRangeParseError {
    Empty,
    Swapped,
    InvalidDouble(String),
}

impl ArgumentType<LoweringSource> for DoubleRangeArgument {
    type Value = DoubleRange;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        let parsed = (|| {
            let min = parse_double_range_bound(reader)?;
            let max = if reader.can_read_n(2)
                && reader.peek() == b'.' as u16
                && reader.peek_offset(1) == b'.' as u16
            {
                reader.skip();
                reader.skip();
                parse_double_range_bound(reader)?
            } else {
                min
            };

            if min.is_none() && max.is_none() {
                return Err(DoubleRangeParseError::Empty);
            }
            if min
                .zip(max)
                .is_some_and(|(min, max)| min.total_cmp(&max).is_gt())
            {
                return Err(DoubleRangeParseError::Swapped);
            }
            Ok(DoubleRange { min, max })
        })();

        parsed.map_err(|error| {
            reader.set_cursor(start);
            match error {
                DoubleRangeParseError::Empty => SimpleCommandExceptionType::new(
                    LiteralMessage::new("Expected value or range of values"),
                )
                .create_with_context(reader),
                DoubleRangeParseError::Swapped => SimpleCommandExceptionType::new(
                    LiteralMessage::new("Min cannot be bigger than max"),
                )
                .create_with_context(reader),
                DoubleRangeParseError::InvalidDouble(value) => BUILT_IN_EXCEPTIONS
                    .reader_invalid_double()
                    .create_with_context(reader, value),
            }
        })
    }

    fn examples(&self) -> Vec<String> {
        ["0.0..5.2", "0", "-5.4", "-100.76..", "..100"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

fn parse_double_range_bound(
    reader: &mut StringReader,
) -> Result<Option<f64>, DoubleRangeParseError> {
    let start = reader.cursor();
    while reader.can_read() && is_allowed_range_number(reader) {
        reader.skip();
    }
    if start == reader.cursor() {
        return Ok(None);
    }
    let value = reader.substring(start, reader.cursor());
    value
        .parse()
        .map(Some)
        .map_err(|_| DoubleRangeParseError::InvalidDouble(value))
}

fn parse_integer_range_bound(reader: &mut StringReader) -> Result<Option<i32>, &'static str> {
    let start = reader.cursor();
    while reader.can_read() && is_allowed_range_number(reader) {
        reader.skip();
    }
    if start == reader.cursor() {
        return Ok(None);
    }
    reader
        .substring(start, reader.cursor())
        .parse()
        .map(Some)
        .map_err(|_| "invalid integer in integer range")
}

fn is_allowed_range_number(reader: &StringReader) -> bool {
    match reader.peek() {
        unit if matches!(unit, 0x30..=0x39) || unit == b'-' as u16 => true,
        unit if unit == b'.' as u16 => {
            !reader.can_read_n(2) || reader.peek_offset(1) != b'.' as u16
        }
        _ => false,
    }
}

#[derive(Clone, Copy)]
struct FunctionArgument;

impl ArgumentType<LoweringSource> for FunctionArgument {
    type Value = FunctionReference;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        if reader.can_read() && reader.peek() == b'#' as u16 {
            reader.skip();
        }
        while reader.can_read() && is_allowed_in_identifier(reader.peek()) {
            reader.skip();
        }
        let raw = reader.substring(start, reader.cursor());
        if let Some(reference) = FunctionReference::parse(&raw) {
            Ok(reference)
        } else {
            reader.set_cursor(start);
            Err(
                SimpleCommandExceptionType::new(LiteralMessage::new("invalid function identifier"))
                    .create_with_context(reader),
            )
        }
    }

    fn examples(&self) -> Vec<String> {
        ["foo", "foo:bar", "#foo"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

fn syntax_error(message: &'static str) -> CommandSyntaxException {
    SimpleCommandExceptionType::new(LiteralMessage::new(message)).create()
}

fn reject_symbolic_links(path: &Path) -> Result<(), LoadError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(LoadError::UnsupportedPack {
            path: path.to_owned(),
            feature: "symbolic links",
        });
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    let entries = fs::read_dir(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::Io {
            path: path.to_owned(),
            source,
        })?;
        reject_symbolic_links(&entry.path())?;
    }
    Ok(())
}

fn child_directories(path: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(Vec::new());
        }
        Err(source) => {
            return Err(LoadError::Io {
                path: path.to_owned(),
                source,
            });
        }
    };
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::Io {
            path: path.to_owned(),
            source,
        })?;
        let file_type = entry.file_type().map_err(|source| LoadError::Io {
            path: entry.path(),
            source,
        })?;
        if file_type.is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    Ok(directories)
}

fn regular_files_recursive(root: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(Vec::new());
        }
        Err(source) => {
            return Err(LoadError::Io {
                path: root.to_owned(),
                source,
            });
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::Io {
            path: root.to_owned(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            paths.extend(regular_files_recursive(&path)?);
        } else if file_type.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn resource_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()?
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .map(|components| components.join("/"))
}

fn metadata(path: &Path) -> Result<fs::Metadata, LoadError> {
    fs::metadata(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })
}

fn read_to_string(path: &Path) -> Result<String, LoadError> {
    fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })
}

fn invalid_pack(path: &Path, reason: impl Into<String>) -> LoadError {
    LoadError::InvalidPack {
        path: path.to_owned(),
        reason: reason.into(),
    }
}

fn invalid_function(line: usize, reason: impl Into<String>) -> FunctionParseError {
    FunctionParseError {
        line,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_line_splitting_handles_all_buffered_reader_endings() {
        assert_eq!(java_lines("a\rb\r\nc\nd\n"), ["a", "b", "c", "d"]);
        assert_eq!(java_lines("a\n\n"), ["a", ""]);
        assert!(java_lines("").is_empty());
    }

    #[test]
    fn java_trim_does_not_remove_unicode_whitespace() {
        assert_eq!(java_trim(" \tvalue\r\n"), "value");
        assert_eq!(java_trim("\u{a0}value\u{a0}"), "\u{a0}value\u{a0}");
    }

    #[test]
    fn command_length_uses_utf16_code_units() {
        let within = "\u{1f600}".repeat(MAX_COMMAND_LENGTH / 2);
        assert!(check_command_length(1, utf16_length(&within)).is_ok());
        let beyond = format!("{within}a");
        assert!(check_command_length(1, utf16_length(&beyond)).is_err());
    }

    #[test]
    fn json_numbers_use_java_int_value_narrowing() {
        let path = Path::new("pack.mcmeta");
        for (input, expected) in [
            ("118.9", 118),
            ("1.18e2", 118),
            ("4294967414", 118),
            ("-4294967178", 118),
            ("1e9999", 0),
            ("1e-9999", 0),
        ] {
            let value: Value = serde_json::from_str(input).unwrap();
            assert_eq!(parse_i32(path, "value", &value).unwrap(), expected);
        }
        for input in ["1e10000", "1e-10000"] {
            let value: Value = serde_json::from_str(input).unwrap();
            assert!(parse_i32(path, "value", &value).is_err());
        }
        let value: Value = serde_json::from_str(&"1".repeat(10_001)).unwrap();
        assert!(parse_i32(path, "value", &value).is_err());
    }

    #[test]
    fn compiler_lowers_only_the_supported_command_set() {
        let compiler = CommandCompiler::new();
        let instruction = compiler.compile("function example:child").unwrap();
        assert!(instruction.modifiers.is_empty());
        assert!(matches!(
            instruction.command,
            CompiledCommand::Function {
                arguments: None,
                ..
            }
        ));

        let instruction = compiler.compile("return -7").unwrap();
        assert!(instruction.modifiers.is_empty());
        assert!(matches!(
            instruction.command,
            CompiledCommand::Return {
                success: true,
                value: -7
            }
        ));

        let instruction = compiler.compile("return fail").unwrap();
        assert!(instruction.modifiers.is_empty());
        assert!(matches!(
            instruction.command,
            CompiledCommand::Return {
                success: false,
                value: 0
            }
        ));

        let instruction = compiler
            .compile("return run return run function example:child")
            .unwrap();
        assert!(matches!(
            instruction.modifiers.as_slice(),
            [Modifier::ReturnRun, Modifier::ReturnRun]
        ));
        assert!(matches!(
            instruction.command,
            CompiledCommand::Function {
                arguments: None,
                ..
            }
        ));

        assert!(matches!(
            compiler
                .compile("scoreboard objectives add values dummy")
                .unwrap()
                .command,
            CompiledCommand::Scoreboard(ScoreboardCommand::AddObjective { ref objective })
                if objective == "values"
        ));
        assert!(matches!(
            compiler
                .compile("scoreboard players set #value values -7")
                .unwrap()
                .command,
            CompiledCommand::Scoreboard(ScoreboardCommand::SetScore {
                holders: ScoreHolderSet::Named(ref holder),
                ref objective,
                value: -7
            }) if holder == "#value" && objective == "values"
        ));
        assert!(matches!(
            compiler
                .compile("return run scoreboard players get #value values")
                .unwrap(),
            Instruction {
                ref modifiers,
                command: CompiledCommand::Scoreboard(ScoreboardCommand::GetScore {
                    holder: ScoreHolderSet::Named(ref holder),
                    ref objective
                })
            } if matches!(modifiers.as_slice(), [Modifier::ReturnRun])
                && holder == "#value"
                && objective == "values"
        ));

        assert!(matches!(
            compiler.compile("function #example:tag").unwrap().command,
            CompiledCommand::Function {
                ref reference,
                arguments: None
            } if reference.to_string() == "#example:tag"
        ));
        assert!(matches!(
            compiler
                .compile("scoreboard objectives list")
                .unwrap()
                .command,
            CompiledCommand::Scoreboard(ScoreboardCommand::ListObjectives)
        ));
        assert!(
            compiler
                .compile("scoreboard objectives add values trigger")
                .is_err()
        );
        assert!(
            compiler
                .compile("scoreboard players set Player values 1")
                .is_err()
        );
        assert!(
            compiler
                .compile("scoreboard players get @s values")
                .is_err()
        );
        assert!(matches!(
            compiler
                .compile("scoreboard players get * values")
                .unwrap()
                .command,
            CompiledCommand::Scoreboard(ScoreboardCommand::GetScore {
                holder: ScoreHolderSet::Wildcard,
                ..
            })
        ));
    }

    #[test]
    fn compiler_lowers_random_value_and_reset_forms() {
        let compiler = CommandCompiler::new();

        assert!(matches!(
            compiler.compile("random value -4..9").unwrap().command,
            CompiledCommand::Random(RandomCommand::Value {
                range: IntegerRange {
                    min: Some(-4),
                    max: Some(9)
                },
                sequence: None
            })
        ));
        assert!(matches!(
            compiler
                .compile("random value ..9 example:sequence")
                .unwrap()
                .command,
            CompiledCommand::Random(RandomCommand::Value {
                range: IntegerRange {
                    min: None,
                    max: Some(9)
                },
                sequence: Some(ref sequence)
            }) if sequence.to_string() == "example:sequence"
        ));
        assert!(matches!(
            compiler.compile("random reset *").unwrap().command,
            CompiledCommand::Random(RandomCommand::ResetAll { settings: None })
        ));
        assert!(matches!(
            compiler.compile("random reset * -7 false").unwrap().command,
            CompiledCommand::Random(RandomCommand::ResetAll {
                settings: Some(RandomSequenceSettings {
                    salt: -7,
                    include_world_seed: false,
                    include_sequence_id: true
                })
            })
        ));
        assert!(matches!(
            compiler
                .compile("random reset sequence 12 true false")
                .unwrap()
                .command,
            CompiledCommand::Random(RandomCommand::Reset {
                ref sequence,
                settings: Some(RandomSequenceSettings {
                    salt: 12,
                    include_world_seed: true,
                    include_sequence_id: false
                })
            }) if sequence.to_string() == "minecraft:sequence"
        ));

        for valid_runtime_error in ["random value 5", "random value -1073741824..1073741823"] {
            assert!(compiler.compile(valid_runtime_error).is_ok());
        }
        for invalid in [
            "random value ..",
            "random value 5..4",
            "random value 1..2 Invalid:sequence",
            "random reset",
            "random reset * 0 maybe",
            "random reset sequence 0 true false extra",
        ] {
            assert!(compiler.compile(invalid).is_err(), "accepted {invalid:?}");
        }
        assert!(
            compiler
                .compile("random roll 1..2 example:sequence")
                .unwrap_err()
                .contains("outside Worldless scope")
        );
    }

    #[test]
    fn compiler_lowers_stopwatch_commands_and_conditions() {
        let compiler = CommandCompiler::new();

        assert!(matches!(
            compiler
                .compile("stopwatch create example:timer")
                .unwrap()
                .command,
            CompiledCommand::Stopwatch(StopwatchCommand::Create { ref id })
                if id.to_string() == "example:timer"
        ));
        assert!(matches!(
            compiler
                .compile("stopwatch query example:timer")
                .unwrap()
                .command,
            CompiledCommand::Stopwatch(StopwatchCommand::Query { ref id, scale })
                if id.to_string() == "example:timer" && scale == 1.0
        ));
        assert!(matches!(
            compiler
                .compile("stopwatch query example:timer -2.5")
                .unwrap()
                .command,
            CompiledCommand::Stopwatch(StopwatchCommand::Query { scale: -2.5, .. })
        ));
        assert!(matches!(
            compiler
                .compile("stopwatch restart example:timer")
                .unwrap()
                .command,
            CompiledCommand::Stopwatch(StopwatchCommand::Restart { .. })
        ));
        assert!(matches!(
            compiler
                .compile("stopwatch remove example:timer")
                .unwrap()
                .command,
            CompiledCommand::Stopwatch(StopwatchCommand::Remove { .. })
        ));

        let terminal = compiler
            .compile("execute unless stopwatch example:timer ..1.25")
            .unwrap();
        assert!(terminal.modifiers.is_empty());
        assert!(matches!(
            terminal.command,
            CompiledCommand::StopwatchCondition(StopwatchCondition {
                expected: false,
                range: DoubleRange {
                    min: None,
                    max: Some(1.25)
                },
                ..
            })
        ));

        let conditional = compiler
            .compile("execute if stopwatch example:timer -0.0..0.0 run return 3")
            .unwrap();
        assert!(matches!(
            conditional.modifiers.as_slice(),
            [Modifier::StopwatchCondition(StopwatchCondition {
                expected: true,
                range: DoubleRange {
                    min: Some(min),
                    max: Some(max)
                },
                ..
            })] if min.is_sign_negative() && *max == 0.0 && !max.is_sign_negative()
        ));
        assert!(matches!(
            conditional.command,
            CompiledCommand::Return {
                success: true,
                value: 3
            }
        ));
    }

    #[test]
    fn compiler_rejects_invalid_stopwatch_syntax() {
        let compiler = CommandCompiler::new();

        for invalid in [
            "stopwatch",
            "stopwatch create",
            "stopwatch create Invalid:timer",
            "stopwatch query example:timer 1 extra",
            "stopwatch restart example:timer extra",
            "stopwatch remove example:timer extra",
            "execute if stopwatch example:timer",
            "execute if stopwatch example:timer ..",
            "execute if stopwatch example:timer 2..1",
            "execute if stopwatch example:timer 0..-0",
            "execute if stopwatch example:timer 1e2",
        ] {
            assert!(compiler.compile(invalid).is_err(), "accepted {invalid:?}");
        }

        for (invalid, expected_message) in [
            (
                "execute if stopwatch example:timer ..",
                "Expected value or range of values",
            ),
            (
                "execute if stopwatch example:timer 2..1",
                "Min cannot be bigger than max",
            ),
            ("execute if stopwatch example:timer .", "Invalid double '.'"),
        ] {
            let error = compiler.compile(invalid).unwrap_err();
            assert!(
                error.starts_with(expected_message),
                "unexpected error for {invalid:?}: {error}"
            );
        }
    }

    #[test]
    fn compiler_preserves_result_action_order() {
        let compiler = CommandCompiler::new();
        let instruction = compiler
            .compile(
                "execute store result score #first values store success score #second values run return run scoreboard players get #source values",
            )
            .unwrap();
        assert!(matches!(
            instruction.modifiers.as_slice(),
            [
                Modifier::StoreScore {
                    kind: StoreKind::Result,
                    holders: ScoreHolderSet::Named(first),
                    objective: first_objective
                },
                Modifier::StoreScore {
                    kind: StoreKind::Success,
                    holders: ScoreHolderSet::Named(second),
                    objective: second_objective
                },
                Modifier::ReturnRun
            ] if first == "#first"
                && first_objective == "values"
                && second == "#second"
                && second_objective == "values"
        ));
        assert!(matches!(
            instruction.command,
            CompiledCommand::Scoreboard(ScoreboardCommand::GetScore {
                holder: ScoreHolderSet::Named(ref holder),
                ref objective
            }) if holder == "#source" && objective == "values"
        ));

        let instruction = compiler
            .compile(
                "return run execute store success score #result values run function example:child",
            )
            .unwrap();
        assert!(matches!(
            instruction.modifiers.as_slice(),
            [
                Modifier::ReturnRun,
                Modifier::StoreScore {
                    kind: StoreKind::Success,
                    ..
                }
            ]
        ));
        assert!(
            compiler
                .compile("execute store result score Player values run return 1")
                .is_err()
        );
    }

    #[test]
    fn compiler_lowers_storage_commands() {
        let compiler = CommandCompiler::new();

        assert!(matches!(
            compiler
                .compile("data merge storage example:state {answer:42}")
                .unwrap()
                .command,
            CompiledCommand::Data(DataCommand::Merge { ref storage, .. })
                if storage.to_string() == "example:state"
        ));
        assert!(matches!(
            compiler
                .compile("data get storage example:state")
                .unwrap()
                .command,
            CompiledCommand::Data(DataCommand::Get { ref storage })
                if storage.to_string() == "example:state"
        ));
        assert!(matches!(
            compiler
                .compile("data get storage example:state answer 2.5")
                .unwrap()
                .command,
            CompiledCommand::Data(DataCommand::GetPath {
                scale: Some(2.5),
                ..
            })
        ));
        assert!(matches!(
            compiler
                .compile("data remove storage example:state answer")
                .unwrap()
                .command,
            CompiledCommand::Data(DataCommand::Remove { .. })
        ));

        for operation in ["insert -2", "prepend", "append", "set", "merge"] {
            for source in [
                "value {value:1}",
                "from storage example:source values[]",
                "string storage example:source text -2 -1",
            ] {
                let instruction = compiler
                    .compile(&format!(
                        "data modify storage example:state target {operation} {source}"
                    ))
                    .unwrap();
                assert!(matches!(
                    instruction.command,
                    CompiledCommand::Data(DataCommand::Modify { .. })
                ));
            }
        }

        let terminal = compiler
            .compile("execute if data storage example:state answer")
            .unwrap();
        assert!(terminal.modifiers.is_empty());
        assert!(matches!(
            terminal.command,
            CompiledCommand::StorageCondition(StorageCondition { expected: true, .. })
        ));

        let conditional = compiler
            .compile("execute unless data storage example:state answer run return 3")
            .unwrap();
        assert!(matches!(
            conditional.modifiers.as_slice(),
            [Modifier::StorageCondition(StorageCondition {
                expected: false,
                ..
            })]
        ));

        for (literal, expected) in [
            ("byte", StorageNumberType::Byte),
            ("short", StorageNumberType::Short),
            ("int", StorageNumberType::Int),
            ("long", StorageNumberType::Long),
            ("float", StorageNumberType::Float),
            ("double", StorageNumberType::Double),
        ] {
            let instruction = compiler
                .compile(&format!(
                    "execute store result storage example:state answer {literal} -0.5 run return 1"
                ))
                .unwrap();
            assert!(matches!(
                instruction.modifiers.as_slice(),
                [Modifier::StoreStorage {
                    kind: StoreKind::Result,
                    number_type,
                    scale,
                    ..
                }] if *number_type == expected && *scale == -0.5
            ));
        }

        for command in [
            "data get entity @s",
            "data get block 0 0 0",
            "data modify storage example:state value set compute block 0 0 0 example:number",
            "execute store result entity @s value int 1 run return 1",
            "execute if data entity @s value run return 1",
        ] {
            assert!(compiler.compile(command).is_err(), "{command}");
        }
    }

    #[test]
    fn compiler_lowers_function_conditions_as_nonterminal_modifiers() {
        let compiler = CommandCompiler::new();
        let instruction = compiler
            .compile(
                "execute if function example:first unless function #example:second run return 3",
            )
            .unwrap();

        assert!(matches!(
            instruction.modifiers.as_slice(),
            [
                Modifier::FunctionCondition {
                    expected: true,
                    function: first
                },
                Modifier::FunctionCondition {
                    expected: false,
                    function: second
                }
            ] if first.to_string() == "example:first" && second.to_string() == "#example:second"
        ));
        assert!(matches!(
            instruction.command,
            CompiledCommand::Return {
                success: true,
                value: 3
            }
        ));
        assert!(
            compiler
                .compile("execute if function example:first")
                .is_err()
        );
        assert!(
            compiler
                .compile("execute unless function ##example:tag run return 1")
                .is_err()
        );
    }

    #[test]
    fn continuation_precedes_comment_handling_and_requires_a_following_line() {
        let compiler = CommandCompiler::new();
        let function = parse_function(
            "# consumes the next line\\\nreturn 1\nreturn 2\n",
            &compiler,
        )
        .unwrap();
        let Function::Plain(instructions) = function else {
            panic!("a function without macro lines is plain")
        };
        assert!(matches!(
            instructions.as_ref(),
            [Instruction {
                modifiers,
                command: CompiledCommand::Return {
                    success: true,
                    value: 2
                }
            }] if modifiers.is_empty()
        ));
        assert!(parse_function("return 1\\", &compiler).is_err());
    }
}
