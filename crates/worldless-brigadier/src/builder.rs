use std::{error::Error, fmt, rc::Rc};

use crate::{
    arguments::{ArgumentType, ArgumentTypeRef},
    context::CommandContext,
    exceptions::CommandSyntaxException,
    suggestion::SuggestionProvider,
    tree::{Command, Node, RedirectModifier, Requirement, TreeError},
};

pub type SingleRedirectModifier<S> =
    Rc<dyn Fn(&CommandContext<S>) -> Result<Rc<S>, CommandSyntaxException>>;

struct BuilderData<S: 'static> {
    arguments: Node<S>,
    command: Option<Command<S>>,
    requirement: Requirement<S>,
    redirect: Option<Node<S>>,
    modifier: Option<RedirectModifier<S>>,
    forks: bool,
}

impl<S: 'static> BuilderData<S> {
    fn new() -> Self {
        Self {
            arguments: Node::root(),
            command: None,
            requirement: Rc::new(|_| true),
            redirect: None,
            modifier: None,
            forks: false,
        }
    }

    fn then(self, node: Node<S>) -> Result<Self, BuilderError> {
        if self.redirect.is_some() {
            return Err(BuilderError::ChildAfterRedirect);
        }
        self.arguments.add_child(node)?;
        Ok(self)
    }

    fn forward(
        mut self,
        target: Option<Node<S>>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Result<Self, BuilderError> {
        if !self.arguments.children().is_empty() {
            return Err(BuilderError::RedirectWithChildren);
        }
        self.redirect = target;
        self.modifier = modifier;
        self.forks = forks;
        Ok(self)
    }
}

pub struct LiteralArgumentBuilder<S: 'static> {
    literal: String,
    data: BuilderData<S>,
}

impl<S: 'static> LiteralArgumentBuilder<S> {
    pub fn literal(literal: impl Into<String>) -> Self {
        Self {
            literal: literal.into(),
            data: BuilderData::new(),
        }
    }

    pub fn then(mut self, argument: impl Into<ArgumentBuilder<S>>) -> Result<Self, BuilderError> {
        self.data = self.data.then(argument.into().build())?;
        Ok(self)
    }

    pub fn then_node(mut self, node: Node<S>) -> Result<Self, BuilderError> {
        self.data = self.data.then(node)?;
        Ok(self)
    }

    pub fn executes(mut self, command: Command<S>) -> Self {
        self.data.command = Some(command);
        self
    }

    pub fn requires(mut self, requirement: Requirement<S>) -> Self {
        self.data.requirement = requirement;
        self
    }

    pub fn redirect(self, target: Node<S>) -> Result<Self, BuilderError> {
        self.forward(target, None, false)
    }

    pub fn redirect_with_modifier(
        self,
        target: Node<S>,
        modifier: SingleRedirectModifier<S>,
    ) -> Result<Self, BuilderError> {
        let modifier: RedirectModifier<S> =
            Rc::new(move |context| modifier(context).map(|s| vec![s]));
        self.forward(target, Some(modifier), false)
    }

    pub fn fork(
        self,
        target: Node<S>,
        modifier: RedirectModifier<S>,
    ) -> Result<Self, BuilderError> {
        self.forward(target, Some(modifier), true)
    }

    pub fn forward(
        mut self,
        target: Node<S>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Result<Self, BuilderError> {
        self.data = self.data.forward(Some(target), modifier, forks)?;
        Ok(self)
    }

    pub(crate) fn forward_option(
        mut self,
        target: Option<Node<S>>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Result<Self, BuilderError> {
        self.data = self.data.forward(target, modifier, forks)?;
        Ok(self)
    }

    pub fn literal_value(&self) -> &str {
        &self.literal
    }

    pub fn arguments(&self) -> Vec<Node<S>> {
        self.data.arguments.children()
    }

    pub fn command(&self) -> Option<Command<S>> {
        self.data.command.clone()
    }

    pub fn requirement(&self) -> Requirement<S> {
        self.data.requirement.clone()
    }

    pub fn redirect_target(&self) -> Option<Node<S>> {
        self.data.redirect.clone()
    }

    pub fn redirect_modifier(&self) -> Option<RedirectModifier<S>> {
        self.data.modifier.clone()
    }

    pub fn is_fork(&self) -> bool {
        self.data.forks
    }

    pub fn build(self) -> Node<S> {
        let node = Node::literal(
            self.literal,
            self.data.command,
            self.data.requirement,
            self.data.redirect,
            self.data.modifier,
            self.data.forks,
        );
        for child in self.data.arguments.children() {
            node.add_child(child)
                .expect("builder arguments cannot contain root nodes");
        }
        node
    }
}

pub struct RequiredArgumentBuilder<S: 'static> {
    name: String,
    argument_type: ArgumentTypeRef<S>,
    suggestions: Option<SuggestionProvider<S>>,
    data: BuilderData<S>,
}

impl<S: 'static> RequiredArgumentBuilder<S> {
    pub fn argument<A: ArgumentType<S>>(name: impl Into<String>, argument_type: A) -> Self {
        Self::from_ref(name.into(), ArgumentTypeRef::new(argument_type))
    }

    pub(crate) fn from_ref(name: String, argument_type: ArgumentTypeRef<S>) -> Self {
        Self {
            name,
            argument_type,
            suggestions: None,
            data: BuilderData::new(),
        }
    }

    pub fn then(mut self, argument: impl Into<ArgumentBuilder<S>>) -> Result<Self, BuilderError> {
        self.data = self.data.then(argument.into().build())?;
        Ok(self)
    }

    pub fn then_node(mut self, node: Node<S>) -> Result<Self, BuilderError> {
        self.data = self.data.then(node)?;
        Ok(self)
    }

    pub fn executes(mut self, command: Command<S>) -> Self {
        self.data.command = Some(command);
        self
    }

    pub fn requires(mut self, requirement: Requirement<S>) -> Self {
        self.data.requirement = requirement;
        self
    }

    pub fn suggests(mut self, provider: SuggestionProvider<S>) -> Self {
        self.suggestions = Some(provider);
        self
    }

    pub(crate) fn suggests_option(mut self, provider: Option<SuggestionProvider<S>>) -> Self {
        self.suggestions = provider;
        self
    }

    pub fn redirect(self, target: Node<S>) -> Result<Self, BuilderError> {
        self.forward(target, None, false)
    }

    pub fn redirect_with_modifier(
        self,
        target: Node<S>,
        modifier: SingleRedirectModifier<S>,
    ) -> Result<Self, BuilderError> {
        let modifier: RedirectModifier<S> =
            Rc::new(move |context| modifier(context).map(|s| vec![s]));
        self.forward(target, Some(modifier), false)
    }

    pub fn fork(
        self,
        target: Node<S>,
        modifier: RedirectModifier<S>,
    ) -> Result<Self, BuilderError> {
        self.forward(target, Some(modifier), true)
    }

    pub fn forward(
        mut self,
        target: Node<S>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Result<Self, BuilderError> {
        self.data = self.data.forward(Some(target), modifier, forks)?;
        Ok(self)
    }

    pub(crate) fn forward_option(
        mut self,
        target: Option<Node<S>>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Result<Self, BuilderError> {
        self.data = self.data.forward(target, modifier, forks)?;
        Ok(self)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn argument_type(&self) -> &ArgumentTypeRef<S> {
        &self.argument_type
    }

    pub fn suggestions_provider(&self) -> Option<SuggestionProvider<S>> {
        self.suggestions.clone()
    }

    pub fn arguments(&self) -> Vec<Node<S>> {
        self.data.arguments.children()
    }

    pub fn command(&self) -> Option<Command<S>> {
        self.data.command.clone()
    }

    pub fn requirement(&self) -> Requirement<S> {
        self.data.requirement.clone()
    }

    pub fn redirect_target(&self) -> Option<Node<S>> {
        self.data.redirect.clone()
    }

    pub fn redirect_modifier(&self) -> Option<RedirectModifier<S>> {
        self.data.modifier.clone()
    }

    pub fn is_fork(&self) -> bool {
        self.data.forks
    }

    pub fn build(self) -> Node<S> {
        let node = Node::argument(
            self.name,
            self.argument_type,
            self.data.command,
            self.data.requirement,
            self.data.redirect,
            self.data.modifier,
            self.data.forks,
            self.suggestions,
        );
        for child in self.data.arguments.children() {
            node.add_child(child)
                .expect("builder arguments cannot contain root nodes");
        }
        node
    }
}

pub enum ArgumentBuilder<S: 'static> {
    Literal(LiteralArgumentBuilder<S>),
    Required(RequiredArgumentBuilder<S>),
}

impl<S: 'static> ArgumentBuilder<S> {
    pub fn then(self, argument: impl Into<Self>) -> Result<Self, BuilderError> {
        match self {
            Self::Literal(builder) => builder.then(argument).map(Self::Literal),
            Self::Required(builder) => builder.then(argument).map(Self::Required),
        }
    }

    pub fn then_node(self, node: Node<S>) -> Result<Self, BuilderError> {
        match self {
            Self::Literal(builder) => builder.then_node(node).map(Self::Literal),
            Self::Required(builder) => builder.then_node(node).map(Self::Required),
        }
    }

    pub fn executes(self, command: Command<S>) -> Self {
        match self {
            Self::Literal(builder) => Self::Literal(builder.executes(command)),
            Self::Required(builder) => Self::Required(builder.executes(command)),
        }
    }

    pub fn requires(self, requirement: Requirement<S>) -> Self {
        match self {
            Self::Literal(builder) => Self::Literal(builder.requires(requirement)),
            Self::Required(builder) => Self::Required(builder.requires(requirement)),
        }
    }

    pub fn redirect(self, target: Node<S>) -> Result<Self, BuilderError> {
        self.forward(target, None, false)
    }

    pub fn redirect_with_modifier(
        self,
        target: Node<S>,
        modifier: SingleRedirectModifier<S>,
    ) -> Result<Self, BuilderError> {
        let modifier: RedirectModifier<S> =
            Rc::new(move |context| modifier(context).map(|source| vec![source]));
        self.forward(target, Some(modifier), false)
    }

    pub fn fork(
        self,
        target: Node<S>,
        modifier: RedirectModifier<S>,
    ) -> Result<Self, BuilderError> {
        self.forward(target, Some(modifier), true)
    }

    pub fn forward(
        self,
        target: Node<S>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Result<Self, BuilderError> {
        self.forward_option(Some(target), modifier, forks)
    }

    pub(crate) fn forward_option(
        self,
        target: Option<Node<S>>,
        modifier: Option<RedirectModifier<S>>,
        forks: bool,
    ) -> Result<Self, BuilderError> {
        match self {
            Self::Literal(builder) => builder
                .forward_option(target, modifier, forks)
                .map(Self::Literal),
            Self::Required(builder) => builder
                .forward_option(target, modifier, forks)
                .map(Self::Required),
        }
    }

    pub fn build(self) -> Node<S> {
        match self {
            Self::Literal(builder) => builder.build(),
            Self::Required(builder) => builder.build(),
        }
    }
}

impl<S: 'static> From<LiteralArgumentBuilder<S>> for ArgumentBuilder<S> {
    fn from(builder: LiteralArgumentBuilder<S>) -> Self {
        Self::Literal(builder)
    }
}

impl<S: 'static> From<RequiredArgumentBuilder<S>> for ArgumentBuilder<S> {
    fn from(builder: RequiredArgumentBuilder<S>) -> Self {
        Self::Required(builder)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuilderError {
    ChildAfterRedirect,
    RedirectWithChildren,
    RootChild,
}

impl fmt::Display for BuilderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChildAfterRedirect => {
                formatter.write_str("cannot add children to a redirected node")
            }
            Self::RedirectWithChildren => {
                formatter.write_str("cannot redirect a node that already has children")
            }
            Self::RootChild => formatter.write_str("cannot add a root node as a child"),
        }
    }
}

impl Error for BuilderError {}

impl From<TreeError> for BuilderError {
    fn from(error: TreeError) -> Self {
        match error {
            TreeError::RootChild => Self::RootChild,
            TreeError::RootBuilder => unreachable!("builders never convert a root to a builder"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use crate::{
        arguments::IntegerArgumentType, context::CommandContextBuilder,
        dispatcher::CommandDispatcher,
    };

    use super::*;

    fn literal<S: 'static>(name: &str) -> LiteralArgumentBuilder<S> {
        LiteralArgumentBuilder::literal(name)
    }

    fn context_builder<S: 'static>(
        source: S,
        root: Node<S>,
        start: usize,
    ) -> CommandContextBuilder<S> {
        CommandContextBuilder::new(CommandDispatcher::new(), source, root, start)
    }

    #[test]
    fn arguments_are_built_immediately_and_keep_insertion_order() {
        let builder = literal::<()>("parent")
            .then(literal("first"))
            .unwrap()
            .then(literal("second"))
            .unwrap();
        assert_eq!(
            builder
                .arguments()
                .iter()
                .map(Node::name)
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
    }

    #[test]
    fn same_name_children_merge_during_then() {
        let builder = literal::<()>("parent")
            .then(literal("child").then(literal("one")).unwrap())
            .unwrap()
            .then(literal("child").then(literal("two")).unwrap())
            .unwrap();
        let children = builder.arguments();
        assert_eq!(children.len(), 1);
        assert_eq!(
            children[0]
                .children()
                .iter()
                .map(Node::name)
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
    }

    #[test]
    fn redirect_records_target() {
        let target = literal::<()>("target").build();
        let builder = literal("source").redirect(target.clone()).unwrap();
        assert!(builder.redirect_target().unwrap().ptr_eq(&target));
        assert!(builder.redirect_modifier().is_none());
        assert!(!builder.is_fork());
    }

    #[test]
    fn redirect_after_child_is_rejected() {
        let target = literal::<()>("target").build();
        let result = literal("source")
            .then(literal("child"))
            .unwrap()
            .redirect(target);
        assert!(matches!(result, Err(BuilderError::RedirectWithChildren)));
    }

    #[test]
    fn child_after_redirect_is_rejected() {
        let target = literal::<()>("target").build();
        let result = literal("source")
            .redirect(target)
            .unwrap()
            .then(literal("child"));
        assert!(matches!(result, Err(BuilderError::ChildAfterRedirect)));
    }

    #[test]
    fn root_child_is_rejected_at_builder_boundary() {
        let result = literal::<()>("source").then_node(Node::root());
        assert!(matches!(result, Err(BuilderError::RootChild)));
    }

    #[test]
    fn literal_build_sets_name_usage_and_requirement() {
        let node = literal::<i32>("foo")
            .requires(Rc::new(|source| *source == 42))
            .build();
        assert_eq!(node.name(), "foo");
        assert_eq!(node.usage_text(), "foo");
        assert!(node.can_use(&42));
        assert!(!node.can_use(&0));
        assert!(node.command().is_none());
    }

    #[test]
    fn literal_build_with_executor_preserves_callback_identity() {
        let command: Command<()> = Rc::new(|_| Ok(42));
        let node = literal("foo").executes(command.clone()).build();
        assert!(Rc::ptr_eq(&node.command().unwrap(), &command));
    }

    #[test]
    fn literal_build_with_children_preserves_tree() {
        let node = literal::<()>("foo")
            .then(literal("bar"))
            .unwrap()
            .then(RequiredArgumentBuilder::argument(
                "value",
                IntegerArgumentType::integer(),
            ))
            .unwrap()
            .build();
        assert_eq!(
            node.children().iter().map(Node::name).collect::<Vec<_>>(),
            ["bar", "value"]
        );
    }

    #[test]
    fn required_build_sets_name_type_usage_and_requirement() {
        let node: Node<i32> =
            RequiredArgumentBuilder::argument("value", IntegerArgumentType::integer_range(1, 10))
                .requires(Rc::new(|source| *source == 42))
                .build();
        assert_eq!(node.name(), "value");
        assert_eq!(node.usage_text(), "<value>");
        assert!(node.can_use(&42));
        assert!(!node.can_use(&0));
        assert_eq!(
            node.argument_type().unwrap(),
            ArgumentTypeRef::new(IntegerArgumentType::integer_range(1, 10))
        );
    }

    #[test]
    fn required_build_with_executor_preserves_callback_identity() {
        let command: Command<()> = Rc::new(|_| Ok(42));
        let node = RequiredArgumentBuilder::argument("value", IntegerArgumentType::integer())
            .executes(command.clone())
            .build();
        assert!(Rc::ptr_eq(&node.command().unwrap(), &command));
    }

    #[test]
    fn required_build_with_children_preserves_tree() {
        let node: Node<()> =
            RequiredArgumentBuilder::argument("value", IntegerArgumentType::integer())
                .then(literal("bar"))
                .unwrap()
                .build();
        assert_eq!(node.children().len(), 1);
        assert_eq!(node.children()[0].name(), "bar");
    }

    #[test]
    fn fork_and_single_redirect_modifiers_keep_their_shapes() {
        let target = literal::<i32>("target").build();
        let single: SingleRedirectModifier<i32> =
            Rc::new(|context| Ok(Rc::new(context.source() + 1)));
        let single_node = literal("single")
            .redirect_with_modifier(target.clone(), single)
            .unwrap()
            .build();
        assert!(!single_node.is_fork());

        let fork: RedirectModifier<i32> = Rc::new(|context| {
            Ok(vec![
                Rc::new(*context.source()),
                Rc::new(context.source() + 1),
            ])
        });
        let fork_node = literal("fork").fork(target, fork).unwrap().build();
        assert!(fork_node.is_fork());

        let context = context_builder(4, Node::root(), 0).build("");
        assert_eq!(
            single_node.redirect_modifier().unwrap()(&context).unwrap(),
            [Rc::new(5)]
        );
        assert_eq!(
            fork_node.redirect_modifier().unwrap()(&context).unwrap(),
            [Rc::new(4), Rc::new(5)]
        );
    }

    #[test]
    fn custom_suggestion_provider_is_retained() {
        let called = Rc::new(Cell::new(false));
        let provider: SuggestionProvider<()> = {
            let called = called.clone();
            Rc::new(move |_, builder| {
                called.set(true);
                Ok(builder.build_future())
            })
        };
        let node = RequiredArgumentBuilder::argument("value", IntegerArgumentType::integer())
            .suggests(provider.clone())
            .build();
        assert!(Rc::ptr_eq(&node.custom_suggestions().unwrap(), &provider));
        let context = context_builder((), Node::root(), 0).build("");
        drop(
            node.list_suggestions(&context, crate::suggestion::SuggestionsBuilder::new("", 0))
                .unwrap(),
        );
        assert!(called.get());
    }

    #[test]
    fn node_builder_enum_remains_fluent() {
        let command: Command<()> = Rc::new(|_| Ok(1));
        let node = ArgumentBuilder::from(literal::<()>("foo"))
            .requires(Rc::new(|_| true))
            .executes(command)
            .then(literal("bar"))
            .unwrap()
            .build();
        assert_eq!(node.name(), "foo");
        assert_eq!(node.children()[0].name(), "bar");
    }

    #[test]
    fn official_argument_builder_arguments() {
        let first = literal::<()>("first").build();
        let second = literal::<()>("second").build();
        let builder = literal("base")
            .then_node(first.clone())
            .unwrap()
            .then_node(second.clone())
            .unwrap();
        assert_eq!(builder.arguments().len(), 2);
        assert!(builder.arguments()[0].ptr_eq(&first));
        assert!(builder.arguments()[1].ptr_eq(&second));
    }

    #[test]
    fn official_argument_builder_redirect() {
        let target = literal::<()>("target").build();
        let builder = literal("base").redirect(target.clone()).unwrap();
        assert!(builder.redirect_target().unwrap().ptr_eq(&target));
    }

    #[test]
    fn official_argument_builder_redirect_with_child() {
        let target = literal::<()>("target").build();
        let builder = literal("base").then(literal("child")).unwrap();
        assert!(matches!(
            builder.redirect(target),
            Err(BuilderError::RedirectWithChildren)
        ));
    }

    #[test]
    fn official_argument_builder_then_with_redirect() {
        let target = literal::<()>("target").build();
        let builder = literal("base").redirect(target).unwrap();
        assert!(matches!(
            builder.then(literal("child")),
            Err(BuilderError::ChildAfterRedirect)
        ));
    }

    #[test]
    fn official_literal_argument_builder_build() {
        let node = literal::<()>("foo").build();
        assert_eq!(node.literal_value().as_deref(), Some("foo"));
        assert!(node.command().is_none());
    }

    #[test]
    fn official_literal_argument_builder_build_with_executor() {
        let command: Command<()> = Rc::new(|_| Ok(42));
        let node = literal("foo").executes(command.clone()).build();
        assert!(Rc::ptr_eq(&node.command().unwrap(), &command));
    }

    #[test]
    fn official_literal_argument_builder_build_with_children() {
        let child = literal::<()>("bar").build();
        let node = literal("foo").then_node(child.clone()).unwrap().build();
        assert_eq!(node.children().len(), 1);
        assert!(node.children()[0].ptr_eq(&child));
    }

    #[test]
    fn official_required_argument_builder_build() {
        let argument_type = IntegerArgumentType::integer();
        let node: Node<()> = RequiredArgumentBuilder::argument("foo", argument_type).build();
        assert_eq!(node.name(), "foo");
        assert_eq!(
            node.argument_type().unwrap(),
            ArgumentTypeRef::new(argument_type)
        );
        assert!(node.command().is_none());
    }

    #[test]
    fn official_required_argument_builder_build_with_executor() {
        let command: Command<()> = Rc::new(|_| Ok(42));
        let node = RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer())
            .executes(command.clone())
            .build();
        assert!(Rc::ptr_eq(&node.command().unwrap(), &command));
    }

    #[test]
    fn official_required_argument_builder_build_with_children() {
        let first: Node<()> =
            RequiredArgumentBuilder::argument("bar", IntegerArgumentType::integer()).build();
        let second: Node<()> =
            RequiredArgumentBuilder::argument("baz", IntegerArgumentType::integer()).build();
        let node = RequiredArgumentBuilder::argument("foo", IntegerArgumentType::integer())
            .then_node(first.clone())
            .unwrap()
            .then_node(second.clone())
            .unwrap()
            .build();
        assert_eq!(node.children().len(), 2);
        assert!(node.children()[0].ptr_eq(&first));
        assert!(node.children()[1].ptr_eq(&second));
    }
}
