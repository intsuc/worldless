use std::cell::RefCell;
use std::cmp::Ordering;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use crate::builder::LiteralArgumentBuilder;
use crate::context::{CommandContextBuilder, ContextChain, ResultConsumer};
use crate::exceptions::{BUILT_IN_EXCEPTIONS, BuiltInExceptionProvider, CommandSyntaxException};
use crate::reader::StringReader;
use crate::suggestion::{Suggestions, SuggestionsBuilder, SuggestionsFuture};
use crate::tree::{AmbiguityConsumer, Node};

pub const ARGUMENT_SEPARATOR: &str = " ";
pub const ARGUMENT_SEPARATOR_CHAR: u16 = b' ' as u16;

pub struct ParseResults<S: 'static> {
    context: CommandContextBuilder<S>,
    reader: StringReader,
    exceptions: Vec<(Node<S>, CommandSyntaxException)>,
}

impl<S: 'static> Clone for ParseResults<S> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            reader: self.reader.clone(),
            exceptions: self.exceptions.clone(),
        }
    }
}

impl<S: 'static> ParseResults<S> {
    pub fn new(
        context: CommandContextBuilder<S>,
        reader: StringReader,
        exceptions: Vec<(Node<S>, CommandSyntaxException)>,
    ) -> Self {
        Self {
            context,
            reader,
            exceptions,
        }
    }

    pub fn from_context(context: CommandContextBuilder<S>) -> Self {
        Self::new(context, StringReader::new(""), Vec::new())
    }

    pub fn context(&self) -> &CommandContextBuilder<S> {
        &self.context
    }

    pub fn reader(&self) -> &StringReader {
        &self.reader
    }

    pub fn exceptions(&self) -> &[(Node<S>, CommandSyntaxException)] {
        &self.exceptions
    }
}

pub struct CommandDispatcher<S: 'static> {
    root: Node<S>,
    consumer: Rc<RefCell<ResultConsumer<S>>>,
}

impl<S: 'static> Clone for CommandDispatcher<S> {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            consumer: self.consumer.clone(),
        }
    }
}

impl<S: 'static> Default for CommandDispatcher<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: 'static> CommandDispatcher<S> {
    pub fn new() -> Self {
        Self::with_root(Node::root())
    }

    pub fn with_root(root: Node<S>) -> Self {
        assert!(
            root.is_root(),
            "dispatcher root must be a root command node"
        );
        Self {
            root,
            consumer: Rc::new(RefCell::new(Rc::new(|_, _, _| {}))),
        }
    }

    pub fn root(&self) -> Node<S> {
        self.root.clone()
    }

    pub fn register(
        &self,
        command: LiteralArgumentBuilder<S>,
    ) -> Result<Node<S>, crate::tree::TreeError> {
        let node = command.build();
        self.root.add_child(node.clone())?;
        Ok(node)
    }

    pub fn set_consumer(&self, consumer: ResultConsumer<S>) {
        *self.consumer.borrow_mut() = consumer;
    }

    pub fn parse(&self, command: &str, source: S) -> ParseResults<S> {
        self.parse_reader(StringReader::new(command), source)
    }

    pub fn parse_reader(&self, command: StringReader, source: S) -> ParseResults<S> {
        let start = command.cursor();
        let context = CommandContextBuilder::new_shared(
            self.clone(),
            Rc::new(source),
            self.root.clone(),
            start,
        );
        self.parse_nodes(self.root.clone(), command, context)
    }

    fn parse_nodes(
        &self,
        node: Node<S>,
        mut original_reader: StringReader,
        context_so_far: CommandContextBuilder<S>,
    ) -> ParseResults<S> {
        let source = context_so_far.source();
        let mut errors = Vec::new();
        let mut potentials = Vec::new();
        let cursor = original_reader.cursor();

        for child in node.relevant_nodes(&mut original_reader) {
            if !child.can_use(source.as_ref()) {
                continue;
            }

            let mut context = context_so_far.copy();
            let mut reader = original_reader.clone();
            let parsed = catch_unwind(AssertUnwindSafe(|| child.parse(&mut reader, &mut context)))
                .unwrap_or_else(|panic| {
                    Err(BUILT_IN_EXCEPTIONS
                        .dispatcher_parse_exception()
                        .create_with_context(&reader, panic_message(panic)))
                })
                .and_then(|()| {
                    if reader.can_read() && reader.peek() != ARGUMENT_SEPARATOR_CHAR {
                        Err(BUILT_IN_EXCEPTIONS
                            .dispatcher_expected_argument_separator()
                            .create_with_context(&reader))
                    } else {
                        Ok(())
                    }
                });

            if let Err(error) = parsed {
                errors.push((child, error));
                reader.set_cursor(cursor);
                continue;
            }

            context.with_command(child.command());
            let redirect = child.redirect();
            if reader.can_read_n(if redirect.is_none() { 2 } else { 1 }) {
                reader.skip();
                if let Some(redirect) = redirect {
                    let child_context = CommandContextBuilder::new_shared(
                        self.clone(),
                        source.clone(),
                        redirect.clone(),
                        reader.cursor(),
                    );
                    let parsed = self.parse_nodes(redirect, reader, child_context);
                    context.with_child(parsed.context);
                    return ParseResults::new(context, parsed.reader, parsed.exceptions);
                }

                potentials.push(self.parse_nodes(child, reader, context));
            } else {
                potentials.push(ParseResults::new(context, reader, Vec::new()));
            }
        }

        if !potentials.is_empty() {
            potentials.sort_by(compare_parse_results);
            potentials.remove(0)
        } else {
            ParseResults::new(context_so_far, original_reader, errors)
        }
    }

    pub fn execute(&self, input: &str, source: S) -> Result<i32, CommandSyntaxException> {
        self.execute_parse(self.parse(input, source))
    }

    pub fn execute_reader(
        &self,
        input: StringReader,
        source: S,
    ) -> Result<i32, CommandSyntaxException> {
        self.execute_parse(self.parse_reader(input, source))
    }

    pub fn execute_parse(&self, parse: ParseResults<S>) -> Result<i32, CommandSyntaxException> {
        if parse.reader.can_read() {
            if parse.exceptions.len() == 1 {
                return Err(parse.exceptions[0].1.clone());
            }
            if parse.context.range().is_empty() {
                return Err(BUILT_IN_EXCEPTIONS
                    .dispatcher_unknown_command()
                    .create_with_context(&parse.reader));
            }
            return Err(BUILT_IN_EXCEPTIONS
                .dispatcher_unknown_argument()
                .create_with_context(&parse.reader));
        }

        let original = parse.context.build_utf16(parse.reader.utf16().to_vec());
        let Some(chain) = ContextChain::try_flatten(original.clone()) else {
            let consumer = self.consumer.borrow().clone();
            consumer(&original, false, 0);
            return Err(BUILT_IN_EXCEPTIONS
                .dispatcher_unknown_command()
                .create_with_context(&parse.reader));
        };

        let source = original.shared_source();
        let consumer = self.consumer.borrow().clone();
        chain.execute_all_shared(source, consumer)
    }

    pub fn all_usage(&self, node: &Node<S>, source: &S, restricted: bool) -> Vec<String> {
        let mut result = Vec::new();
        self.collect_all_usage(node, source, &mut result, String::new(), restricted);
        result
    }

    fn collect_all_usage(
        &self,
        node: &Node<S>,
        source: &S,
        result: &mut Vec<String>,
        prefix: String,
        restricted: bool,
    ) {
        if restricted && !node.can_use(source) {
            return;
        }
        if node.command().is_some() {
            result.push(prefix.clone());
        }

        if let Some(redirect) = node.redirect() {
            let redirect_text = if redirect.ptr_eq(&self.root) {
                "...".to_owned()
            } else {
                format!("-> {}", redirect.usage_text())
            };
            result.push(if prefix.is_empty() {
                format!("{} {redirect_text}", node.usage_text())
            } else {
                format!("{prefix} {redirect_text}")
            });
        } else {
            for child in node.children() {
                let child_prefix = if prefix.is_empty() {
                    child.usage_text()
                } else {
                    format!("{prefix} {}", child.usage_text())
                };
                self.collect_all_usage(&child, source, result, child_prefix, restricted);
            }
        }
    }

    pub fn smart_usage(&self, node: &Node<S>, source: &S) -> Vec<(Node<S>, String)> {
        let optional = node.command().is_some();
        node.children()
            .into_iter()
            .filter_map(|child| {
                self.smart_usage_for(&child, source, optional, false)
                    .map(|usage| (child, usage))
            })
            .collect()
    }

    fn smart_usage_for(
        &self,
        node: &Node<S>,
        source: &S,
        optional: bool,
        deep: bool,
    ) -> Option<String> {
        if !node.can_use(source) {
            return None;
        }
        let usage_text = node.usage_text();
        let own_usage = if optional {
            format!("[{usage_text}]")
        } else {
            usage_text
        };
        let child_optional = node.command().is_some();

        if !deep {
            if let Some(redirect) = node.redirect() {
                let redirect_text = if redirect.ptr_eq(&self.root) {
                    "...".to_owned()
                } else {
                    format!("-> {}", redirect.usage_text())
                };
                return Some(format!("{own_usage} {redirect_text}"));
            }

            let children: Vec<_> = node
                .children()
                .into_iter()
                .filter(|child| child.can_use(source))
                .collect();
            if children.len() == 1 {
                if let Some(child_usage) =
                    self.smart_usage_for(&children[0], source, child_optional, child_optional)
                {
                    return Some(format!("{own_usage} {child_usage}"));
                }
            } else if children.len() > 1 {
                let mut unique = Vec::<String>::new();
                for child in &children {
                    if let Some(usage) = self.smart_usage_for(child, source, child_optional, true)
                        && !unique.contains(&usage)
                    {
                        unique.push(usage);
                    }
                }
                if unique.len() == 1 {
                    let usage = &unique[0];
                    return Some(format!(
                        "{own_usage} {}",
                        if child_optional {
                            format!("[{usage}]")
                        } else {
                            usage.clone()
                        }
                    ));
                }
                if unique.len() > 1 {
                    let alternatives = children
                        .iter()
                        .map(Node::usage_text)
                        .collect::<Vec<_>>()
                        .join("|");
                    let alternatives = if child_optional {
                        format!("[{alternatives}]")
                    } else {
                        format!("({alternatives})")
                    };
                    return Some(format!("{own_usage} {alternatives}"));
                }
            }
        }

        Some(own_usage)
    }

    pub fn completion_suggestions(&self, parse: &ParseResults<S>) -> SuggestionsFuture {
        self.completion_suggestions_at(parse, parse.reader.total_length())
    }

    pub fn completion_suggestions_at(
        &self,
        parse: &ParseResults<S>,
        cursor: usize,
    ) -> SuggestionsFuture {
        let suggestion_context = parse
            .context
            .find_suggestion_context(cursor)
            .expect("Can't find node before cursor");
        let parent = suggestion_context.parent.clone();
        let start = suggestion_context.start_pos.min(cursor);
        let full_input = parse.reader.utf16().to_vec();
        let truncated_input = parse.reader.substring_utf16(0, cursor);
        let command_context = suggestion_context
            .context
            .build_utf16(truncated_input.clone());
        let mut futures = Vec::with_capacity(parent.children().len());
        for node in parent.children() {
            let builder = SuggestionsBuilder::from_utf16(truncated_input.clone(), start);
            match node.list_suggestions(&command_context, builder) {
                Ok(future) => futures.push(future),
                Err(_) => futures.push(Box::pin(async { Ok(Suggestions::empty()) })),
            }
        }

        Box::pin(CompletionSuggestions::new(full_input, futures))
    }

    pub fn path(&self, target: &Node<S>) -> Vec<String> {
        let mut paths = Vec::new();
        collect_paths(&self.root, &mut paths, Vec::new());
        paths
            .into_iter()
            .find(|path| path.last().is_some_and(|node| node.ptr_eq(target)))
            .map(|path| {
                path.into_iter()
                    .filter(|node| !node.ptr_eq(&self.root))
                    .map(|node| node.name())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn find_node<'a>(&self, path: impl IntoIterator<Item = &'a str>) -> Option<Node<S>> {
        let mut node = self.root.clone();
        for name in path {
            node = node.child(name)?;
        }
        Some(node)
    }

    pub fn find_ambiguities(&self, consumer: &AmbiguityConsumer<S>) {
        self.root.find_ambiguities(consumer);
    }
}

struct CompletionSuggestions {
    input: Vec<u16>,
    futures: Vec<SuggestionsFuture>,
    results: Vec<Option<Suggestions>>,
    settled: Vec<bool>,
    failed: bool,
}

impl CompletionSuggestions {
    fn new(input: Vec<u16>, futures: Vec<SuggestionsFuture>) -> Self {
        let results = std::iter::repeat_with(|| None)
            .take(futures.len())
            .collect();
        let settled = vec![false; futures.len()];
        Self {
            input,
            futures,
            results,
            settled,
            failed: false,
        }
    }
}

impl Future for CompletionSuggestions {
    type Output = Result<Suggestions, CommandSyntaxException>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        let mut pending = false;
        for index in 0..this.futures.len() {
            if this.settled[index] {
                continue;
            }
            match this.futures[index].as_mut().poll(context) {
                Poll::Ready(Ok(suggestions)) => {
                    this.results[index] = Some(suggestions);
                    this.settled[index] = true;
                }
                // Brigadier discards the exceptional `allOf` continuation, so the
                // dispatcher-owned result remains incomplete in this case.
                Poll::Ready(Err(_)) => {
                    this.failed = true;
                    this.settled[index] = true;
                }
                Poll::Pending => pending = true,
            }
        }
        if pending || this.failed {
            return Poll::Pending;
        }
        let results = this
            .results
            .iter_mut()
            .map(|result| result.take().expect("all suggestion futures completed"))
            .collect::<Vec<_>>();
        Poll::Ready(Ok(Suggestions::merge_utf16(&this.input, &results)))
    }
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "null".to_owned()
    }
}

fn compare_parse_results<S: 'static>(a: &ParseResults<S>, b: &ParseResults<S>) -> Ordering {
    match (a.reader.can_read(), b.reader.can_read()) {
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        _ if a.exceptions.is_empty() && !b.exceptions.is_empty() => Ordering::Less,
        _ if !a.exceptions.is_empty() && b.exceptions.is_empty() => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

fn collect_paths<S: 'static>(
    node: &Node<S>,
    result: &mut Vec<Vec<Node<S>>>,
    parents: Vec<Node<S>>,
) {
    let mut current = parents;
    current.push(node.clone());
    result.push(current.clone());
    for child in node.children() {
        collect_paths(&child, result, current.clone());
    }
}

#[cfg(test)]
mod source_identity_tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::CommandDispatcher;
    use crate::builder::LiteralArgumentBuilder;
    use crate::tree::{Command, RedirectModifier, Requirement};

    struct Source {
        value: Cell<i32>,
    }

    #[test]
    fn requirement_and_command_observe_the_same_non_clone_source() {
        let dispatcher = CommandDispatcher::new();
        let requirement: Requirement<Source> = Rc::new(|source| {
            source.value.set(41);
            true
        });
        let command: Command<Source> = Rc::new(|context| Ok(context.source().value.get() + 1));

        dispatcher
            .register(
                LiteralArgumentBuilder::literal("run")
                    .requires(requirement)
                    .executes(command),
            )
            .unwrap();

        assert_eq!(
            dispatcher
                .execute(
                    "run",
                    Source {
                        value: Cell::new(0),
                    },
                )
                .unwrap(),
            42
        );
    }

    #[test]
    fn fork_commands_observe_the_modifier_source_handles() {
        let dispatcher = CommandDispatcher::new();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let command: Command<Source> = {
            let observed = observed.clone();
            Rc::new(move |context| {
                observed.borrow_mut().push(context.shared_source());
                Ok(context.source().value.get())
            })
        };
        dispatcher
            .register(LiteralArgumentBuilder::literal("run").executes(command))
            .unwrap();

        let first = Rc::new(Source {
            value: Cell::new(1),
        });
        let second = Rc::new(Source {
            value: Cell::new(2),
        });
        let modifier: RedirectModifier<Source> = {
            let first = first.clone();
            let second = second.clone();
            Rc::new(move |_| Ok(vec![first.clone(), second.clone()]))
        };
        dispatcher
            .register(
                LiteralArgumentBuilder::literal("fork")
                    .fork(dispatcher.root(), modifier)
                    .unwrap(),
            )
            .unwrap();

        assert_eq!(
            dispatcher
                .execute(
                    "fork run",
                    Source {
                        value: Cell::new(0),
                    },
                )
                .unwrap(),
            2
        );
        let observed = observed.borrow();
        assert_eq!(observed.len(), 2);
        assert!(Rc::ptr_eq(&observed[0], &first));
        assert!(Rc::ptr_eq(&observed[1], &second));
    }
}

#[cfg(test)]
mod command_suggestions_tests {
    use std::cell::Cell;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::task::{Context, Poll, Waker};

    use super::*;
    use crate::arguments::{ArgumentType, IntegerArgumentType, StringArgumentType};
    use crate::builder::RequiredArgumentBuilder;
    use crate::context::{CommandContext, StringRange};
    use crate::exceptions::SimpleCommandExceptionType;
    use crate::message::LiteralMessage;
    use crate::suggestion::{Suggestion, SuggestionProvider};
    use crate::tree::Command;

    fn literal(name: &str) -> LiteralArgumentBuilder<()> {
        LiteralArgumentBuilder::literal(name)
    }

    fn input_with_offset(input: &str, offset: usize) -> StringReader {
        let mut result = StringReader::new(input);
        result.set_cursor(offset);
        result
    }

    fn block_on(mut future: SuggestionsFuture) -> Result<Suggestions, CommandSyntaxException> {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(result) => return result,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    struct WaitUntilReleased(Rc<Cell<bool>>);

    impl Future for WaitUntilReleased {
        type Output = Result<Suggestions, CommandSyntaxException>;

        fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
            if self.0.get() {
                Poll::Ready(Ok(Suggestions::empty()))
            } else {
                Poll::Pending
            }
        }
    }

    struct Release(Rc<Cell<bool>>);

    impl Future for Release {
        type Output = Result<Suggestions, CommandSyntaxException>;

        fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
            self.0.set(true);
            context.waker().wake_by_ref();
            Poll::Ready(Ok(Suggestions::empty()))
        }
    }

    #[test]
    fn completion_futures_are_polled_together() {
        let released = Rc::new(Cell::new(false));
        let futures: Vec<SuggestionsFuture> = vec![
            Box::pin(WaitUntilReleased(released.clone())),
            Box::pin(Release(released)),
        ];
        let mut completion = Box::pin(CompletionSuggestions::new(Vec::new(), futures));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(completion.as_mut().poll(&mut context).is_pending());
        assert!(completion.as_mut().poll(&mut context).is_ready());
    }

    #[test]
    fn exceptional_completion_keeps_the_dispatcher_future_incomplete() {
        let error = SimpleCommandExceptionType::new(LiteralMessage::new("failed")).create();
        let released = Rc::new(Cell::new(false));
        let futures: Vec<SuggestionsFuture> = vec![
            Box::pin(std::future::ready(Err(error))),
            Box::pin(Release(released.clone())),
        ];
        let mut completion = Box::pin(CompletionSuggestions::new(Vec::new(), futures));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(completion.as_mut().poll(&mut context).is_pending());
        assert!(released.get());
        assert!(completion.as_mut().poll(&mut context).is_pending());
    }

    struct PanickingArgument;

    impl ArgumentType<()> for PanickingArgument {
        type Value = ();

        fn parse(&self, _reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
            panic!("broken parser")
        }
    }

    #[test]
    fn argument_parser_panics_become_dispatcher_parse_exceptions() {
        let dispatcher = CommandDispatcher::new();
        dispatcher
            .register(
                literal("run")
                    .then(RequiredArgumentBuilder::argument(
                        "value",
                        PanickingArgument,
                    ))
                    .unwrap(),
            )
            .unwrap();

        let parse = dispatcher.parse("run value", ());
        assert_eq!(parse.exceptions().len(), 1);
        let error = &parse.exceptions()[0].1;
        assert!(error.is_type(&BUILT_IN_EXCEPTIONS.dispatcher_parse_exception()));
        assert_eq!(
            error.raw_message().string(),
            "Could not parse command: broken parser"
        );
        assert_eq!(error.cursor(), 4);
    }

    #[test]
    fn missing_executable_consumer_can_replace_itself() {
        let dispatcher = CommandDispatcher::new();
        dispatcher.register(literal("foo")).unwrap();
        let called = Rc::new(Cell::new(false));
        let dispatcher_from_consumer = dispatcher.clone();
        let called_from_consumer = called.clone();
        dispatcher.set_consumer(Rc::new(move |_, _, _| {
            called_from_consumer.set(true);
            dispatcher_from_consumer.set_consumer(Rc::new(|_, _, _| {}));
        }));

        assert!(dispatcher.execute("foo", ()).is_err());
        assert!(called.get());
    }

    fn test_suggestions(
        subject: &CommandDispatcher<()>,
        contents: &str,
        cursor: usize,
        range: StringRange,
        suggestions: &[&str],
    ) {
        let result =
            block_on(subject.completion_suggestions_at(&subject.parse(contents, ()), cursor))
                .unwrap();
        assert_eq!(result.range(), &range);
        let expected: Vec<_> = suggestions
            .iter()
            .map(|text| Suggestion::new(range, text))
            .collect();
        assert_eq!(result.list(), expected);
    }

    fn register_root_commands(subject: &CommandDispatcher<()>) {
        subject.register(literal("foo")).unwrap();
        subject.register(literal("bar")).unwrap();
        subject.register(literal("baz")).unwrap();
    }

    fn register_parent_commands(subject: &CommandDispatcher<()>) {
        let parent = literal("parent")
            .then(literal("foo"))
            .unwrap()
            .then(literal("bar"))
            .unwrap()
            .then(literal("baz"))
            .unwrap();
        subject.register(parent).unwrap();
    }

    #[test]
    fn get_completion_suggestions_root_commands() {
        let subject = CommandDispatcher::new();
        register_root_commands(&subject);

        let result = block_on(subject.completion_suggestions(&subject.parse("", ()))).unwrap();

        assert_eq!(result.range(), &StringRange::at(0));
        assert_eq!(
            result.list(),
            [
                Suggestion::new(StringRange::at(0), "bar"),
                Suggestion::new(StringRange::at(0), "baz"),
                Suggestion::new(StringRange::at(0), "foo"),
            ]
        );
    }

    #[test]
    fn get_completion_suggestions_root_commands_with_input_offset() {
        let subject = CommandDispatcher::new();
        register_root_commands(&subject);

        let parse = subject.parse_reader(input_with_offset("OOO", 3), ());
        let result = block_on(subject.completion_suggestions(&parse)).unwrap();

        assert_eq!(result.range(), &StringRange::at(3));
        assert_eq!(
            result.list(),
            [
                Suggestion::new(StringRange::at(3), "bar"),
                Suggestion::new(StringRange::at(3), "baz"),
                Suggestion::new(StringRange::at(3), "foo"),
            ]
        );
    }

    #[test]
    fn get_completion_suggestions_root_commands_partial() {
        let subject = CommandDispatcher::new();
        register_root_commands(&subject);

        let result = block_on(subject.completion_suggestions(&subject.parse("b", ()))).unwrap();

        let range = StringRange::between(0, 1);
        assert_eq!(result.range(), &range);
        assert_eq!(
            result.list(),
            [Suggestion::new(range, "bar"), Suggestion::new(range, "baz")]
        );
    }

    #[test]
    fn get_completion_suggestions_root_commands_partial_with_input_offset() {
        let subject = CommandDispatcher::new();
        register_root_commands(&subject);

        let parse = subject.parse_reader(input_with_offset("Zb", 1), ());
        let result = block_on(subject.completion_suggestions(&parse)).unwrap();

        let range = StringRange::between(1, 2);
        assert_eq!(result.range(), &range);
        assert_eq!(
            result.list(),
            [Suggestion::new(range, "bar"), Suggestion::new(range, "baz")]
        );
    }

    #[test]
    fn get_completion_suggestions_sub_commands() {
        let subject = CommandDispatcher::new();
        register_parent_commands(&subject);

        let result =
            block_on(subject.completion_suggestions(&subject.parse("parent ", ()))).unwrap();

        assert_eq!(result.range(), &StringRange::at(7));
        assert_eq!(
            result.list(),
            [
                Suggestion::new(StringRange::at(7), "bar"),
                Suggestion::new(StringRange::at(7), "baz"),
                Suggestion::new(StringRange::at(7), "foo"),
            ]
        );
    }

    #[test]
    fn get_completion_suggestions_moving_cursor_sub_commands() {
        let subject = CommandDispatcher::new();
        let parent_one = literal("parent_one")
            .then(literal("faz"))
            .unwrap()
            .then(literal("fbz"))
            .unwrap()
            .then(literal("gaz"))
            .unwrap();
        subject.register(parent_one).unwrap();
        subject.register(literal("parent_two")).unwrap();

        test_suggestions(
            &subject,
            "parent_one faz ",
            0,
            StringRange::at(0),
            &["parent_one", "parent_two"],
        );
        test_suggestions(
            &subject,
            "parent_one faz ",
            1,
            StringRange::between(0, 1),
            &["parent_one", "parent_two"],
        );
        test_suggestions(
            &subject,
            "parent_one faz ",
            7,
            StringRange::between(0, 7),
            &["parent_one", "parent_two"],
        );
        test_suggestions(
            &subject,
            "parent_one faz ",
            8,
            StringRange::between(0, 8),
            &["parent_one"],
        );
        test_suggestions(&subject, "parent_one faz ", 10, StringRange::at(0), &[]);
        test_suggestions(
            &subject,
            "parent_one faz ",
            11,
            StringRange::at(11),
            &["faz", "fbz", "gaz"],
        );
        test_suggestions(
            &subject,
            "parent_one faz ",
            12,
            StringRange::between(11, 12),
            &["faz", "fbz"],
        );
        test_suggestions(
            &subject,
            "parent_one faz ",
            13,
            StringRange::between(11, 13),
            &["faz"],
        );
        test_suggestions(&subject, "parent_one faz ", 14, StringRange::at(0), &[]);
        test_suggestions(&subject, "parent_one faz ", 15, StringRange::at(0), &[]);
    }

    #[test]
    fn get_completion_suggestions_sub_commands_partial() {
        let subject = CommandDispatcher::new();
        register_parent_commands(&subject);

        let parse = subject.parse("parent b", ());
        let result = block_on(subject.completion_suggestions(&parse)).unwrap();

        let range = StringRange::between(7, 8);
        assert_eq!(result.range(), &range);
        assert_eq!(
            result.list(),
            [Suggestion::new(range, "bar"), Suggestion::new(range, "baz")]
        );
    }

    #[test]
    fn get_completion_suggestions_sub_commands_partial_with_input_offset() {
        let subject = CommandDispatcher::new();
        register_parent_commands(&subject);

        let parse = subject.parse_reader(input_with_offset("junk parent b", 5), ());
        let result = block_on(subject.completion_suggestions(&parse)).unwrap();

        let range = StringRange::between(12, 13);
        assert_eq!(result.range(), &range);
        assert_eq!(
            result.list(),
            [Suggestion::new(range, "bar"), Suggestion::new(range, "baz")]
        );
    }

    #[test]
    fn get_completion_suggestions_redirect() {
        let subject = CommandDispatcher::new();
        let actual = subject
            .register(literal("actual").then(literal("sub")).unwrap())
            .unwrap();
        subject
            .register(literal("redirect").redirect(actual).unwrap())
            .unwrap();

        let parse = subject.parse("redirect ", ());
        let result = block_on(subject.completion_suggestions(&parse)).unwrap();

        assert_eq!(result.range(), &StringRange::at(9));
        assert_eq!(result.list(), [Suggestion::new(StringRange::at(9), "sub")]);
    }

    #[test]
    fn get_completion_suggestions_redirect_partial() {
        let subject = CommandDispatcher::new();
        let actual = subject
            .register(literal("actual").then(literal("sub")).unwrap())
            .unwrap();
        subject
            .register(literal("redirect").redirect(actual).unwrap())
            .unwrap();

        let parse = subject.parse("redirect s", ());
        let result = block_on(subject.completion_suggestions(&parse)).unwrap();

        let range = StringRange::between(9, 10);
        assert_eq!(result.range(), &range);
        assert_eq!(result.list(), [Suggestion::new(range, "sub")]);
    }

    #[test]
    fn get_completion_suggestions_moving_cursor_redirect() {
        let subject = CommandDispatcher::new();
        let actual_one = subject
            .register(
                literal("actual_one")
                    .then(literal("faz"))
                    .unwrap()
                    .then(literal("fbz"))
                    .unwrap()
                    .then(literal("gaz"))
                    .unwrap(),
            )
            .unwrap();
        subject.register(literal("actual_two")).unwrap();
        subject
            .register(
                literal("redirect_one")
                    .redirect(actual_one.clone())
                    .unwrap(),
            )
            .unwrap();
        subject
            .register(literal("redirect_two").redirect(actual_one).unwrap())
            .unwrap();

        test_suggestions(
            &subject,
            "redirect_one faz ",
            0,
            StringRange::at(0),
            &["actual_one", "actual_two", "redirect_one", "redirect_two"],
        );
        test_suggestions(
            &subject,
            "redirect_one faz ",
            9,
            StringRange::between(0, 9),
            &["redirect_one", "redirect_two"],
        );
        test_suggestions(
            &subject,
            "redirect_one faz ",
            10,
            StringRange::between(0, 10),
            &["redirect_one"],
        );
        test_suggestions(&subject, "redirect_one faz ", 12, StringRange::at(0), &[]);
        test_suggestions(
            &subject,
            "redirect_one faz ",
            13,
            StringRange::at(13),
            &["faz", "fbz", "gaz"],
        );
        test_suggestions(
            &subject,
            "redirect_one faz ",
            14,
            StringRange::between(13, 14),
            &["faz", "fbz"],
        );
        test_suggestions(
            &subject,
            "redirect_one faz ",
            15,
            StringRange::between(13, 15),
            &["faz"],
        );
        test_suggestions(&subject, "redirect_one faz ", 16, StringRange::at(0), &[]);
        test_suggestions(&subject, "redirect_one faz ", 17, StringRange::at(0), &[]);
    }

    #[test]
    fn get_completion_suggestions_redirect_partial_with_input_offset() {
        let subject = CommandDispatcher::new();
        let actual = subject
            .register(literal("actual").then(literal("sub")).unwrap())
            .unwrap();
        subject
            .register(literal("redirect").redirect(actual).unwrap())
            .unwrap();

        let parse = subject.parse_reader(input_with_offset("/redirect s", 1), ());
        let result = block_on(subject.completion_suggestions(&parse)).unwrap();

        let range = StringRange::between(10, 11);
        assert_eq!(result.range(), &range);
        assert_eq!(result.list(), [Suggestion::new(range, "sub")]);
    }

    #[test]
    fn get_completion_suggestions_redirect_lots() {
        let subject = CommandDispatcher::new();
        let loop_node = subject.register(literal("redirect")).unwrap();
        let loop_argument =
            RequiredArgumentBuilder::argument("loop", IntegerArgumentType::integer())
                .redirect(loop_node)
                .unwrap();
        let loop_literal = literal("loop").then(loop_argument).unwrap();
        subject
            .register(literal("redirect").then(loop_literal).unwrap())
            .unwrap();

        let parse = subject.parse("redirect loop 1 loop 02 loop 003 ", ());
        let result = block_on(subject.completion_suggestions(&parse)).unwrap();

        assert_eq!(result.range(), &StringRange::at(33));
        assert_eq!(
            result.list(),
            [Suggestion::new(StringRange::at(33), "loop")]
        );
    }

    #[test]
    fn get_completion_suggestions_redirect_contextual_argument() {
        let subject = CommandDispatcher::new();
        let provider: SuggestionProvider<()> = Rc::new(
            |context: &CommandContext<()>, mut builder: SuggestionsBuilder| {
                let arg_one = StringArgumentType::get_string(context, "arg_one")
                    .expect("arg_one was parsed before suggestions are requested");
                builder.suggest(format!("contextual_{arg_one}"));
                Ok(builder.build_future())
            },
        );
        let arg_two = RequiredArgumentBuilder::argument("arg_two", StringArgumentType::word())
            .suggests(provider);
        let arg_one = RequiredArgumentBuilder::argument("arg_one", StringArgumentType::word())
            .then(arg_two)
            .unwrap();
        let actual = subject
            .register(literal("actual").then(arg_one).unwrap())
            .unwrap();
        subject
            .register(literal("redirect").redirect(actual).unwrap())
            .unwrap();

        let parse = subject.parse("redirect first ", ());
        let result = block_on(subject.completion_suggestions(&parse)).unwrap();

        assert_eq!(result.range(), &StringRange::at(15));
        assert_eq!(
            result.list(),
            [Suggestion::new(StringRange::at(15), "contextual_first")]
        );
    }

    #[test]
    fn get_completion_suggestions_execute_simulation() {
        let subject = CommandDispatcher::new();
        let execute = subject.register(literal("execute")).unwrap();
        let as_argument = RequiredArgumentBuilder::argument("name", StringArgumentType::word())
            .redirect(execute.clone())
            .unwrap();
        let store_argument = RequiredArgumentBuilder::argument("name", StringArgumentType::word())
            .redirect(execute)
            .unwrap();
        let command: Command<()> = Rc::new(|_| Ok(0));
        let execute_builder = literal("execute")
            .then(literal("as").then(as_argument).unwrap())
            .unwrap()
            .then(literal("store").then(store_argument).unwrap())
            .unwrap()
            .then(literal("run").executes(command))
            .unwrap();
        subject.register(execute_builder).unwrap();

        let parse = subject.parse("execute as Dinnerbone as", ());
        let result = block_on(subject.completion_suggestions(&parse)).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn get_completion_suggestions_execute_simulation_partial() {
        let subject = CommandDispatcher::new();
        let execute = subject.register(literal("execute")).unwrap();
        let as_builder = literal("as")
            .then(literal("bar").redirect(execute.clone()).unwrap())
            .unwrap()
            .then(literal("baz").redirect(execute.clone()).unwrap())
            .unwrap();
        let store_argument = RequiredArgumentBuilder::argument("name", StringArgumentType::word())
            .redirect(execute)
            .unwrap();
        let command: Command<()> = Rc::new(|_| Ok(0));
        let execute_builder = literal("execute")
            .then(as_builder)
            .unwrap()
            .then(literal("store").then(store_argument).unwrap())
            .unwrap()
            .then(literal("run").executes(command))
            .unwrap();
        subject.register(execute_builder).unwrap();

        let parse = subject.parse("execute as bar as ", ());
        let result = block_on(subject.completion_suggestions(&parse)).unwrap();

        assert_eq!(result.range(), &StringRange::at(18));
        assert_eq!(
            result.list(),
            [
                Suggestion::new(StringRange::at(18), "bar"),
                Suggestion::new(StringRange::at(18), "baz"),
            ]
        );
    }
}
