use std::{
    any::{Any, type_name},
    cell::{Ref, RefCell},
    error::Error,
    fmt,
    rc::Rc,
};

use crate::{
    arguments::ArgumentValueComparator,
    dispatcher::CommandDispatcher,
    exceptions::CommandSyntaxException,
    tree::{Command, Node, RedirectModifier, SINGLE_SUCCESS},
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StringRange {
    start: usize,
    end: usize,
}

impl StringRange {
    pub const fn at(position: usize) -> Self {
        Self {
            start: position,
            end: position,
        }
    }

    pub const fn between(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn encompassing(left: Self, right: Self) -> Self {
        Self {
            start: if left.start < right.start {
                left.start
            } else {
                right.start
            },
            end: if left.end > right.end {
                left.end
            } else {
                right.end
            },
        }
    }

    pub const fn start(self) -> usize {
        self.start
    }

    pub const fn end(self) -> usize {
        self.end
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub const fn len(self) -> i32 {
        (self.end as i32).wrapping_sub(self.start as i32)
    }

    pub const fn java_hash_code(self) -> i32 {
        31_i32
            .wrapping_mul(31_i32.wrapping_add(self.start as i32))
            .wrapping_add(self.end as i32)
    }

    pub fn get(self, input: &str) -> Result<&str, ContextError> {
        let start = utf16_to_byte(input, self.start).ok_or(ContextError::InvalidRange(self))?;
        let end = utf16_to_byte(input, self.end).ok_or(ContextError::InvalidRange(self))?;
        input
            .get(start..end)
            .ok_or(ContextError::InvalidRange(self))
    }

    pub fn get_utf16(self, input: &[u16]) -> Result<&[u16], ContextError> {
        input
            .get(self.start..self.end)
            .ok_or(ContextError::InvalidRange(self))
    }
}

fn utf16_to_byte(input: &str, position: usize) -> Option<usize> {
    if position == 0 {
        return Some(0);
    }
    let mut utf16 = 0;
    for (byte, character) in input.char_indices() {
        if utf16 == position {
            return Some(byte);
        }
        utf16 += character.len_utf16();
        if utf16 > position {
            return None;
        }
    }
    (utf16 == position).then_some(input.len())
}

#[derive(Clone)]
pub struct ParsedArgument {
    range: StringRange,
    result: Rc<dyn Any>,
    type_name: &'static str,
    value_equality: ArgumentValueComparator,
}

impl ParsedArgument {
    pub fn new<T: Any + PartialEq>(start: usize, end: usize, result: T) -> Self {
        Self::from_rc_with_equality(
            start,
            end,
            Rc::new(result),
            type_name::<T>(),
            Rc::new(|left, right| {
                left.downcast_ref::<T>()
                    .zip(right.downcast_ref::<T>())
                    .is_some_and(|(left, right)| left == right)
            }),
        )
    }

    pub fn identity<T: Any>(start: usize, end: usize, result: T) -> Self {
        Self::from_rc_with_equality(
            start,
            end,
            Rc::new(result),
            type_name::<T>(),
            Rc::new(|_, _| false),
        )
    }

    pub fn from_rc_identity(start: usize, end: usize, result: Rc<dyn Any>) -> Self {
        Self::from_rc_with_equality(
            start,
            end,
            result,
            "erased argument value",
            Rc::new(|_, _| false),
        )
    }

    pub(crate) fn from_rc_with_equality(
        start: usize,
        end: usize,
        result: Rc<dyn Any>,
        type_name: &'static str,
        value_equality: ArgumentValueComparator,
    ) -> Self {
        Self {
            range: StringRange::between(start, end),
            result,
            type_name,
            value_equality,
        }
    }

    pub const fn range(&self) -> StringRange {
        self.range
    }

    pub fn result<T: Any>(&self) -> Option<Rc<T>> {
        self.result.clone().downcast().ok()
    }

    pub fn erased_result(&self) -> Rc<dyn Any> {
        self.result.clone()
    }

    pub const fn result_type_name(&self) -> &'static str {
        self.type_name
    }

    fn java_equals(&self, other: &Self) -> bool {
        self.range == other.range
            && (Rc::ptr_eq(&self.result, &other.result)
                || any_java_builtin_equals(&self.result, &other.result).unwrap_or_else(|| {
                    (self.value_equality)(self.result.as_ref(), other.result.as_ref())
                }))
    }
}

impl fmt::Debug for ParsedArgument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ParsedArgument")
            .field("range", &self.range)
            .field("type_name", &self.type_name)
            .finish_non_exhaustive()
    }
}

impl PartialEq for ParsedArgument {
    fn eq(&self, other: &Self) -> bool {
        self.java_equals(other)
    }
}

fn any_java_builtin_equals(left: &Rc<dyn Any>, right: &Rc<dyn Any>) -> Option<bool> {
    if Rc::ptr_eq(left, right) {
        return Some(true);
    }
    macro_rules! compare {
        ($type:ty) => {
            if let (Some(left), Some(right)) =
                (left.downcast_ref::<$type>(), right.downcast_ref::<$type>())
            {
                return Some(left == right);
            }
        };
    }
    compare!(bool);
    compare!(i8);
    compare!(i16);
    compare!(i32);
    compare!(i64);
    compare!(u8);
    compare!(u16);
    compare!(char);
    compare!(String);
    if let (Some(left), Some(right)) = (left.downcast_ref::<f32>(), right.downcast_ref::<f32>()) {
        return Some((left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits());
    }
    if let (Some(left), Some(right)) = (left.downcast_ref::<f64>(), right.downcast_ref::<f64>()) {
        return Some((left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits());
    }
    None
}

pub struct ParsedCommandNode<S: 'static> {
    node: Node<S>,
    range: StringRange,
}

impl<S: 'static> Clone for ParsedCommandNode<S> {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
            range: self.range,
        }
    }
}

impl<S: 'static> ParsedCommandNode<S> {
    pub fn new(node: Node<S>, range: StringRange) -> Self {
        Self { node, range }
    }

    pub const fn range(&self) -> StringRange {
        self.range
    }

    pub fn node(&self) -> &Node<S> {
        &self.node
    }
}

impl<S: 'static> fmt::Debug for ParsedCommandNode<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{:?}", self.node, self.range)
    }
}

impl<S: 'static> PartialEq for ParsedCommandNode<S> {
    fn eq(&self, other: &Self) -> bool {
        self.range == other.range && self.node.java_equals(&other.node)
    }
}

pub type ResultConsumer<S> = Rc<dyn Fn(&CommandContext<S>, bool, i32)>;

pub struct CommandContext<S: 'static> {
    source: Rc<S>,
    input: Rc<[u16]>,
    command: Option<Command<S>>,
    arguments: Rc<[(String, ParsedArgument)]>,
    root_node: Node<S>,
    nodes: Rc<[ParsedCommandNode<S>]>,
    range: StringRange,
    child: Option<Rc<Self>>,
    modifier: Option<RedirectModifier<S>>,
    forks: bool,
}

impl<S: 'static> Clone for CommandContext<S> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            input: self.input.clone(),
            command: self.command.clone(),
            arguments: self.arguments.clone(),
            root_node: self.root_node.clone(),
            nodes: self.nodes.clone(),
            range: self.range,
            child: self.child.clone(),
            modifier: self.modifier.clone(),
            forks: self.forks,
        }
    }
}

impl<S: 'static> CommandContext<S> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: S,
        input: impl AsRef<str>,
        arguments: Vec<(String, ParsedArgument)>,
        command: Option<Command<S>>,
        root_node: Node<S>,
        nodes: Vec<ParsedCommandNode<S>>,
        range: StringRange,
        child: Option<Self>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Self {
        Self::from_utf16(
            source,
            input.as_ref().encode_utf16().collect(),
            arguments,
            command,
            root_node,
            nodes,
            range,
            child,
            modifier,
            forks,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_utf16(
        source: S,
        input: Vec<u16>,
        arguments: Vec<(String, ParsedArgument)>,
        command: Option<Command<S>>,
        root_node: Node<S>,
        nodes: Vec<ParsedCommandNode<S>>,
        range: StringRange,
        child: Option<Self>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Self {
        Self {
            source: Rc::new(source),
            input: input.into(),
            command,
            arguments: arguments.into(),
            root_node,
            nodes: nodes.into(),
            range,
            child: child.map(Rc::new),
            modifier,
            forks,
        }
    }

    pub fn source(&self) -> &S {
        self.source.as_ref()
    }

    pub fn shared_source(&self) -> Rc<S> {
        self.source.clone()
    }

    pub fn input(&self) -> String {
        String::from_utf16_lossy(&self.input)
    }

    pub fn input_utf16(&self) -> &[u16] {
        &self.input
    }

    pub fn command(&self) -> Option<Command<S>> {
        self.command.clone()
    }

    pub fn argument<T: Any>(&self, name: &str) -> Result<Rc<T>, ContextError> {
        let argument = self.parsed_argument(name)?;
        argument
            .result()
            .ok_or_else(|| ContextError::WrongArgumentType {
                name: name.to_owned(),
                expected: type_name::<T>(),
                actual: argument.result_type_name(),
            })
    }

    pub fn argument_any(&self, name: &str) -> Result<Rc<dyn Any>, ContextError> {
        self.parsed_argument(name)
            .map(ParsedArgument::erased_result)
    }

    fn parsed_argument(&self, name: &str) -> Result<&ParsedArgument, ContextError> {
        self.arguments
            .iter()
            .find_map(|(argument_name, argument)| (argument_name == name).then_some(argument))
            .ok_or_else(|| ContextError::MissingArgument(name.to_owned()))
    }

    pub fn arguments(&self) -> &[(String, ParsedArgument)] {
        &self.arguments
    }

    pub fn root_node(&self) -> &Node<S> {
        &self.root_node
    }

    pub fn nodes(&self) -> &[ParsedCommandNode<S>] {
        &self.nodes
    }

    pub const fn range(&self) -> StringRange {
        self.range
    }

    pub fn child(&self) -> Option<&Self> {
        self.child.as_deref()
    }

    pub fn last_child(&self) -> &Self {
        let mut result = self;
        while let Some(child) = result.child() {
            result = child;
        }
        result
    }

    pub fn redirect_modifier(&self) -> Option<RedirectModifier<S>> {
        self.modifier.clone()
    }

    pub const fn is_forked(&self) -> bool {
        self.forks
    }

    pub fn has_nodes(&self) -> bool {
        !self.nodes.is_empty()
    }

    pub fn copy_for(&self, source: S) -> Self {
        self.copy_for_shared(Rc::new(source))
    }

    fn copy_for_shared(&self, source: Rc<S>) -> Self {
        let mut copy = self.clone();
        copy.source = source;
        copy
    }

    pub fn java_equals(&self, other: &Self) -> bool
    where
        S: PartialEq,
    {
        self.source == other.source
            && ordered_map_equals(&self.arguments, &other.arguments)
            && self.root_node.java_equals(&other.root_node)
            && self.nodes.as_ref() == other.nodes.as_ref()
            && callback_eq(&self.command, &other.command)
            && match (&self.child, &other.child) {
                (Some(left), Some(right)) => left.java_equals(right),
                (None, None) => true,
                _ => false,
            }
    }
}

impl<S: fmt::Debug + 'static> fmt::Debug for CommandContext<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandContext")
            .field("source", &self.source)
            .field("input", &self.input)
            .field("arguments", &self.arguments)
            .field("root_node", &self.root_node)
            .field("nodes", &self.nodes)
            .field("range", &self.range)
            .field("forks", &self.forks)
            .finish_non_exhaustive()
    }
}

fn callback_eq<T: ?Sized>(left: &Option<Rc<T>>, right: &Option<Rc<T>>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => Rc::ptr_eq(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn ordered_map_equals(
    left: &[(String, ParsedArgument)],
    right: &[(String, ParsedArgument)],
) -> bool {
    left.len() == right.len()
        && left.iter().all(|(name, value)| {
            right
                .iter()
                .find_map(|(other_name, other)| (name == other_name).then_some(other))
                == Some(value)
        })
}

pub struct CommandContextBuilder<S: 'static>(Rc<RefCell<CommandContextBuilderData<S>>>);

struct CommandContextBuilderData<S: 'static> {
    arguments: Vec<(String, ParsedArgument)>,
    dispatcher: CommandDispatcher<S>,
    root_node: Node<S>,
    nodes: Vec<ParsedCommandNode<S>>,
    source: Rc<S>,
    command: Option<Command<S>>,
    child: Option<CommandContextBuilder<S>>,
    range: StringRange,
    modifier: Option<RedirectModifier<S>>,
    forks: bool,
}

impl<S: 'static> Clone for CommandContextBuilder<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S: 'static> CommandContextBuilder<S> {
    pub fn new(
        dispatcher: CommandDispatcher<S>,
        source: S,
        root_node: Node<S>,
        start: usize,
    ) -> Self {
        Self::new_shared(dispatcher, Rc::new(source), root_node, start)
    }

    pub(crate) fn new_shared(
        dispatcher: CommandDispatcher<S>,
        source: Rc<S>,
        root_node: Node<S>,
        start: usize,
    ) -> Self {
        Self(Rc::new(RefCell::new(CommandContextBuilderData {
            arguments: Vec::new(),
            dispatcher,
            root_node,
            nodes: Vec::new(),
            source,
            command: None,
            child: None,
            range: StringRange::at(start),
            modifier: None,
            forks: false,
        })))
    }

    pub fn copy(&self) -> Self {
        let data = self.0.borrow();
        Self(Rc::new(RefCell::new(CommandContextBuilderData {
            arguments: data.arguments.clone(),
            dispatcher: data.dispatcher.clone(),
            root_node: data.root_node.clone(),
            nodes: data.nodes.clone(),
            source: data.source.clone(),
            command: data.command.clone(),
            child: data.child.clone(),
            range: data.range,
            modifier: None,
            forks: data.forks,
        })))
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn source(&self) -> Rc<S> {
        self.0.borrow().source.clone()
    }

    pub fn dispatcher(&self) -> CommandDispatcher<S> {
        self.0.borrow().dispatcher.clone()
    }

    pub fn with_source(&mut self, source: S) -> &mut Self {
        self.0.borrow_mut().source = Rc::new(source);
        self
    }

    pub fn root_node(&self) -> Node<S> {
        self.0.borrow().root_node.clone()
    }

    pub fn arguments(&self) -> Ref<'_, [(String, ParsedArgument)]> {
        Ref::map(self.0.borrow(), |data| data.arguments.as_slice())
    }

    pub fn with_argument(
        &mut self,
        name: impl Into<String>,
        argument: ParsedArgument,
    ) -> &mut Self {
        let name = name.into();
        let mut data = self.0.borrow_mut();
        if let Some((_, existing)) = data
            .arguments
            .iter_mut()
            .find(|(argument_name, _)| argument_name == &name)
        {
            *existing = argument;
        } else {
            data.arguments.push((name, argument));
        }
        drop(data);
        self
    }

    pub fn command(&self) -> Option<Command<S>> {
        self.0.borrow().command.clone()
    }

    pub fn with_command(&mut self, command: Option<Command<S>>) -> &mut Self {
        self.0.borrow_mut().command = command;
        self
    }

    pub fn with_node(&mut self, node: Node<S>, range: StringRange) -> &mut Self {
        let modifier = node.redirect_modifier();
        let forks = node.is_fork();
        let mut data = self.0.borrow_mut();
        data.modifier = modifier;
        data.forks = forks;
        data.nodes.push(ParsedCommandNode { node, range });
        data.range = StringRange::encompassing(data.range, range);
        drop(data);
        self
    }

    pub fn nodes(&self) -> Ref<'_, [ParsedCommandNode<S>]> {
        Ref::map(self.0.borrow(), |data| data.nodes.as_slice())
    }

    pub fn range(&self) -> StringRange {
        self.0.borrow().range
    }

    pub fn with_child(&mut self, child: Self) -> &mut Self {
        self.0.borrow_mut().child = Some(child);
        self
    }

    pub fn child(&self) -> Option<Self> {
        self.0.borrow().child.clone()
    }

    pub fn last_child(&self) -> Self {
        let mut result = self.clone();
        while let Some(child) = result.child() {
            result = child;
        }
        result
    }

    pub fn build(&self, input: impl AsRef<str>) -> CommandContext<S> {
        self.build_utf16(input.as_ref().encode_utf16().collect::<Vec<_>>())
    }

    pub fn build_utf16(&self, input: impl Into<Vec<u16>>) -> CommandContext<S> {
        self.build_with_input(Rc::from(input.into()))
    }

    pub(crate) fn build_with_input(&self, input: Rc<[u16]>) -> CommandContext<S> {
        let (source, command, arguments, root_node, nodes, range, child, modifier, forks) = {
            let data = self.0.borrow();
            (
                data.source.clone(),
                data.command.clone(),
                data.arguments.clone(),
                data.root_node.clone(),
                data.nodes.clone(),
                data.range,
                data.child.clone(),
                data.modifier.clone(),
                data.forks,
            )
        };
        CommandContext {
            source,
            input: input.clone(),
            command,
            arguments: arguments.into(),
            root_node,
            nodes: nodes.into(),
            range,
            child: child.map(|child| Rc::new(child.build_with_input(input))),
            modifier,
            forks,
        }
    }

    pub fn find_suggestion_context(
        &self,
        cursor: usize,
    ) -> Result<SuggestionContext<S>, ContextError> {
        let (range, child, nodes, root_node) = {
            let data = self.0.borrow();
            (
                data.range,
                data.child.clone(),
                data.nodes.clone(),
                data.root_node.clone(),
            )
        };
        if range.start() > cursor {
            return Err(ContextError::SuggestionCursor(cursor));
        }
        if range.end() < cursor {
            if let Some(child) = child {
                return child.find_suggestion_context(cursor);
            }
            if let Some(last) = nodes.last() {
                return Ok(SuggestionContext {
                    context: self.clone(),
                    parent: last.node.clone(),
                    start_pos: last.range.end() + 1,
                });
            }
            return Ok(SuggestionContext {
                context: self.clone(),
                parent: root_node,
                start_pos: range.start(),
            });
        }

        let mut previous = root_node;
        for node in &nodes {
            if node.range.start() <= cursor && cursor <= node.range.end() {
                return Ok(SuggestionContext {
                    context: self.clone(),
                    parent: previous,
                    start_pos: node.range.start(),
                });
            }
            previous = node.node.clone();
        }
        Ok(SuggestionContext {
            context: self.clone(),
            parent: previous,
            start_pos: range.start(),
        })
    }
}

pub struct SuggestionContext<S: 'static> {
    pub context: CommandContextBuilder<S>,
    pub parent: Node<S>,
    pub start_pos: usize,
}

impl<S: 'static> Clone for SuggestionContext<S> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            parent: self.parent.clone(),
            start_pos: self.start_pos,
        }
    }
}

impl<S: 'static> SuggestionContext<S> {
    pub fn new(context: CommandContextBuilder<S>, parent: Node<S>, start_pos: usize) -> Self {
        Self {
            context,
            parent,
            start_pos,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextError {
    MissingArgument(String),
    WrongArgumentType {
        name: String,
        expected: &'static str,
        actual: &'static str,
    },
    InvalidRange(StringRange),
    SuggestionCursor(usize),
    MissingExecutable,
}

impl fmt::Display for ContextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingArgument(name) => {
                write!(
                    formatter,
                    "no such argument {name:?} exists on this command"
                )
            }
            Self::WrongArgumentType {
                name,
                expected,
                actual,
            } => write!(
                formatter,
                "argument {name:?} is {actual}, not requested type {expected}"
            ),
            Self::InvalidRange(range) => write!(formatter, "invalid UTF-16 range {range:?}"),
            Self::SuggestionCursor(cursor) => {
                write!(
                    formatter,
                    "cannot find a command node before cursor {cursor}"
                )
            }
            Self::MissingExecutable => {
                formatter.write_str("last command in a context chain must be executable")
            }
        }
    }
}

impl Error for ContextError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Modify,
    Execute,
}

pub struct ContextChain<S: 'static> {
    modifiers: Vec<CommandContext<S>>,
    executable: CommandContext<S>,
}

impl<S: 'static> Clone for ContextChain<S> {
    fn clone(&self) -> Self {
        Self {
            modifiers: self.modifiers.clone(),
            executable: self.executable.clone(),
        }
    }
}

impl<S: 'static> ContextChain<S> {
    pub fn new(
        modifiers: Vec<CommandContext<S>>,
        executable: CommandContext<S>,
    ) -> Result<Self, ContextError> {
        if executable.command().is_none() {
            return Err(ContextError::MissingExecutable);
        }
        Ok(Self {
            modifiers,
            executable,
        })
    }

    pub fn try_flatten(root: CommandContext<S>) -> Option<Self> {
        let mut modifiers = Vec::new();
        let mut current = root;
        loop {
            let Some(child) = current.child.clone() else {
                return Self::new(modifiers, current).ok();
            };
            modifiers.push(current);
            current = child.as_ref().clone();
        }
    }

    pub fn stage(&self) -> Stage {
        if self.modifiers.is_empty() {
            Stage::Execute
        } else {
            Stage::Modify
        }
    }

    pub fn top_context(&self) -> &CommandContext<S> {
        self.modifiers.first().unwrap_or(&self.executable)
    }

    pub fn next_stage(&self) -> Option<Self> {
        (!self.modifiers.is_empty()).then(|| {
            Self::new(self.modifiers[1..].to_vec(), self.executable.clone())
                .expect("an existing context chain has an executable command")
        })
    }

    pub fn run_modifier(
        context: &CommandContext<S>,
        source: S,
        consumer: &ResultConsumer<S>,
        forked: bool,
    ) -> Result<Vec<Rc<S>>, CommandSyntaxException> {
        let Some(modifier) = context.redirect_modifier() else {
            return Ok(vec![Rc::new(source)]);
        };
        let context = context.copy_for(source);
        Self::apply_modifier(modifier, context, consumer, forked)
    }

    fn run_modifier_shared(
        context: &CommandContext<S>,
        source: Rc<S>,
        consumer: &ResultConsumer<S>,
        forked: bool,
    ) -> Result<Vec<Rc<S>>, CommandSyntaxException> {
        let Some(modifier) = context.redirect_modifier() else {
            return Ok(vec![source]);
        };
        let context = context.copy_for_shared(source);
        Self::apply_modifier(modifier, context, consumer, forked)
    }

    fn apply_modifier(
        modifier: RedirectModifier<S>,
        context: CommandContext<S>,
        consumer: &ResultConsumer<S>,
        forked: bool,
    ) -> Result<Vec<Rc<S>>, CommandSyntaxException> {
        match modifier(&context) {
            Ok(sources) => Ok(sources),
            Err(error) => {
                consumer(&context, false, 0);
                if forked { Ok(Vec::new()) } else { Err(error) }
            }
        }
    }

    pub fn run_executable(
        context: &CommandContext<S>,
        source: S,
        consumer: &ResultConsumer<S>,
        forked: bool,
    ) -> Result<i32, CommandSyntaxException> {
        let context = context.copy_for(source);
        Self::execute_context(context, consumer, forked)
    }

    fn run_executable_shared(
        context: &CommandContext<S>,
        source: Rc<S>,
        consumer: &ResultConsumer<S>,
        forked: bool,
    ) -> Result<i32, CommandSyntaxException> {
        let context = context.copy_for_shared(source);
        Self::execute_context(context, consumer, forked)
    }

    fn execute_context(
        context: CommandContext<S>,
        consumer: &ResultConsumer<S>,
        forked: bool,
    ) -> Result<i32, CommandSyntaxException> {
        let command = context
            .command()
            .expect("ContextChain executable must contain a command");
        match command(&context) {
            Ok(result) => {
                consumer(&context, true, result);
                Ok(if forked { SINGLE_SUCCESS } else { result })
            }
            Err(error) => {
                consumer(&context, false, 0);
                if forked { Ok(0) } else { Err(error) }
            }
        }
    }

    pub fn execute_all(
        &self,
        source: S,
        consumer: ResultConsumer<S>,
    ) -> Result<i32, CommandSyntaxException> {
        self.execute_all_shared(Rc::new(source), consumer)
    }

    pub(crate) fn execute_all_shared(
        &self,
        source: Rc<S>,
        consumer: ResultConsumer<S>,
    ) -> Result<i32, CommandSyntaxException> {
        if self.modifiers.is_empty() {
            return Self::run_executable_shared(&self.executable, source, &consumer, false);
        }

        let mut forked = false;
        let mut current_sources = vec![source];
        for modifier in &self.modifiers {
            forked |= modifier.is_forked();
            let mut next_sources = Vec::new();
            for source in current_sources {
                next_sources.extend(Self::run_modifier_shared(
                    modifier, source, &consumer, forked,
                )?);
            }
            if next_sources.is_empty() {
                return Ok(0);
            }
            current_sources = next_sources;
        }

        let mut result = 0_i32;
        for source in current_sources {
            result = result.wrapping_add(Self::run_executable_shared(
                &self.executable,
                source,
                &consumer,
                forked,
            )?);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use crate::{
        builder::LiteralArgumentBuilder,
        dispatcher::CommandDispatcher,
        tree::{Command, RedirectModifier},
    };

    use super::*;

    fn context_builder<S: 'static>(
        source: S,
        root: Node<S>,
        start: usize,
    ) -> CommandContextBuilder<S> {
        CommandContextBuilder::new(CommandDispatcher::new(), source, root, start)
    }

    #[test]
    fn ranges_use_utf16_offsets() {
        let input = "a😀bc";
        assert_eq!(StringRange::between(1, 3).get(input).unwrap(), "😀");
        assert!(StringRange::between(1, 2).get(input).is_err());
        assert_eq!(
            StringRange::between(1, 2)
                .get_utf16(&input.encode_utf16().collect::<Vec<_>>())
                .unwrap(),
            &[0xd83d]
        );
        assert_eq!(StringRange::between(2, 1).len(), -1);
        assert_eq!(StringRange::between(2, 5).java_hash_code(), 1_028);
    }

    #[test]
    fn context_preserves_unpaired_surrogates() {
        let builder = context_builder((), Node::root(), 0);
        let context = builder.build_utf16(vec![b'a' as u16, 0xd800, b'b' as u16]);
        assert_eq!(context.input_utf16(), &[0x61, 0xd800, 0x62]);
        assert_eq!(context.input(), "a�b");
    }

    #[test]
    fn parsed_arguments_compare_builtin_values() {
        assert_eq!(
            ParsedArgument::new(0, 3, 123_i32),
            ParsedArgument::new(0, 3, 123_i32)
        );
        assert_ne!(
            ParsedArgument::new(0, 3, 123_i32),
            ParsedArgument::new(3, 6, 123_i32)
        );
        assert_eq!(
            ParsedArgument::new(0, 1, f32::NAN),
            ParsedArgument::new(0, 1, f32::NAN)
        );
        assert_ne!(
            ParsedArgument::new(0, 1, 0.0_f32),
            ParsedArgument::new(0, 1, -0.0_f32)
        );
    }

    #[test]
    fn parsed_arguments_support_custom_value_and_explicit_identity_equality() {
        #[derive(PartialEq)]
        struct Value(i32);
        struct IdentityOnly;

        assert_eq!(
            ParsedArgument::new(0, 1, Value(1)),
            ParsedArgument::new(0, 1, Value(1))
        );
        assert_ne!(
            ParsedArgument::new(0, 1, Value(1)),
            ParsedArgument::new(0, 1, Value(2))
        );

        let identity = ParsedArgument::identity(0, 1, IdentityOnly);
        assert_eq!(identity, identity.clone());
        assert_ne!(identity, ParsedArgument::identity(0, 1, IdentityOnly));
    }

    #[test]
    fn parsed_argument_ranges_and_types_match_java_contract() {
        let argument = ParsedArgument::new(2, 5, 123_i32);
        assert_eq!(argument.range(), StringRange::between(2, 5));
        assert_eq!(*argument.result::<i32>().unwrap(), 123);
        assert!(argument.result::<String>().is_none());
        assert_eq!(argument.range().get("012345").unwrap(), "234");

        assert_ne!(argument, ParsedArgument::new(2, 5, 124_i32));
        assert_ne!(argument, ParsedArgument::new(2, 6, 123_i32));
        assert_ne!(argument, ParsedArgument::new(2, 5, "123".to_owned()));
    }

    #[test]
    fn builder_arguments_keep_insertion_order_and_replace_in_place() {
        let mut builder = context_builder((), Node::root(), 0);
        builder
            .with_argument("first", ParsedArgument::new(0, 1, 1_i32))
            .with_argument("second", ParsedArgument::new(2, 3, 2_i32))
            .with_argument("first", ParsedArgument::new(4, 5, 3_i32));

        let arguments = builder.arguments();
        let names: Vec<_> = arguments.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["first", "second"]);
        let context = builder.build("input");
        assert_eq!(*context.argument::<i32>("first").unwrap(), 3);
        assert!(matches!(
            context.argument::<i32>("missing"),
            Err(ContextError::MissingArgument(name)) if name == "missing"
        ));
        assert!(matches!(
            context.argument::<String>("second"),
            Err(ContextError::WrongArgumentType { name, .. }) if name == "second"
        ));
    }

    #[test]
    fn builder_clone_aliases_and_copy_shares_child_but_drops_modifier() {
        let redirect = LiteralArgumentBuilder::<()>::literal("target").build();
        let modifier: RedirectModifier<()> = Rc::new(|_| Ok(vec![Rc::new(())]));
        let redirected = LiteralArgumentBuilder::<()>::literal("redirect")
            .fork(redirect, modifier)
            .unwrap()
            .build();
        let mut child = context_builder((), Node::root(), 4);
        child.with_node(
            LiteralArgumentBuilder::literal("child").build(),
            StringRange::between(4, 9),
        );
        let mut original = context_builder((), Node::root(), 0);
        original
            .with_node(redirected, StringRange::between(0, 3))
            .with_child(child.clone());

        let cloned = original.clone();
        let copy = original.copy();
        assert!(cloned.ptr_eq(&original));
        assert!(!copy.ptr_eq(&original));
        assert!(cloned.child().unwrap().ptr_eq(&child));
        assert!(copy.child().unwrap().ptr_eq(&child));

        let original = original.build("");
        let copy = copy.build("");
        assert!(original.redirect_modifier().is_some());
        assert!(copy.redirect_modifier().is_none());
        assert!(original.is_forked());
        assert!(copy.is_forked());
    }

    #[test]
    fn attached_child_mutations_are_visible_to_build_and_suggestion_lookup() {
        let command: Command<()> = Rc::new(|_| Ok(1));
        let child_node = LiteralArgumentBuilder::literal("child").build();
        let mut parent = context_builder((), Node::root(), 0);
        let mut child = context_builder((), Node::root(), 4);
        parent.with_child(child.clone());

        child
            .with_node(child_node, StringRange::between(4, 9))
            .with_command(Some(command.clone()));

        let built_child = parent
            .build("root child")
            .child()
            .unwrap()
            .command()
            .unwrap();
        assert!(Rc::ptr_eq(&built_child, &command));
        let suggestion = parent.find_suggestion_context(5).unwrap();
        assert!(suggestion.context.ptr_eq(&child));
        assert!(parent.last_child().ptr_eq(&child));

        let isolated = context_builder((), Node::root(), 0);
        assert!(isolated.last_child().ptr_eq(&isolated));
    }

    #[test]
    fn builder_copy_preserves_dispatcher_reference() {
        let dispatcher = CommandDispatcher::<()>::new();
        let builder = CommandContextBuilder::new(dispatcher.clone(), (), dispatcher.root(), 0);
        assert!(builder.dispatcher().root().ptr_eq(&dispatcher.root()));
        assert!(
            builder
                .copy()
                .dispatcher()
                .root()
                .ptr_eq(&dispatcher.root())
        );
    }

    #[test]
    fn context_equality_uses_java_observable_members() {
        let command: Command<()> = Rc::new(|_| Ok(1));
        let root = Node::root();
        let node = LiteralArgumentBuilder::literal("foo")
            .executes(command.clone())
            .build();

        let make = |input: &str, start: usize| {
            let mut builder = context_builder((), root.clone(), start);
            builder
                .with_argument("value", ParsedArgument::new(0, 3, 123_i32))
                .with_node(node.clone(), StringRange::between(0, 3))
                .with_command(Some(command.clone()));
            builder.build(input)
        };
        let left = make("foo", 0);
        let right = make("different", 20);
        assert!(left.java_equals(&right));

        let mut different = context_builder((), root, 0);
        different
            .with_argument("value", ParsedArgument::new(0, 3, 124_i32))
            .with_node(node, StringRange::between(0, 3))
            .with_command(Some(command));
        assert!(!left.java_equals(&different.build("foo")));
    }

    #[test]
    fn finds_suggestion_parent_at_node_boundaries_and_after_input() {
        let root = Node::root();
        let first = LiteralArgumentBuilder::literal("foo").build();
        let second = LiteralArgumentBuilder::literal("bar").build();
        let mut builder = context_builder((), root.clone(), 0);
        builder
            .with_node(first.clone(), StringRange::between(0, 3))
            .with_node(second.clone(), StringRange::between(4, 7));

        let within_first = builder.find_suggestion_context(2).unwrap();
        assert!(within_first.parent.ptr_eq(&root));
        assert_eq!(within_first.start_pos, 0);
        let within_second = builder.find_suggestion_context(5).unwrap();
        assert!(within_second.parent.ptr_eq(&first));
        assert_eq!(within_second.start_pos, 4);
        let after = builder.find_suggestion_context(9).unwrap();
        assert!(after.parent.ptr_eq(&second));
        assert_eq!(after.start_pos, 8);
        assert_eq!(
            builder
                .find_suggestion_context(usize::MAX)
                .unwrap()
                .start_pos,
            8
        );
    }

    #[test]
    fn context_chain_executes_redirects_and_forks_like_java() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let command: Command<i32> = {
            let seen = seen.clone();
            Rc::new(move |context| {
                seen.borrow_mut().push(*context.source());
                Ok(*context.source() * 10)
            })
        };
        let target = LiteralArgumentBuilder::literal("target")
            .executes(command.clone())
            .build();
        let modifier: RedirectModifier<i32> = Rc::new(|context| {
            Ok(vec![
                Rc::new(context.source() + 1),
                Rc::new(context.source() + 2),
            ])
        });
        let redirect = LiteralArgumentBuilder::literal("fork")
            .fork(target.clone(), modifier)
            .unwrap()
            .build();

        let mut first = context_builder(1, Node::root(), 0);
        first.with_node(redirect, StringRange::between(0, 4));
        let mut executable = context_builder(1, target.clone(), 5);
        executable
            .with_node(target, StringRange::between(5, 11))
            .with_command(Some(command));
        first.with_child(executable);

        let context = first.build("fork target");
        let chain = ContextChain::try_flatten(context).unwrap();
        assert_eq!(chain.stage(), Stage::Modify);
        assert_eq!(chain.next_stage().unwrap().stage(), Stage::Execute);
        let consumed = Rc::new(RefCell::new(Vec::new()));
        let consumer: ResultConsumer<i32> = {
            let consumed = consumed.clone();
            Rc::new(move |context, success, result| {
                consumed
                    .borrow_mut()
                    .push((*context.source(), success, result));
            })
        };
        assert_eq!(chain.execute_all(1, consumer).unwrap(), 2);
        assert_eq!(&*seen.borrow(), &[2, 3]);
        assert_eq!(&*consumed.borrow(), &[(2, true, 20), (3, true, 30)]);
    }

    #[test]
    fn context_chain_requires_an_executable_and_sums_nonfork_results() {
        let empty = context_builder((), Node::root(), 0).build("");
        assert!(ContextChain::try_flatten(empty).is_none());

        let command: Command<i32> = Rc::new(|context| Ok(*context.source()));
        let node = LiteralArgumentBuilder::literal("run")
            .executes(command.clone())
            .build();
        let mut builder = context_builder(7, Node::root(), 0);
        builder
            .with_node(node, StringRange::between(0, 3))
            .with_command(Some(command));
        let chain = ContextChain::try_flatten(builder.build("run")).unwrap();
        assert_eq!(chain.stage(), Stage::Execute);
        assert_eq!(chain.execute_all(7, Rc::new(|_, _, _| {})).unwrap(), 7);
    }

    #[test]
    fn context_chain_constructor_rejects_missing_executable() {
        let empty = context_builder((), Node::root(), 0).build("");
        assert!(matches!(
            ContextChain::new(Vec::new(), empty),
            Err(ContextError::MissingExecutable)
        ));

        let command: Command<()> = Rc::new(|_| Ok(1));
        let mut executable = context_builder((), Node::root(), 0);
        executable.with_command(Some(command));
        assert!(ContextChain::new(Vec::new(), executable.build("")).is_ok());
    }

    #[test]
    fn official_command_context_get_argument_nonexistent() {
        let context = context_builder((), Node::root(), 0).build("");
        assert!(matches!(
            context.argument::<i32>("foo"),
            Err(ContextError::MissingArgument(name)) if name == "foo"
        ));
    }

    #[test]
    fn official_command_context_get_argument_wrong_type() {
        let mut builder = context_builder((), Node::root(), 0);
        builder.with_argument("foo", ParsedArgument::new(0, 1, 123_i32));
        assert!(matches!(
            builder.build("123").argument::<String>("foo"),
            Err(ContextError::WrongArgumentType { name, .. }) if name == "foo"
        ));
    }

    #[test]
    fn official_command_context_get_argument() {
        let mut builder = context_builder((), Node::root(), 0);
        builder.with_argument("foo", ParsedArgument::new(0, 1, 123_i32));
        let context = builder.build("123");
        assert_eq!(*context.argument::<i32>("foo").unwrap(), 123);
        assert_eq!(
            *context
                .argument_any("foo")
                .unwrap()
                .downcast::<i32>()
                .unwrap(),
            123
        );
    }

    #[test]
    fn official_command_context_source() {
        let source = Rc::new(42);
        let context = context_builder(source.clone(), Node::root(), 0).build("");
        assert!(Rc::ptr_eq(context.source(), &source));
    }

    #[test]
    fn official_command_context_root_node() {
        let root = Node::<()>::root();
        let context = context_builder((), root.clone(), 0).build("");
        assert!(context.root_node().ptr_eq(&root));
    }

    #[test]
    fn official_command_context_equals() {
        let root = Node::<i32>::root();
        let other_root = Node::<i32>::root();
        other_root
            .add_child(LiteralArgumentBuilder::literal("other-root").build())
            .unwrap();
        let command: Command<i32> = Rc::new(|_| Ok(1));
        let other_command: Command<i32> = Rc::new(|_| Ok(1));
        let node = LiteralArgumentBuilder::literal("one").build();
        let other_node = LiteralArgumentBuilder::literal("two").build();

        let make = |source: i32,
                    root: Node<i32>,
                    command: Option<Command<i32>>,
                    argument: Option<i32>,
                    nodes: &[Node<i32>]| {
            let mut builder = context_builder(source, root, 0);
            builder.with_command(command);
            if let Some(argument) = argument {
                builder.with_argument("foo", ParsedArgument::new(0, 1, argument));
            }
            for (index, node) in nodes.iter().enumerate() {
                builder.with_node(node.clone(), StringRange::between(index, index + 1));
            }
            builder.build("ignored")
        };

        let base = make(1, root.clone(), None, None, &[]);
        assert!(base.java_equals(&make(1, root.clone(), None, None, &[])));
        assert!(!base.java_equals(&make(1, other_root, None, None, &[])));
        assert!(!base.java_equals(&make(2, root.clone(), None, None, &[])));
        assert!(!base.java_equals(&make(1, root.clone(), Some(command.clone()), None, &[])));
        assert!(
            !make(1, root.clone(), Some(command), None, &[]).java_equals(&make(
                1,
                root.clone(),
                Some(other_command),
                None,
                &[]
            ))
        );
        assert!(!base.java_equals(&make(1, root.clone(), None, Some(123), &[])));
        assert!(
            !make(
                1,
                root.clone(),
                None,
                None,
                &[node.clone(), other_node.clone()]
            )
            .java_equals(&make(1, root, None, None, &[other_node, node]))
        );
    }

    #[test]
    fn official_context_chain_execute_all_for_single_command() {
        let dispatcher = CommandDispatcher::new();
        let command: Command<&'static str> = Rc::new(|_| Ok(4));
        dispatcher
            .register(LiteralArgumentBuilder::literal("foo").executes(command))
            .unwrap();
        let parsed = dispatcher.parse("foo", "compile_source");
        let chain = ContextChain::try_flatten(parsed.context().build("foo")).unwrap();
        let completions = Rc::new(RefCell::new(Vec::new()));
        let consumer: ResultConsumer<&'static str> = {
            let completions = completions.clone();
            Rc::new(move |context, success, result| {
                completions
                    .borrow_mut()
                    .push((*context.source(), success, result));
            })
        };
        assert_eq!(chain.execute_all("runtime_source", consumer).unwrap(), 4);
        assert_eq!(&*completions.borrow(), &[("runtime_source", true, 4)]);
    }

    #[test]
    fn official_context_chain_execute_all_for_redirected_command() {
        let dispatcher = CommandDispatcher::new();
        let command: Command<&'static str> = Rc::new(|context| {
            assert_eq!(*context.source(), "redirected_source");
            Ok(4)
        });
        dispatcher
            .register(LiteralArgumentBuilder::literal("foo").executes(command))
            .unwrap();
        dispatcher
            .register(
                LiteralArgumentBuilder::literal("bar")
                    .redirect_with_modifier(
                        dispatcher.root(),
                        Rc::new(|_| Ok(Rc::new("redirected_source"))),
                    )
                    .unwrap(),
            )
            .unwrap();
        let parsed = dispatcher.parse("bar foo", "compile_source");
        let chain = ContextChain::try_flatten(parsed.context().build("bar foo")).unwrap();
        let completions = Rc::new(RefCell::new(Vec::new()));
        let consumer: ResultConsumer<&'static str> = {
            let completions = completions.clone();
            Rc::new(move |context, success, result| {
                completions
                    .borrow_mut()
                    .push((*context.source(), success, result));
            })
        };
        assert_eq!(chain.execute_all("runtime_source", consumer).unwrap(), 4);
        assert_eq!(&*completions.borrow(), &[("redirected_source", true, 4)]);
    }

    #[test]
    fn official_context_chain_single_stage_execution() {
        let dispatcher = CommandDispatcher::new();
        dispatcher
            .register(LiteralArgumentBuilder::literal("foo").executes(Rc::new(|_| Ok(1))))
            .unwrap();
        let parsed = dispatcher.parse("foo", ());
        let context = parsed.context().build("foo");
        let chain = ContextChain::try_flatten(context.clone()).unwrap();
        assert_eq!(chain.stage(), Stage::Execute);
        assert!(chain.top_context().java_equals(&context));
        assert!(chain.next_stage().is_none());
    }

    #[test]
    fn official_context_chain_multi_stage_execution() {
        let dispatcher = CommandDispatcher::new();
        dispatcher
            .register(LiteralArgumentBuilder::literal("foo").executes(Rc::new(|_| Ok(1))))
            .unwrap();
        dispatcher
            .register(
                LiteralArgumentBuilder::literal("bar")
                    .redirect(dispatcher.root())
                    .unwrap(),
            )
            .unwrap();
        let parsed = dispatcher.parse("bar bar foo", ());
        let context = parsed.context().build("bar bar foo");
        let stage_zero = ContextChain::try_flatten(context.clone()).unwrap();
        assert_eq!(stage_zero.stage(), Stage::Modify);
        assert!(stage_zero.top_context().java_equals(&context));
        let stage_one = stage_zero.next_stage().unwrap();
        assert_eq!(stage_one.stage(), Stage::Modify);
        assert!(
            stage_one
                .top_context()
                .java_equals(context.child().unwrap())
        );
        let stage_two = stage_one.next_stage().unwrap();
        assert_eq!(stage_two.stage(), Stage::Execute);
        assert!(
            stage_two
                .top_context()
                .java_equals(context.child().unwrap().child().unwrap())
        );
        assert!(stage_two.next_stage().is_none());
    }

    #[test]
    fn official_context_chain_missing_execute() {
        let dispatcher = CommandDispatcher::new();
        dispatcher
            .register(
                LiteralArgumentBuilder::literal("bar")
                    .redirect(dispatcher.root())
                    .unwrap(),
            )
            .unwrap();
        let parsed = dispatcher.parse("bar bar", ());
        assert!(ContextChain::try_flatten(parsed.context().build("bar bar")).is_none());
    }

    #[test]
    fn official_parsed_argument_equals() {
        let first = ParsedArgument::new(0, 3, "bar".to_owned());
        assert_eq!(first, ParsedArgument::new(0, 3, "bar".to_owned()));
        assert_ne!(first, ParsedArgument::new(3, 6, "baz".to_owned()));
        assert_ne!(
            ParsedArgument::new(3, 6, "baz".to_owned()),
            ParsedArgument::new(6, 9, "baz".to_owned())
        );
    }

    #[test]
    fn official_parsed_argument_get_raw() {
        let argument = ParsedArgument::new(2, 5, String::new());
        assert_eq!(argument.range().get("0123456789").unwrap(), "234");
    }
}
