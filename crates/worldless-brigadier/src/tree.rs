use std::{cell::RefCell, cmp::Ordering, error::Error, fmt, rc::Rc};

use crate::{
    arguments::ArgumentTypeRef,
    builder::{ArgumentBuilder, LiteralArgumentBuilder, RequiredArgumentBuilder},
    context::{CommandContext, CommandContextBuilder, ParsedArgument, StringRange},
    exceptions::{BUILT_IN_EXCEPTIONS, BuiltInExceptionProvider, CommandSyntaxException},
    java_case::java_root_lowercase,
    java_hash_set::{java_hash_set_order, java_utf16_hash_code},
    reader::StringReader,
    suggestion::{SuggestionProvider, Suggestions, SuggestionsBuilder, SuggestionsFuture},
};

pub type Command<S> = Rc<dyn Fn(&CommandContext<S>) -> Result<i32, CommandSyntaxException>>;
pub const SINGLE_SUCCESS: i32 = 1;
pub type Requirement<S> = Rc<dyn Fn(&S) -> bool>;
pub type RedirectModifier<S> =
    Rc<dyn Fn(&CommandContext<S>) -> Result<Vec<Rc<S>>, CommandSyntaxException>>;
pub type AmbiguityConsumer<S> = Rc<dyn Fn(&Node<S>, &Node<S>, &Node<S>, &[String])>;

pub struct Node<S: 'static>(Rc<RefCell<NodeData<S>>>);

impl<S: 'static> Clone for Node<S> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

struct NodeData<S: 'static> {
    kind: NodeKind<S>,
    children: Vec<Node<S>>,
    command: Option<Command<S>>,
    requirement: Requirement<S>,
    redirect: Option<Node<S>>,
    modifier: Option<RedirectModifier<S>>,
    forks: bool,
}

enum NodeKind<S: 'static> {
    Root,
    Literal {
        literal: String,
        lower_case_utf16: Vec<u16>,
    },
    Argument {
        name: String,
        argument_type: ArgumentTypeRef<S>,
        custom_suggestions: Option<SuggestionProvider<S>>,
    },
}

impl<S: 'static> Node<S> {
    pub fn root() -> Self {
        Self::new(
            NodeKind::Root,
            None,
            Rc::new(|_| true),
            None,
            Some(Rc::new(|context| Ok(vec![context.shared_source()]))),
            false,
        )
    }

    pub fn literal(
        literal: String,
        command: Option<Command<S>>,
        requirement: Requirement<S>,
        redirect: Option<Node<S>>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Self {
        let lower_case_utf16 = java_root_lowercase(&literal.encode_utf16().collect::<Vec<_>>());
        Self::new(
            NodeKind::Literal {
                literal,
                lower_case_utf16,
            },
            command,
            requirement,
            redirect,
            modifier,
            forks,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn argument(
        name: String,
        argument_type: ArgumentTypeRef<S>,
        command: Option<Command<S>>,
        requirement: Requirement<S>,
        redirect: Option<Node<S>>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
        custom_suggestions: Option<SuggestionProvider<S>>,
    ) -> Self {
        Self::new(
            NodeKind::Argument {
                name,
                argument_type,
                custom_suggestions,
            },
            command,
            requirement,
            redirect,
            modifier,
            forks,
        )
    }

    fn new(
        kind: NodeKind<S>,
        command: Option<Command<S>>,
        requirement: Requirement<S>,
        redirect: Option<Node<S>>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Self {
        Self(Rc::new(RefCell::new(NodeData {
            kind,
            children: Vec::new(),
            command,
            requirement,
            redirect,
            modifier,
            forks,
        })))
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn is_root(&self) -> bool {
        matches!(self.0.borrow().kind, NodeKind::Root)
    }

    pub fn is_literal(&self) -> bool {
        matches!(self.0.borrow().kind, NodeKind::Literal { .. })
    }

    pub fn is_argument(&self) -> bool {
        matches!(self.0.borrow().kind, NodeKind::Argument { .. })
    }

    pub fn name(&self) -> String {
        match &self.0.borrow().kind {
            NodeKind::Root => String::new(),
            NodeKind::Literal { literal, .. } => literal.clone(),
            NodeKind::Argument { name, .. } => name.clone(),
        }
    }

    pub fn literal_value(&self) -> Option<String> {
        match &self.0.borrow().kind {
            NodeKind::Literal { literal, .. } => Some(literal.clone()),
            _ => None,
        }
    }

    pub fn argument_type(&self) -> Option<ArgumentTypeRef<S>> {
        match &self.0.borrow().kind {
            NodeKind::Argument { argument_type, .. } => Some(argument_type.clone()),
            _ => None,
        }
    }

    pub fn custom_suggestions(&self) -> Option<SuggestionProvider<S>> {
        match &self.0.borrow().kind {
            NodeKind::Argument {
                custom_suggestions, ..
            } => custom_suggestions.clone(),
            _ => None,
        }
    }

    pub fn usage_text(&self) -> String {
        match &self.0.borrow().kind {
            NodeKind::Root => String::new(),
            NodeKind::Literal { literal, .. } => literal.clone(),
            NodeKind::Argument { name, .. } => format!("<{name}>"),
        }
    }

    pub fn command(&self) -> Option<Command<S>> {
        self.0.borrow().command.clone()
    }

    pub fn requirement(&self) -> Requirement<S> {
        self.0.borrow().requirement.clone()
    }

    pub fn redirect(&self) -> Option<Node<S>> {
        self.0.borrow().redirect.clone()
    }

    pub fn redirect_modifier(&self) -> Option<RedirectModifier<S>> {
        self.0.borrow().modifier.clone()
    }

    pub fn is_fork(&self) -> bool {
        self.0.borrow().forks
    }

    pub fn can_use(&self, source: &S) -> bool {
        let requirement = self.requirement();
        requirement(source)
    }

    pub fn children(&self) -> Vec<Node<S>> {
        self.0.borrow().children.clone()
    }

    pub fn child(&self, name: &str) -> Option<Node<S>> {
        self.0
            .borrow()
            .children
            .iter()
            .find(|child| child.name() == name)
            .cloned()
    }

    pub fn add_child(&self, node: Node<S>) -> Result<(), TreeError> {
        if node.is_root() {
            return Err(TreeError::RootChild);
        }

        if let Some(existing) = self.child(&node.name()) {
            if let Some(command) = node.command() {
                existing.0.borrow_mut().command = Some(command);
            }
            for grandchild in node.children() {
                existing.add_child(grandchild)?;
            }
        } else {
            self.0.borrow_mut().children.push(node);
        }
        Ok(())
    }

    pub fn parse(
        &self,
        reader: &mut StringReader,
        context: &mut CommandContextBuilder<S>,
    ) -> Result<(), CommandSyntaxException> {
        enum ParseKind<S: 'static> {
            Root,
            Literal(String),
            Argument(String, ArgumentTypeRef<S>),
        }

        let kind = match &self.0.borrow().kind {
            NodeKind::Root => ParseKind::Root,
            NodeKind::Literal { literal, .. } => ParseKind::Literal(literal.clone()),
            NodeKind::Argument {
                name,
                argument_type,
                ..
            } => ParseKind::Argument(name.clone(), argument_type.clone()),
        };

        match kind {
            ParseKind::Root => Ok(()),
            ParseKind::Literal(literal) => {
                let start = reader.cursor();
                let expected: Vec<_> = literal.encode_utf16().collect();
                let matches = reader.can_read_n(expected.len())
                    && reader.utf16()[start..start + expected.len()] == expected
                    && (start + expected.len() == reader.total_length()
                        || reader.utf16()[start + expected.len()] == b' ' as u16);
                if !matches {
                    reader.set_cursor(start);
                    return Err(BUILT_IN_EXCEPTIONS
                        .literal_incorrect()
                        .create_with_context(reader, literal));
                }

                let end = start + expected.len();
                reader.set_cursor(end);
                context.with_node(self.clone(), StringRange::between(start, end));
                Ok(())
            }
            ParseKind::Argument(name, argument_type) => {
                let start = reader.cursor();
                let value_equality = argument_type.value_comparator();
                let source = context.source();
                let result = argument_type.parse_with_source(reader, source.as_ref())?;
                let parsed = ParsedArgument::from_rc_with_equality(
                    start,
                    reader.cursor(),
                    result,
                    argument_type.value_type_name(),
                    value_equality,
                );
                let range = parsed.range();
                context.with_argument(name, parsed);
                context.with_node(self.clone(), range);
                Ok(())
            }
        }
    }

    pub fn list_suggestions(
        &self,
        context: &CommandContext<S>,
        mut builder: SuggestionsBuilder,
    ) -> Result<SuggestionsFuture, CommandSyntaxException> {
        enum SuggestKind<S: 'static> {
            Empty,
            Literal(Vec<u16>, String),
            Argument(ArgumentTypeRef<S>, Option<SuggestionProvider<S>>),
        }
        let kind = match &self.0.borrow().kind {
            NodeKind::Root => SuggestKind::Empty,
            NodeKind::Literal {
                literal,
                lower_case_utf16,
            } => SuggestKind::Literal(lower_case_utf16.clone(), literal.clone()),
            NodeKind::Argument {
                argument_type,
                custom_suggestions,
                ..
            } => SuggestKind::Argument(argument_type.clone(), custom_suggestions.clone()),
        };

        match kind {
            SuggestKind::Empty => Ok(Suggestions::empty_future()),
            SuggestKind::Literal(lower, literal) => {
                if lower.starts_with(builder.remaining_lower_case_utf16()) {
                    builder.suggest(literal);
                    Ok(builder.build_future())
                } else {
                    Ok(Suggestions::empty_future())
                }
            }
            SuggestKind::Argument(_, Some(provider)) => provider(context, builder),
            SuggestKind::Argument(argument_type, None) => {
                Ok(argument_type.list_suggestions(context, builder))
            }
        }
    }

    pub fn relevant_nodes(&self, input: &mut StringReader) -> Vec<Node<S>> {
        let children = self.children();
        if children.iter().any(Node::is_literal) {
            let cursor = input.cursor();
            while input.can_read() && input.peek() != b' ' as u16 {
                input.skip();
            }
            let text = input.substring_utf16(cursor, input.cursor());
            input.set_cursor(cursor);
            if let Some(literal) = children.iter().find(|node| {
                node.literal_value()
                    .is_some_and(|value| value.encode_utf16().eq(text.iter().copied()))
            }) {
                return vec![literal.clone()];
            }
        }
        children.into_iter().filter(Node::is_argument).collect()
    }

    pub fn compare_to(&self, other: &Self) -> Ordering {
        match (self.is_literal(), other.is_literal()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => self.name().encode_utf16().cmp(other.name().encode_utf16()),
        }
    }

    pub fn find_ambiguities(&self, consumer: &AmbiguityConsumer<S>) {
        let children = self.children();
        for child in &children {
            for sibling in &children {
                if child.ptr_eq(sibling) {
                    continue;
                }
                let matches = java_hash_set_order(
                    child
                        .examples()
                        .into_iter()
                        .filter(|example| sibling.is_valid_input(example)),
                    |example| java_utf16_hash_code(example.encode_utf16()),
                    |left, right| left == right,
                    |left, right| Some(left.encode_utf16().cmp(right.encode_utf16())),
                    |left, right| left.encode_utf16().cmp(right.encode_utf16()),
                );
                if !matches.is_empty() {
                    consumer(self, child, sibling, &matches);
                }
            }
            child.find_ambiguities(consumer);
        }
    }

    pub fn examples(&self) -> Vec<String> {
        enum ExampleKind<S: 'static> {
            Root,
            Literal(String),
            Argument(ArgumentTypeRef<S>),
        }
        let kind = match &self.0.borrow().kind {
            NodeKind::Root => ExampleKind::Root,
            NodeKind::Literal { literal, .. } => ExampleKind::Literal(literal.clone()),
            NodeKind::Argument { argument_type, .. } => {
                ExampleKind::Argument(argument_type.clone())
            }
        };
        match kind {
            ExampleKind::Root => Vec::new(),
            ExampleKind::Literal(literal) => vec![literal],
            ExampleKind::Argument(argument_type) => argument_type.examples(),
        }
    }

    pub fn is_valid_input(&self, input: &str) -> bool {
        enum ValidationKind<S: 'static> {
            Root,
            Literal(String),
            Argument(ArgumentTypeRef<S>),
        }
        let kind = match &self.0.borrow().kind {
            NodeKind::Root => ValidationKind::Root,
            NodeKind::Literal { literal, .. } => ValidationKind::Literal(literal.clone()),
            NodeKind::Argument { argument_type, .. } => {
                ValidationKind::Argument(argument_type.clone())
            }
        };
        let mut reader = StringReader::new(input);
        match kind {
            ValidationKind::Root => false,
            ValidationKind::Literal(literal) => {
                let expected: Vec<_> = literal.encode_utf16().collect();
                reader.utf16().starts_with(&expected)
                    && (reader.total_length() == expected.len()
                        || reader.utf16()[expected.len()] == b' ' as u16)
            }
            ValidationKind::Argument(argument_type) => argument_type
                .parse(&mut reader)
                .is_ok_and(|_| !reader.can_read() || reader.peek() == b' ' as u16),
        }
    }

    pub fn create_builder(&self) -> Result<ArgumentBuilder<S>, TreeError> {
        let data = self.0.borrow();
        let mut builder = match &data.kind {
            NodeKind::Root => return Err(TreeError::RootBuilder),
            NodeKind::Literal { literal, .. } => {
                ArgumentBuilder::Literal(LiteralArgumentBuilder::literal(literal.clone()))
            }
            NodeKind::Argument {
                name,
                argument_type,
                custom_suggestions,
            } => ArgumentBuilder::Required(
                RequiredArgumentBuilder::from_ref(name.clone(), argument_type.clone())
                    .suggests_option(custom_suggestions.clone()),
            ),
        };
        builder = builder
            .requires(data.requirement.clone())
            .forward_option(data.redirect.clone(), data.modifier.clone(), data.forks)
            .expect("a fresh builder has no children");
        if let Some(command) = &data.command {
            builder = builder.executes(command.clone());
        }
        Ok(builder)
    }

    pub fn java_equals(&self, other: &Self) -> bool {
        if self.ptr_eq(other) {
            return true;
        }

        let (kind_equal, command_equal, left_children) = {
            let left = self.0.borrow();
            let right = other.0.borrow();
            let kind_equal = match (&left.kind, &right.kind) {
                (NodeKind::Root, NodeKind::Root) => true,
                (
                    NodeKind::Literal { literal: left, .. },
                    NodeKind::Literal { literal: right, .. },
                ) => left == right,
                (
                    NodeKind::Argument {
                        name: left_name,
                        argument_type: left_type,
                        ..
                    },
                    NodeKind::Argument {
                        name: right_name,
                        argument_type: right_type,
                        ..
                    },
                ) => left_name == right_name && left_type == right_type,
                _ => false,
            };
            let command_equal = match (&left.command, &right.command) {
                (Some(left), Some(right)) => Rc::ptr_eq(left, right),
                (None, None) => true,
                _ => false,
            };
            (kind_equal, command_equal, left.children.clone())
        };
        if !kind_equal || !command_equal {
            return false;
        }
        let right_children = other.children();
        left_children.len() == right_children.len()
            && left_children.iter().all(|left| {
                right_children
                    .iter()
                    .find(|right| right.name() == left.name())
                    .is_some_and(|right| left.java_equals(right))
            })
    }
}

impl<S: 'static> PartialEq for Node<S> {
    fn eq(&self, other: &Self) -> bool {
        self.java_equals(other)
    }
}

impl<S: 'static> fmt::Display for Node<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0.borrow().kind {
            NodeKind::Root => formatter.write_str("<root>"),
            NodeKind::Literal { literal, .. } => write!(formatter, "<literal {literal}>"),
            NodeKind::Argument {
                name,
                argument_type,
                ..
            } => write!(formatter, "<argument {name}:{}>", argument_type.display()),
        }
    }
}

impl<S: 'static> fmt::Debug for Node<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeError {
    RootChild,
    RootBuilder,
}

impl fmt::Display for TreeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootChild => formatter.write_str("cannot add a root node as a child"),
            Self::RootBuilder => formatter.write_str("cannot convert a root node into a builder"),
        }
    }
}

impl Error for TreeError {}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    use crate::{
        arguments::{ArgumentType, IntegerArgumentType},
        builder::{ArgumentBuilder, LiteralArgumentBuilder, RequiredArgumentBuilder},
        dispatcher::CommandDispatcher,
    };

    use super::*;

    fn ready<T>(mut future: Pin<Box<dyn Future<Output = T>>>) -> T {
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("test future unexpectedly remained pending"),
        }
    }

    fn literal<S: 'static>(name: &str) -> Node<S> {
        LiteralArgumentBuilder::literal(name).build()
    }

    fn context_builder<S: 'static>(
        source: S,
        root: Node<S>,
        start: usize,
    ) -> CommandContextBuilder<S> {
        CommandContextBuilder::new(CommandDispatcher::new(), source, root, start)
    }

    #[test]
    fn root_has_brigadier_defaults() {
        let root = Node::<()>::root();
        assert!(root.is_root());
        assert_eq!(root.name(), "");
        assert_eq!(root.usage_text(), "");
        assert_eq!(root.to_string(), "<root>");
        assert!(root.command().is_none());
        assert!(root.redirect().is_none());
        assert!(!root.is_fork());
        assert!(root.can_use(&()));
        assert!(root.examples().is_empty());
        assert!(matches!(root.create_builder(), Err(TreeError::RootBuilder)));

        let mut reader = StringReader::new("foo");
        let mut context = context_builder((), root.clone(), 0);
        root.parse(&mut reader, &mut context).unwrap();
        assert_eq!(reader.cursor(), 0);
        assert!(context.nodes().is_empty());
        let context = context.build("foo");
        let shared_source = context.shared_source();
        let redirected_sources = root.redirect_modifier().unwrap()(&context).unwrap();
        assert_eq!(redirected_sources.len(), 1);
        assert!(Rc::ptr_eq(&redirected_sources[0], &shared_source));
        assert!(root.add_child(Node::root()).is_err());
    }

    #[test]
    fn child_insertion_merges_same_name_without_replacing_identity() {
        let parent = Node::<()>::root();
        let first = literal("child");
        let first_identity = first.clone();
        first.add_child(literal("first-grandchild")).unwrap();
        parent.add_child(first).unwrap();

        let command: Command<()> = Rc::new(|_| Ok(42));
        let replacement = LiteralArgumentBuilder::literal("child")
            .executes(command.clone())
            .then_node(literal("second-grandchild"))
            .unwrap()
            .build();
        parent.add_child(replacement).unwrap();

        let child = parent.child("child").unwrap();
        assert!(child.ptr_eq(&first_identity));
        assert!(child.child("first-grandchild").is_some());
        assert!(child.child("second-grandchild").is_some());
        assert!(Rc::ptr_eq(&child.command().unwrap(), &command));
        assert_eq!(parent.children().len(), 1);
    }

    #[test]
    fn merging_a_commandless_node_preserves_existing_command() {
        let parent = Node::<()>::root();
        let command: Command<()> = Rc::new(|_| Ok(1));
        parent
            .add_child(
                LiteralArgumentBuilder::literal("child")
                    .executes(command.clone())
                    .build(),
            )
            .unwrap();
        parent.add_child(literal("child")).unwrap();
        assert!(Rc::ptr_eq(
            &parent.child("child").unwrap().command().unwrap(),
            &command
        ));
    }

    #[test]
    fn relevant_nodes_prefer_an_exact_literal_then_keep_argument_order() {
        let parent = Node::<()>::root();
        let first =
            RequiredArgumentBuilder::argument("first", IntegerArgumentType::integer()).build();
        let exact = literal("123");
        let second =
            RequiredArgumentBuilder::argument("second", IntegerArgumentType::integer()).build();
        parent.add_child(first.clone()).unwrap();
        parent.add_child(exact.clone()).unwrap();
        parent.add_child(second.clone()).unwrap();

        let mut exact_reader = StringReader::new("123 tail");
        let relevant = parent.relevant_nodes(&mut exact_reader);
        assert_eq!(exact_reader.cursor(), 0);
        assert_eq!(relevant.len(), 1);
        assert!(relevant[0].ptr_eq(&exact));

        let mut argument_reader = StringReader::new("456");
        let relevant = parent.relevant_nodes(&mut argument_reader);
        assert_eq!(relevant.len(), 2);
        assert!(relevant[0].ptr_eq(&first));
        assert!(relevant[1].ptr_eq(&second));
    }

    #[test]
    fn input_validation_releases_the_node_before_custom_parsing() {
        struct MutatingArgument(Rc<RefCell<Option<Node<()>>>>);

        impl ArgumentType<()> for MutatingArgument {
            type Value = ();

            fn parse(
                &self,
                reader: &mut StringReader,
            ) -> Result<Self::Value, CommandSyntaxException> {
                reader.skip();
                self.0
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .add_child(literal("side-effect"))
                    .unwrap();
                Ok(())
            }
        }

        let node_handle = Rc::new(RefCell::new(None));
        let node =
            RequiredArgumentBuilder::argument("value", MutatingArgument(node_handle.clone()))
                .build();
        *node_handle.borrow_mut() = Some(node.clone());

        assert!(node.is_valid_input("x"));
        assert!(node.child("side-effect").is_some());
    }

    #[test]
    fn examples_release_the_node_before_calling_custom_argument_type() {
        struct MutatingExamples(Rc<RefCell<Option<Node<()>>>>);

        impl ArgumentType<()> for MutatingExamples {
            type Value = ();

            fn parse(
                &self,
                _reader: &mut StringReader,
            ) -> Result<Self::Value, CommandSyntaxException> {
                Ok(())
            }

            fn examples(&self) -> Vec<String> {
                self.0
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .add_child(literal("side-effect"))
                    .unwrap();
                vec!["value".to_owned()]
            }
        }

        let node_handle = Rc::new(RefCell::new(None));
        let node =
            RequiredArgumentBuilder::argument("value", MutatingExamples(node_handle.clone()))
                .build();
        *node_handle.borrow_mut() = Some(node.clone());

        assert_eq!(node.examples(), ["value"]);
        assert!(node.child("side-effect").is_some());
    }

    #[test]
    fn compare_and_ambiguity_detection_match_command_node_contract() {
        let literal_b = literal::<()>("b");
        let literal_a = literal::<()>("a");
        let argument =
            RequiredArgumentBuilder::argument("number", IntegerArgumentType::integer()).build();
        assert_eq!(literal_a.compare_to(&literal_b), Ordering::Less);
        assert_eq!(literal_b.compare_to(&argument), Ordering::Less);
        assert_eq!(argument.compare_to(&literal_b), Ordering::Greater);
        assert_eq!(
            literal::<()>("\u{10000}").compare_to(&literal("\u{e000}")),
            Ordering::Less
        );

        let parent = Node::root();
        parent.add_child(literal::<()>("123")).unwrap();
        parent.add_child(argument).unwrap();
        let found = Rc::new(RefCell::new(Vec::new()));
        let consumer: AmbiguityConsumer<()> = {
            let found = found.clone();
            Rc::new(move |parent, child, sibling, examples| {
                found.borrow_mut().push((
                    parent.name(),
                    child.name(),
                    sibling.name(),
                    examples.to_vec(),
                ));
            })
        };
        parent.find_ambiguities(&consumer);
        assert_eq!(
            &*found.borrow(),
            &[
                (
                    String::new(),
                    "123".to_owned(),
                    "number".to_owned(),
                    vec!["123".to_owned()]
                ),
                (
                    String::new(),
                    "number".to_owned(),
                    "123".to_owned(),
                    vec!["123".to_owned()]
                )
            ]
        );
    }

    #[test]
    fn ambiguity_examples_follow_java_hash_set_iteration_order() {
        let parent = Node::root();
        parent
            .add_child(
                RequiredArgumentBuilder::argument("first", IntegerArgumentType::integer()).build(),
            )
            .unwrap();
        parent
            .add_child(
                RequiredArgumentBuilder::argument("second", IntegerArgumentType::integer()).build(),
            )
            .unwrap();

        let found = Rc::new(RefCell::new(Vec::new()));
        let consumer: AmbiguityConsumer<()> = {
            let found = found.clone();
            Rc::new(move |_, child, sibling, examples| {
                found
                    .borrow_mut()
                    .push((child.name(), sibling.name(), examples.to_vec()));
            })
        };
        parent.find_ambiguities(&consumer);

        assert_eq!(
            &*found.borrow(),
            &[
                (
                    "first".to_owned(),
                    "second".to_owned(),
                    vec!["0".to_owned(), "-123".to_owned(), "123".to_owned()],
                ),
                (
                    "second".to_owned(),
                    "first".to_owned(),
                    vec!["0".to_owned(), "-123".to_owned(), "123".to_owned()],
                ),
            ]
        );
    }

    #[test]
    fn literal_parse_requires_a_complete_token_and_restores_cursor_on_error() {
        let node = literal::<()>("foo");
        for (input, cursor) in [("foo", 3), ("foo bar", 3)] {
            let mut reader = StringReader::new(input);
            let mut context = context_builder((), Node::root(), 0);
            node.parse(&mut reader, &mut context).unwrap();
            assert_eq!(reader.cursor(), cursor);
            assert_eq!(context.nodes()[0].range(), StringRange::between(0, 3));
        }

        for input in ["food", "bar", "fo"] {
            let mut reader = StringReader::new(input);
            let mut context = context_builder((), Node::root(), 0);
            let error = node.parse(&mut reader, &mut context).unwrap_err();
            assert_eq!(reader.cursor(), 0);
            assert_eq!(error.cursor(), 0);
            assert!(error.is_type(&BUILT_IN_EXCEPTIONS.literal_incorrect()));
        }
    }

    #[test]
    fn literal_parse_and_ranges_use_utf16_units() {
        let node = literal::<()>("😀");
        let mut reader = StringReader::new("😀 tail");
        let mut context = context_builder((), Node::root(), 0);
        node.parse(&mut reader, &mut context).unwrap();
        assert_eq!(reader.cursor(), 2);
        assert_eq!(context.nodes()[0].range(), StringRange::between(0, 2));
    }

    #[test]
    fn literal_suggestions_match_lowercase_prefix() {
        let node = literal::<()>("Foobar");
        let context = context_builder((), Node::root(), 0).build("foo");
        let suggestions = ready(
            node.list_suggestions(&context, SuggestionsBuilder::new("foo", 0))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(suggestions.list().len(), 1);
        assert_eq!(suggestions.list()[0].text(), "Foobar");

        let suggestions = ready(
            node.list_suggestions(&context, SuggestionsBuilder::new("bar", 0))
                .unwrap(),
        )
        .unwrap();
        assert!(suggestions.is_empty());
    }

    #[derive(Clone, Copy)]
    struct SourceArgument;

    impl ArgumentType<i32> for SourceArgument {
        type Value = i32;

        fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
            reader.read_int()
        }

        fn parse_with_source(
            &self,
            reader: &mut StringReader,
            source: &i32,
        ) -> Result<Self::Value, CommandSyntaxException> {
            reader.read_int().map(|value| value + source)
        }

        fn examples(&self) -> Vec<String> {
            vec!["123".to_owned()]
        }
    }

    #[test]
    fn argument_parse_uses_source_and_records_typed_result() {
        let node = RequiredArgumentBuilder::argument("value", SourceArgument).build();
        let mut reader = StringReader::new("123 tail");
        let mut builder = context_builder(10, Node::root(), 0);
        node.parse(&mut reader, &mut builder).unwrap();
        assert_eq!(reader.cursor(), 3);
        assert_eq!(builder.nodes()[0].range(), StringRange::between(0, 3));
        let context = builder.build("123 tail");
        assert_eq!(*context.argument::<i32>("value").unwrap(), 133);
        assert_eq!(node.examples(), ["123"]);
        assert_eq!(node.usage_text(), "<value>");
    }

    #[derive(PartialEq)]
    struct CustomValue(i32);

    #[derive(Clone, Copy)]
    struct ValueEqualArgument;

    impl ArgumentType<()> for ValueEqualArgument {
        type Value = CustomValue;

        fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
            reader.read_int().map(CustomValue)
        }

        fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
            left == right
        }
    }

    #[test]
    fn parsed_custom_argument_uses_argument_type_value_equality() {
        let node = RequiredArgumentBuilder::argument("value", ValueEqualArgument).build();
        let parse = || {
            let mut reader = StringReader::new("123");
            let mut builder = context_builder((), Node::root(), 0);
            node.parse(&mut reader, &mut builder).unwrap();
            builder.arguments()[0].1.clone()
        };
        assert_eq!(parse(), parse());
    }

    #[test]
    fn node_java_equality_has_brigadier_field_boundaries() {
        let command: Command<()> = Rc::new(|_| Ok(1));
        let left = LiteralArgumentBuilder::literal("foo")
            .requires(Rc::new(|_| false))
            .executes(command.clone())
            .build();
        let right = LiteralArgumentBuilder::literal("foo")
            .requires(Rc::new(|_| true))
            .executes(command)
            .build();
        assert!(left.java_equals(&right));
        assert!(!left.java_equals(&literal("bar")));

        let int_a: Node<()> =
            RequiredArgumentBuilder::argument("value", IntegerArgumentType::integer_range(0, 10))
                .build();
        let int_b: Node<()> =
            RequiredArgumentBuilder::argument("value", IntegerArgumentType::integer_range(0, 10))
                .build();
        let int_other: Node<()> =
            RequiredArgumentBuilder::argument("value", IntegerArgumentType::integer_range(1, 10))
                .build();
        assert!(int_a.java_equals(&int_b));
        assert!(!int_a.java_equals(&int_other));
    }

    #[test]
    fn create_builder_preserves_node_configuration_but_not_children() {
        let target = literal::<i32>("target");
        let command: Command<i32> = Rc::new(|_| Ok(1));
        let modifier: RedirectModifier<i32> = Rc::new(|context| Ok(vec![context.shared_source()]));
        let original = LiteralArgumentBuilder::literal("source")
            .requires(Rc::new(|source| *source > 0))
            .executes(command)
            .fork(target.clone(), modifier)
            .unwrap()
            .build();
        let rebuilt = original.create_builder().unwrap().build();
        assert_eq!(rebuilt.literal_value().as_deref(), Some("source"));
        assert!(rebuilt.can_use(&1));
        assert!(!rebuilt.can_use(&0));
        assert!(rebuilt.redirect().unwrap().ptr_eq(&target));
        assert!(rebuilt.is_fork());
        assert!(rebuilt.command().is_some());
        assert!(rebuilt.children().is_empty());
    }

    #[derive(Clone, Copy)]
    enum OfficialNodeFlavor {
        Root,
        Literal,
        Argument,
    }

    fn official_node(flavor: OfficialNodeFlavor) -> Node<()> {
        match flavor {
            OfficialNodeFlavor::Root => Node::root(),
            OfficialNodeFlavor::Literal => literal("foo"),
            OfficialNodeFlavor::Argument => {
                RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer()).build()
            }
        }
    }

    fn official_abstract_add_child(flavor: OfficialNodeFlavor) {
        let node = official_node(flavor);
        node.add_child(literal("child1")).unwrap();
        node.add_child(literal("child2")).unwrap();
        node.add_child(literal("child1")).unwrap();
        assert_eq!(node.children().len(), 2);
    }

    fn official_abstract_add_child_merges_grandchildren(flavor: OfficialNodeFlavor) {
        let node = official_node(flavor);
        node.add_child(
            LiteralArgumentBuilder::literal("child")
                .then(LiteralArgumentBuilder::literal("grandchild1"))
                .unwrap()
                .build(),
        )
        .unwrap();
        node.add_child(
            LiteralArgumentBuilder::literal("child")
                .then(LiteralArgumentBuilder::literal("grandchild2"))
                .unwrap()
                .build(),
        )
        .unwrap();
        assert_eq!(node.children().len(), 1);
        assert_eq!(node.children()[0].children().len(), 2);
    }

    fn official_abstract_add_child_preserves_command(flavor: OfficialNodeFlavor) {
        let node = official_node(flavor);
        let command: Command<()> = Rc::new(|_| Ok(1));
        node.add_child(
            LiteralArgumentBuilder::literal("child")
                .executes(command.clone())
                .build(),
        )
        .unwrap();
        node.add_child(literal("child")).unwrap();
        assert!(Rc::ptr_eq(&node.children()[0].command().unwrap(), &command));
    }

    fn official_abstract_add_child_overwrites_command(flavor: OfficialNodeFlavor) {
        let node = official_node(flavor);
        let command: Command<()> = Rc::new(|_| Ok(1));
        node.add_child(literal("child")).unwrap();
        node.add_child(
            LiteralArgumentBuilder::literal("child")
                .executes(command.clone())
                .build(),
        )
        .unwrap();
        assert!(Rc::ptr_eq(&node.children()[0].command().unwrap(), &command));
    }

    #[test]
    fn official_root_abstract_add_child() {
        official_abstract_add_child(OfficialNodeFlavor::Root);
    }

    #[test]
    fn official_root_abstract_add_child_merges_grandchildren() {
        official_abstract_add_child_merges_grandchildren(OfficialNodeFlavor::Root);
    }

    #[test]
    fn official_root_abstract_add_child_preserves_command() {
        official_abstract_add_child_preserves_command(OfficialNodeFlavor::Root);
    }

    #[test]
    fn official_root_abstract_add_child_overwrites_command() {
        official_abstract_add_child_overwrites_command(OfficialNodeFlavor::Root);
    }

    #[test]
    fn official_literal_abstract_add_child() {
        official_abstract_add_child(OfficialNodeFlavor::Literal);
    }

    #[test]
    fn official_literal_abstract_add_child_merges_grandchildren() {
        official_abstract_add_child_merges_grandchildren(OfficialNodeFlavor::Literal);
    }

    #[test]
    fn official_literal_abstract_add_child_preserves_command() {
        official_abstract_add_child_preserves_command(OfficialNodeFlavor::Literal);
    }

    #[test]
    fn official_literal_abstract_add_child_overwrites_command() {
        official_abstract_add_child_overwrites_command(OfficialNodeFlavor::Literal);
    }

    #[test]
    fn official_argument_abstract_add_child() {
        official_abstract_add_child(OfficialNodeFlavor::Argument);
    }

    #[test]
    fn official_argument_abstract_add_child_merges_grandchildren() {
        official_abstract_add_child_merges_grandchildren(OfficialNodeFlavor::Argument);
    }

    #[test]
    fn official_argument_abstract_add_child_preserves_command() {
        official_abstract_add_child_preserves_command(OfficialNodeFlavor::Argument);
    }

    #[test]
    fn official_argument_abstract_add_child_overwrites_command() {
        official_abstract_add_child_overwrites_command(OfficialNodeFlavor::Argument);
    }

    #[test]
    fn official_root_parse() {
        let node = Node::root();
        let mut reader = StringReader::new("hello world");
        let mut builder = context_builder((), Node::root(), 0);
        node.parse(&mut reader, &mut builder).unwrap();
        assert_eq!(reader.cursor(), 0);
    }

    #[test]
    fn official_root_add_child_no_root() {
        assert_eq!(
            Node::<()>::root().add_child(Node::root()),
            Err(TreeError::RootChild)
        );
    }

    #[test]
    fn official_root_usage() {
        assert_eq!(Node::<()>::root().usage_text(), "");
    }

    #[test]
    fn official_root_suggestions() {
        let node = Node::root();
        let context = context_builder((), Node::root(), 0).build("");
        let suggestions = ready(
            node.list_suggestions(&context, SuggestionsBuilder::new("", 0))
                .unwrap(),
        )
        .unwrap();
        assert!(suggestions.is_empty());
    }

    #[test]
    fn official_root_create_builder() {
        assert!(matches!(
            Node::<()>::root().create_builder(),
            Err(TreeError::RootBuilder)
        ));
    }

    #[test]
    fn official_root_equals() {
        let first = Node::<()>::root();
        let second = Node::<()>::root();
        assert!(first.java_equals(&second));
        first.add_child(literal("foo")).unwrap();
        assert!(!first.java_equals(&second));
        second.add_child(literal("foo")).unwrap();
        assert!(first.java_equals(&second));
    }

    #[test]
    fn official_literal_parse() {
        let node = literal::<()>("foo");
        let mut reader = StringReader::new("foo bar");
        let mut builder = context_builder((), Node::root(), 0);
        node.parse(&mut reader, &mut builder).unwrap();
        assert_eq!(reader.remaining(), " bar");
    }

    #[test]
    fn official_literal_parse_exact() {
        let node = literal::<()>("foo");
        let mut reader = StringReader::new("foo");
        let mut builder = context_builder((), Node::root(), 0);
        node.parse(&mut reader, &mut builder).unwrap();
        assert_eq!(reader.remaining(), "");
    }

    #[test]
    fn official_literal_parse_similar() {
        let node = literal::<()>("foo");
        let mut reader = StringReader::new("foobar");
        let mut builder = context_builder((), Node::root(), 0);
        let error = node.parse(&mut reader, &mut builder).unwrap_err();
        assert!(error.is_type(&BUILT_IN_EXCEPTIONS.literal_incorrect()));
        assert_eq!(error.cursor(), 0);
    }

    #[test]
    fn official_literal_parse_invalid() {
        let node = literal::<()>("foo");
        let mut reader = StringReader::new("bar");
        let mut builder = context_builder((), Node::root(), 0);
        let error = node.parse(&mut reader, &mut builder).unwrap_err();
        assert!(error.is_type(&BUILT_IN_EXCEPTIONS.literal_incorrect()));
        assert_eq!(error.cursor(), 0);
    }

    #[test]
    fn official_literal_usage() {
        assert_eq!(literal::<()>("foo").usage_text(), "foo");
    }

    #[test]
    fn official_literal_suggestions() {
        let node = literal::<()>("foo");
        let root = Node::root();
        let builder = context_builder((), root, 0);
        let empty = ready(
            node.list_suggestions(&builder.build(""), SuggestionsBuilder::new("", 0))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(empty.list().len(), 1);
        assert_eq!(empty.list()[0].range(), &StringRange::at(0));
        assert_eq!(empty.list()[0].text(), "foo");

        for input in ["foo", "food", "b"] {
            let result = ready(
                node.list_suggestions(&builder.build(input), SuggestionsBuilder::new(input, 0))
                    .unwrap(),
            )
            .unwrap();
            assert!(result.is_empty());
        }
    }

    #[test]
    fn official_literal_equals() {
        let command: Command<()> = Rc::new(|_| Ok(1));
        assert!(literal::<()>("foo").java_equals(&literal("foo")));
        assert!(!literal::<()>("foo").java_equals(&literal("bar")));
        let with_command = LiteralArgumentBuilder::literal("bar")
            .executes(command.clone())
            .build();
        assert!(
            with_command.java_equals(
                &LiteralArgumentBuilder::literal("bar")
                    .executes(command)
                    .build()
            )
        );
        assert!(!with_command.java_equals(&literal("bar")));
        let with_child: Node<()> = LiteralArgumentBuilder::literal("foo")
            .then(LiteralArgumentBuilder::literal("bar"))
            .unwrap()
            .build();
        let same_child: Node<()> = LiteralArgumentBuilder::literal("foo")
            .then(LiteralArgumentBuilder::literal("bar"))
            .unwrap()
            .build();
        assert!(with_child.java_equals(&same_child));
    }

    #[test]
    fn official_literal_create_builder() {
        let node = literal::<()>("foo");
        let node_requirement = node.requirement();
        let node_command = node.command();
        let ArgumentBuilder::Literal(builder) = node.create_builder().unwrap() else {
            panic!("literal node created a required argument builder");
        };
        assert_eq!(builder.literal_value(), node.literal_value().unwrap());
        assert!(Rc::ptr_eq(&builder.requirement(), &node_requirement));
        assert!(builder.command().is_none() && node_command.is_none());
    }

    #[test]
    fn official_argument_parse() {
        let node: Node<()> =
            RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer()).build();
        let mut reader = StringReader::new("123 456");
        let mut builder = context_builder((), Node::root(), 0);
        node.parse(&mut reader, &mut builder).unwrap();
        let arguments = builder.arguments();
        let argument = arguments
            .iter()
            .find_map(|(name, argument)| (name == "foo").then_some(argument))
            .unwrap();
        assert_eq!(*argument.result::<i32>().unwrap(), 123);
    }

    #[test]
    fn official_argument_usage() {
        let node: Node<()> =
            RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer()).build();
        assert_eq!(node.usage_text(), "<foo>");
        assert_eq!(node.to_string(), "<argument foo:integer()>");
    }

    #[test]
    fn official_argument_suggestions() {
        let node: Node<()> =
            RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer()).build();
        let context = context_builder((), Node::root(), 0).build("");
        let result = ready(
            node.list_suggestions(&context, SuggestionsBuilder::new("", 0))
                .unwrap(),
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn official_argument_equals() {
        let command: Command<()> = Rc::new(|_| Ok(1));
        let integer =
            || RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer()).build();
        assert!(integer().java_equals(&integer()));
        assert!(
            RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer())
                .executes(command.clone())
                .build()
                .java_equals(
                    &RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer(),)
                        .executes(command)
                        .build()
                )
        );
        let bounded_bar: Node<()> =
            RequiredArgumentBuilder::argument("bar", IntegerArgumentType::integer_range(-100, 100))
                .build();
        assert!(
            bounded_bar.java_equals(
                &RequiredArgumentBuilder::argument(
                    "bar",
                    IntegerArgumentType::integer_range(-100, 100),
                )
                .build()
            )
        );
        assert!(!bounded_bar.java_equals(&integer()));
        let with_child: Node<()> =
            RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer())
                .then(RequiredArgumentBuilder::argument(
                    "bar",
                    IntegerArgumentType::integer(),
                ))
                .unwrap()
                .build();
        let same_child: Node<()> =
            RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer())
                .then(RequiredArgumentBuilder::argument(
                    "bar",
                    IntegerArgumentType::integer(),
                ))
                .unwrap()
                .build();
        assert!(with_child.java_equals(&same_child));
    }

    #[test]
    fn official_argument_create_builder() {
        let node: Node<()> =
            RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer()).build();
        let node_requirement = node.requirement();
        let node_type = node.argument_type().unwrap();
        let node_command = node.command();
        let ArgumentBuilder::Required(builder) = node.create_builder().unwrap() else {
            panic!("argument node created a literal builder");
        };
        assert_eq!(builder.name(), node.name());
        assert_eq!(builder.argument_type(), &node_type);
        assert!(Rc::ptr_eq(&builder.requirement(), &node_requirement));
        assert!(builder.command().is_none() && node_command.is_none());
    }
}
