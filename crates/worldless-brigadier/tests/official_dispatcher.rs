use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use worldless_brigadier::{
    CommandDispatcher, StringReader,
    arguments::IntegerArgumentType,
    builder::{LiteralArgumentBuilder, RequiredArgumentBuilder},
    context::ResultConsumer,
    exceptions::{
        BUILT_IN_EXCEPTIONS, BuiltInExceptionProvider, CommandExceptionType, CommandSyntaxException,
    },
    tree::{Command, Node, RedirectModifier},
};

#[derive(Clone, Debug)]
struct Source(Rc<()>);

impl Source {
    fn new() -> Self {
        Self(Rc::new(()))
    }

    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

fn literal<S: Clone + 'static>(name: &str) -> LiteralArgumentBuilder<S> {
    LiteralArgumentBuilder::literal(name)
}

fn integer<S: Clone + 'static>(name: &str) -> RequiredArgumentBuilder<S> {
    RequiredArgumentBuilder::argument(name, IntegerArgumentType::integer())
}

fn recording_command<S: Clone + 'static>(result: i32, calls: Rc<RefCell<Vec<S>>>) -> Command<S> {
    Rc::new(move |context| {
        calls.borrow_mut().push(context.source().clone());
        Ok(result)
    })
}

fn assert_error_type<T>(
    result: Result<T, CommandSyntaxException>,
    expected: &impl CommandExceptionType,
    cursor: isize,
) {
    let error = match result {
        Ok(_) => panic!("command must fail"),
        Err(error) => error,
    };
    assert!(error.is_type(expected), "unexpected error: {error}");
    assert_eq!(error.cursor(), cursor);
}

fn assert_same_exception(actual: &CommandSyntaxException, expected: &CommandSyntaxException) {
    assert_eq!(actual.exception_type(), expected.exception_type());
    let actual_message = actual.raw_message();
    let expected_message = expected.raw_message();
    assert_eq!(actual_message.string(), expected_message.string());
    assert_eq!(actual_message.hash_code(), expected_message.hash_code());
    assert_eq!(
        actual_message as *const dyn worldless_brigadier::Message as *const (),
        expected_message as *const dyn worldless_brigadier::Message as *const (),
    );
    assert_eq!(actual.input_utf16(), expected.input_utf16());
    assert_eq!(actual.cursor(), expected.cursor());
}

#[test]
fn command_dispatcher_test_create_and_execute_command() {
    let subject = CommandDispatcher::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    subject
        .register(literal("foo").executes(recording_command(42, calls.clone())))
        .unwrap();
    let source = Source::new();

    assert_eq!(subject.execute("foo", source.clone()).unwrap(), 42);
    assert_eq!(calls.borrow().len(), 1);
    assert!(calls.borrow()[0].same(&source));
}

#[test]
fn command_dispatcher_test_create_and_execute_offset_command() {
    let subject = CommandDispatcher::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    subject
        .register(literal("foo").executes(recording_command(42, calls.clone())))
        .unwrap();
    let mut input = StringReader::new("/foo");
    input.set_cursor(1);

    assert_eq!(subject.execute_reader(input, Source::new()).unwrap(), 42);
    assert_eq!(calls.borrow().len(), 1);
}

#[test]
fn command_dispatcher_test_create_and_merge_commands() {
    let subject = CommandDispatcher::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    subject
        .register(
            literal("base")
                .then(literal("foo").executes(recording_command(42, calls.clone())))
                .unwrap(),
        )
        .unwrap();
    subject
        .register(
            literal("base")
                .then(literal("bar").executes(recording_command(42, calls.clone())))
                .unwrap(),
        )
        .unwrap();

    assert_eq!(subject.execute("base foo", Source::new()).unwrap(), 42);
    assert_eq!(subject.execute("base bar", Source::new()).unwrap(), 42);
    assert_eq!(calls.borrow().len(), 2);
}

#[test]
fn command_dispatcher_test_execute_unknown_command() {
    let subject = CommandDispatcher::<Source>::new();
    subject.register(literal("bar")).unwrap();
    subject.register(literal("baz")).unwrap();
    assert_error_type(
        subject.execute("foo", Source::new()),
        &BUILT_IN_EXCEPTIONS.dispatcher_unknown_command(),
        0,
    );
}

#[test]
fn command_dispatcher_test_execute_impermissible_command() {
    let subject = CommandDispatcher::<Source>::new();
    subject
        .register(literal("foo").requires(Rc::new(|_| false)))
        .unwrap();
    assert_error_type(
        subject.execute("foo", Source::new()),
        &BUILT_IN_EXCEPTIONS.dispatcher_unknown_command(),
        0,
    );
}

#[test]
fn command_dispatcher_test_execute_empty_command() {
    let subject = CommandDispatcher::<Source>::new();
    subject.register(literal("")).unwrap();
    assert_error_type(
        subject.execute("", Source::new()),
        &BUILT_IN_EXCEPTIONS.dispatcher_unknown_command(),
        0,
    );
}

#[test]
fn command_dispatcher_test_execute_unknown_subcommand() {
    let subject = CommandDispatcher::new();
    subject
        .register(literal("foo").executes(Rc::new(|_| Ok(42))))
        .unwrap();
    assert_error_type(
        subject.execute("foo bar", Source::new()),
        &BUILT_IN_EXCEPTIONS.dispatcher_unknown_argument(),
        4,
    );
}

#[test]
fn command_dispatcher_test_execute_incorrect_literal() {
    let subject = CommandDispatcher::new();
    subject
        .register(
            literal("foo")
                .executes(Rc::new(|_| Ok(42)))
                .then(literal("bar"))
                .unwrap(),
        )
        .unwrap();
    assert_error_type(
        subject.execute("foo baz", Source::new()),
        &BUILT_IN_EXCEPTIONS.dispatcher_unknown_argument(),
        4,
    );
}

#[test]
fn command_dispatcher_test_execute_ambiguous_incorrect_argument() {
    let subject = CommandDispatcher::new();
    let command = literal("foo")
        .executes(Rc::new(|_| Ok(42)))
        .then(literal("bar"))
        .unwrap()
        .then(literal("baz"))
        .unwrap();
    subject.register(command).unwrap();
    assert_error_type(
        subject.execute("foo unknown", Source::new()),
        &BUILT_IN_EXCEPTIONS.dispatcher_unknown_argument(),
        4,
    );
}

#[test]
fn command_dispatcher_test_execute_subcommand() {
    let subject = CommandDispatcher::new();
    let calls = Rc::new(Cell::new(0));
    let subcommand: Command<Source> = {
        let calls = calls.clone();
        Rc::new(move |_| {
            calls.set(calls.get() + 1);
            Ok(100)
        })
    };
    let command = literal("foo")
        .then(literal("a"))
        .unwrap()
        .then(literal("=").executes(subcommand))
        .unwrap()
        .then(literal("c"))
        .unwrap()
        .executes(Rc::new(|_| Ok(42)));
    subject.register(command).unwrap();

    assert_eq!(subject.execute("foo =", Source::new()).unwrap(), 100);
    assert_eq!(calls.get(), 1);
}

#[test]
fn command_dispatcher_test_parse_incomplete_literal() {
    let subject = CommandDispatcher::new();
    subject
        .register(
            literal("foo")
                .then(literal("bar").executes(Rc::new(|_| Ok(42))))
                .unwrap(),
        )
        .unwrap();
    let parse = subject.parse("foo ", Source::new());
    assert_eq!(parse.reader().remaining(), " ");
    assert_eq!(parse.context().nodes().len(), 1);
}

#[test]
fn command_dispatcher_test_parse_incomplete_argument() {
    let subject = CommandDispatcher::new();
    subject
        .register(
            literal("foo")
                .then(integer("bar").executes(Rc::new(|_| Ok(42))))
                .unwrap(),
        )
        .unwrap();
    let parse = subject.parse("foo ", Source::new());
    assert_eq!(parse.reader().remaining(), " ");
    assert_eq!(parse.context().nodes().len(), 1);
}

#[test]
fn command_dispatcher_test_execute_ambiguous_parent_subcommand() {
    let subject = CommandDispatcher::new();
    let parent_calls = Rc::new(Cell::new(0));
    let sub_calls = Rc::new(Cell::new(0));
    let parent: Command<Source> = {
        let calls = parent_calls.clone();
        Rc::new(move |_| {
            calls.set(calls.get() + 1);
            Ok(42)
        })
    };
    let sub: Command<Source> = {
        let calls = sub_calls.clone();
        Rc::new(move |_| {
            calls.set(calls.get() + 1);
            Ok(100)
        })
    };
    let command = literal("test")
        .then(integer("incorrect").executes(parent))
        .unwrap()
        .then(integer("right").then(integer("sub").executes(sub)).unwrap())
        .unwrap();
    subject.register(command).unwrap();

    assert_eq!(subject.execute("test 1 2", Source::new()).unwrap(), 100);
    assert_eq!(sub_calls.get(), 1);
    assert_eq!(parent_calls.get(), 0);
}

#[test]
fn command_dispatcher_test_execute_ambiguous_parent_subcommand_via_redirect() {
    let subject = CommandDispatcher::new();
    let parent_calls = Rc::new(Cell::new(0));
    let sub_calls = Rc::new(Cell::new(0));
    let parent: Command<Source> = {
        let calls = parent_calls.clone();
        Rc::new(move |_| {
            calls.set(calls.get() + 1);
            Ok(42)
        })
    };
    let sub: Command<Source> = {
        let calls = sub_calls.clone();
        Rc::new(move |_| {
            calls.set(calls.get() + 1);
            Ok(100)
        })
    };
    let command = literal("test")
        .then(integer("incorrect").executes(parent))
        .unwrap()
        .then(integer("right").then(integer("sub").executes(sub)).unwrap())
        .unwrap();
    let target = subject.register(command).unwrap();
    subject
        .register(literal("redirect").redirect(target).unwrap())
        .unwrap();

    assert_eq!(subject.execute("redirect 1 2", Source::new()).unwrap(), 100);
    assert_eq!(sub_calls.get(), 1);
    assert_eq!(parent_calls.get(), 0);
}

#[test]
fn command_dispatcher_test_execute_redirected_multiple_times() {
    let subject = CommandDispatcher::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let concrete = subject
        .register(literal("actual").executes(recording_command(42, calls.clone())))
        .unwrap();
    let redirected = subject
        .register(literal("redirected").redirect(subject.root()).unwrap())
        .unwrap();
    let input = "redirected redirected actual";
    let parse = subject.parse(input, Source::new());
    let context = parse.context();
    assert_eq!(context.range().get(input).unwrap(), "redirected");
    assert_eq!(context.nodes().len(), 1);
    assert!(context.root_node().ptr_eq(&subject.root()));
    assert_eq!(context.nodes()[0].range(), context.range());
    assert!(context.nodes()[0].node().ptr_eq(&redirected));
    let child1 = context.child().unwrap();
    assert_eq!(child1.range().get(input).unwrap(), "redirected");
    assert_eq!(child1.nodes().len(), 1);
    assert!(child1.root_node().ptr_eq(&subject.root()));
    assert_eq!(child1.nodes()[0].range(), child1.range());
    assert!(child1.nodes()[0].node().ptr_eq(&redirected));
    let child2 = child1.child().unwrap();
    assert_eq!(child2.range().get(input).unwrap(), "actual");
    assert_eq!(child2.nodes().len(), 1);
    assert!(child2.root_node().ptr_eq(&subject.root()));
    assert_eq!(child2.nodes()[0].range(), child2.range());
    assert!(child2.nodes()[0].node().ptr_eq(&concrete));
    assert_eq!(subject.execute_parse(parse).unwrap(), 42);
    assert_eq!(calls.borrow().len(), 1);
}

#[test]
fn command_dispatcher_test_correct_execute_context_after_redirect() {
    let subject = CommandDispatcher::<i32>::new();
    let root = subject.root();
    let add_value = integer("value")
        .redirect_with_modifier(
            root.clone(),
            Rc::new(|context| {
                Ok(Rc::new(
                    *context.source() + IntegerArgumentType::get_integer(context, "value").unwrap(),
                ))
            }),
        )
        .unwrap();
    subject
        .register(literal("add").then(add_value).unwrap())
        .unwrap();
    subject
        .register(literal("blank").redirect(root.clone()).unwrap())
        .unwrap();
    subject
        .register(literal("run").executes(Rc::new(|context| Ok(*context.source()))))
        .unwrap();

    assert_eq!(subject.execute("run", 0).unwrap(), 0);
    assert_eq!(subject.execute("run", 1).unwrap(), 1);
    assert_eq!(subject.execute("add 5 run", 1).unwrap(), 6);
    assert_eq!(subject.execute("add 5 add 6 run", 2).unwrap(), 13);
    assert_eq!(subject.execute("add 5 blank run", 1).unwrap(), 6);
    assert_eq!(subject.execute("blank add 5 run", 1).unwrap(), 6);
    assert_eq!(subject.execute("add 5 blank add 6 run", 2).unwrap(), 13);
    assert_eq!(
        subject.execute("add 5 blank blank add 6 run", 2).unwrap(),
        13
    );
}

#[test]
fn command_dispatcher_test_shared_redirect_and_execute_nodes() {
    let subject = CommandDispatcher::<i32>::new();
    let add_value = integer("value")
        .redirect_with_modifier(
            subject.root(),
            Rc::new(|context| {
                Ok(Rc::new(
                    *context.source() + IntegerArgumentType::get_integer(context, "value").unwrap(),
                ))
            }),
        )
        .unwrap()
        .executes(Rc::new(|context| Ok(*context.source())));
    subject
        .register(literal("add").then(add_value).unwrap())
        .unwrap();

    assert_eq!(subject.execute("add 5", 1).unwrap(), 1);
    assert_eq!(subject.execute("add 5 add 6", 1).unwrap(), 6);
}

#[test]
fn command_dispatcher_test_execute_redirected() {
    let subject = CommandDispatcher::new();
    let original = Source::new();
    let first = Source::new();
    let second = Source::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let concrete = subject
        .register(literal("actual").executes(recording_command(42, calls.clone())))
        .unwrap();
    let modifier: RedirectModifier<Source> = {
        let first = first.clone();
        let second = second.clone();
        Rc::new(move |_| Ok(vec![Rc::new(first.clone()), Rc::new(second.clone())]))
    };
    let redirected = subject
        .register(
            literal("redirected")
                .fork(subject.root(), modifier)
                .unwrap(),
        )
        .unwrap();
    let input = "redirected actual";
    let parse = subject.parse(input, original.clone());
    let context = parse.context();
    assert_eq!(context.range().get(input).unwrap(), "redirected");
    assert_eq!(context.nodes().len(), 1);
    assert!(context.root_node().ptr_eq(&subject.root()));
    assert_eq!(context.nodes()[0].range(), context.range());
    assert!(context.nodes()[0].node().ptr_eq(&redirected));
    assert!(context.source().same(&original));
    let child = context.child().unwrap();
    assert_eq!(child.range().get(input).unwrap(), "actual");
    assert_eq!(child.nodes().len(), 1);
    assert!(child.root_node().ptr_eq(&subject.root()));
    assert_eq!(child.nodes()[0].range(), child.range());
    assert!(child.nodes()[0].node().ptr_eq(&concrete));
    assert!(child.source().same(&original));
    assert_eq!(subject.execute_parse(parse).unwrap(), 2);
    assert_eq!(calls.borrow().len(), 2);
    assert!(calls.borrow().iter().any(|source| source.same(&first)));
    assert!(calls.borrow().iter().any(|source| source.same(&second)));
}

#[test]
fn command_dispatcher_test_incomplete_redirect_should_throw() {
    let subject = CommandDispatcher::new();
    let foo = subject
        .register(
            literal("foo")
                .then(
                    literal("bar")
                        .then(integer("value").executes(Rc::new(|context| {
                            Ok(IntegerArgumentType::get_integer(context, "value").unwrap())
                        })))
                        .unwrap(),
                )
                .unwrap()
                .then(literal("awa").executes(Rc::new(|_| Ok(2))))
                .unwrap(),
        )
        .unwrap();
    subject
        .register(literal("baz").redirect(foo).unwrap())
        .unwrap();
    let error = subject
        .execute("baz bar", Source::new())
        .expect_err("incomplete redirect must fail");
    assert!(error.is_type(&BUILT_IN_EXCEPTIONS.dispatcher_unknown_command()));
}

#[test]
fn command_dispatcher_test_redirect_modifier_empty_result() {
    let subject = CommandDispatcher::new();
    let foo = subject
        .register(
            literal("foo")
                .then(
                    literal("bar")
                        .then(integer("value").executes(Rc::new(|context| {
                            Ok(IntegerArgumentType::get_integer(context, "value").unwrap())
                        })))
                        .unwrap(),
                )
                .unwrap()
                .then(literal("awa").executes(Rc::new(|_| Ok(2))))
                .unwrap(),
        )
        .unwrap();
    subject
        .register(
            literal("baz")
                .fork(foo, Rc::new(|_| Ok(Vec::new())))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(subject.execute("baz bar 100", Source::new()).unwrap(), 0);
}

#[test]
fn command_dispatcher_test_execute_orphaned_subcommand() {
    let subject = CommandDispatcher::new();
    subject
        .register(
            literal("foo")
                .then(integer("bar"))
                .unwrap()
                .executes(Rc::new(|_| Ok(42))),
        )
        .unwrap();
    assert_error_type(
        subject.execute("foo 5", Source::new()),
        &BUILT_IN_EXCEPTIONS.dispatcher_unknown_command(),
        5,
    );
}

#[test]
fn command_dispatcher_test_execute_invalid_other() {
    let subject = CommandDispatcher::new();
    let wrong_calls = Rc::new(Cell::new(0));
    let right_calls = Rc::new(Cell::new(0));
    let wrong: Command<Source> = {
        let calls = wrong_calls.clone();
        Rc::new(move |_| {
            calls.set(calls.get() + 1);
            Ok(1)
        })
    };
    let right: Command<Source> = {
        let calls = right_calls.clone();
        Rc::new(move |_| {
            calls.set(calls.get() + 1);
            Ok(42)
        })
    };
    subject.register(literal("w").executes(wrong)).unwrap();
    subject.register(literal("world").executes(right)).unwrap();
    assert_eq!(subject.execute("world", Source::new()).unwrap(), 42);
    assert_eq!(wrong_calls.get(), 0);
    assert_eq!(right_calls.get(), 1);
}

#[test]
fn command_dispatcher_parse_no_space_separator() {
    let subject = CommandDispatcher::new();
    subject
        .register(
            literal("foo")
                .then(integer("bar").executes(Rc::new(|_| Ok(42))))
                .unwrap(),
        )
        .unwrap();
    assert_error_type(
        subject.execute("foo$", Source::new()),
        &BUILT_IN_EXCEPTIONS.dispatcher_unknown_command(),
        0,
    );
}

#[test]
fn command_dispatcher_test_execute_invalid_subcommand() {
    let subject = CommandDispatcher::new();
    subject
        .register(
            literal("foo")
                .then(integer("bar"))
                .unwrap()
                .executes(Rc::new(|_| Ok(42))),
        )
        .unwrap();
    assert_error_type(
        subject.execute("foo bar", Source::new()),
        &BUILT_IN_EXCEPTIONS.reader_expected_int(),
        4,
    );
}

#[test]
fn command_dispatcher_test_get_path() {
    let subject = CommandDispatcher::<Source>::new();
    let bar = literal::<Source>("bar").build();
    subject
        .register(literal("foo").then_node(bar.clone()).unwrap())
        .unwrap();
    assert_eq!(subject.path(&bar), ["foo", "bar"]);
}

#[test]
fn command_dispatcher_test_find_node_exists() {
    let subject = CommandDispatcher::<Source>::new();
    let bar = literal::<Source>("bar").build();
    subject
        .register(literal("foo").then_node(bar.clone()).unwrap())
        .unwrap();
    assert!(subject.find_node(["foo", "bar"]).unwrap().ptr_eq(&bar));
}

#[test]
fn command_dispatcher_test_find_node_doesnt_exist() {
    let subject = CommandDispatcher::<Source>::new();
    assert!(subject.find_node(["foo", "bar"]).is_none());
}

#[test]
fn command_dispatcher_test_result_consumer_in_non_error_run() {
    let subject = CommandDispatcher::new();
    let consumer_calls = Rc::new(RefCell::new(Vec::new()));
    let consumer: ResultConsumer<Source> = {
        let calls = consumer_calls.clone();
        Rc::new(move |context, success, result| {
            calls
                .borrow_mut()
                .push((context.source().clone(), success, result));
        })
    };
    subject.set_consumer(consumer);
    subject
        .register(literal("foo").executes(Rc::new(|_| Ok(5))))
        .unwrap();
    let source = Source::new();
    assert_eq!(subject.execute("foo", source.clone()).unwrap(), 5);
    let calls = consumer_calls.borrow();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].0.same(&source));
    assert_eq!((calls[0].1, calls[0].2), (true, 5));
}

#[test]
fn command_dispatcher_test_result_consumer_in_forked_non_error_run() {
    let subject = CommandDispatcher::<i32>::new();
    let consumer_calls = Rc::new(RefCell::new(Vec::new()));
    subject.set_consumer({
        let calls = consumer_calls.clone();
        Rc::new(move |context, success, result| {
            calls
                .borrow_mut()
                .push((*context.source(), success, result));
        })
    });
    subject
        .register(literal("foo").executes(Rc::new(|context| Ok(*context.source()))))
        .unwrap();
    subject
        .register(
            literal("repeat")
                .fork(
                    subject.root(),
                    Rc::new(|_| Ok(vec![Rc::new(9), Rc::new(10), Rc::new(11)])),
                )
                .unwrap(),
        )
        .unwrap();
    assert_eq!(subject.execute("repeat foo", 0).unwrap(), 3);
    assert_eq!(
        *consumer_calls.borrow(),
        [(9, true, 9), (10, true, 10), (11, true, 11)]
    );
}

fn register_crashing_command(
    subject: &CommandDispatcher<Source>,
    exception: CommandSyntaxException,
) {
    subject
        .register(literal("crash").executes(Rc::new(move |_| Err(exception.clone()))))
        .unwrap();
}

fn recording_consumer(calls: Rc<RefCell<Vec<(Source, bool, i32)>>>) -> ResultConsumer<Source> {
    Rc::new(move |context, success, result| {
        calls
            .borrow_mut()
            .push((context.source().clone(), success, result));
    })
}

#[test]
fn command_dispatcher_test_exception_in_non_forked_command() {
    let subject = CommandDispatcher::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    subject.set_consumer(recording_consumer(calls.clone()));
    let expected = BUILT_IN_EXCEPTIONS.reader_expected_bool().create();
    register_crashing_command(&subject, expected.clone());
    let error = subject
        .execute("crash", Source::new())
        .expect_err("command must propagate its exception");
    assert_same_exception(&error, &expected);
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!((calls.borrow()[0].1, calls.borrow()[0].2), (false, 0));
}

#[test]
fn command_dispatcher_test_exception_in_non_forked_redirected_command() {
    let subject = CommandDispatcher::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    subject.set_consumer(recording_consumer(calls.clone()));
    let expected = BUILT_IN_EXCEPTIONS.reader_expected_bool().create();
    register_crashing_command(&subject, expected.clone());
    subject
        .register(literal("redirect").redirect(subject.root()).unwrap())
        .unwrap();
    let error = subject
        .execute("redirect crash", Source::new())
        .expect_err("redirected command must propagate its exception");
    assert_same_exception(&error, &expected);
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!((calls.borrow()[0].1, calls.borrow()[0].2), (false, 0));
}

#[test]
fn command_dispatcher_test_exception_in_forked_redirected_command() {
    let subject = CommandDispatcher::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    subject.set_consumer(recording_consumer(calls.clone()));
    register_crashing_command(
        &subject,
        BUILT_IN_EXCEPTIONS.reader_expected_bool().create(),
    );
    subject
        .register(
            literal("redirect")
                .fork(
                    subject.root(),
                    Rc::new(|context| Ok(vec![context.shared_source()])),
                )
                .unwrap(),
        )
        .unwrap();
    assert_eq!(subject.execute("redirect crash", Source::new()).unwrap(), 0);
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!((calls.borrow()[0].1, calls.borrow()[0].2), (false, 0));
}

#[test]
fn command_dispatcher_test_exception_in_non_forked_redirect() {
    let subject = CommandDispatcher::new();
    let consumer_calls = Rc::new(RefCell::new(Vec::new()));
    let command_calls = Rc::new(Cell::new(0));
    subject.set_consumer(recording_consumer(consumer_calls.clone()));
    subject
        .register(literal("noop").executes({
            let calls = command_calls.clone();
            Rc::new(move |_| {
                calls.set(calls.get() + 1);
                Ok(3)
            })
        }))
        .unwrap();
    let exception = BUILT_IN_EXCEPTIONS.reader_expected_bool().create();
    let thrown = exception.clone();
    subject
        .register(
            literal("redirect")
                .redirect_with_modifier(subject.root(), Rc::new(move |_| Err(thrown.clone())))
                .unwrap(),
        )
        .unwrap();
    let error = subject
        .execute("redirect noop", Source::new())
        .expect_err("redirect modifier must propagate its exception");
    assert_same_exception(&error, &exception);
    assert_eq!(command_calls.get(), 0);
    assert_eq!(consumer_calls.borrow().len(), 1);
    assert_eq!(
        (consumer_calls.borrow()[0].1, consumer_calls.borrow()[0].2),
        (false, 0)
    );
}

#[test]
fn command_dispatcher_test_exception_in_forked_redirect() {
    let subject = CommandDispatcher::new();
    let consumer_calls = Rc::new(RefCell::new(Vec::new()));
    let command_calls = Rc::new(Cell::new(0));
    subject.set_consumer(recording_consumer(consumer_calls.clone()));
    subject
        .register(literal("noop").executes({
            let calls = command_calls.clone();
            Rc::new(move |_| {
                calls.set(calls.get() + 1);
                Ok(3)
            })
        }))
        .unwrap();
    let exception = BUILT_IN_EXCEPTIONS.reader_expected_bool().create();
    subject
        .register(
            literal("redirect")
                .fork(subject.root(), Rc::new(move |_| Err(exception.clone())))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(subject.execute("redirect noop", Source::new()).unwrap(), 0);
    assert_eq!(command_calls.get(), 0);
    assert_eq!(consumer_calls.borrow().len(), 1);
    assert_eq!(
        (consumer_calls.borrow()[0].1, consumer_calls.borrow()[0].2),
        (false, 0)
    );
}

#[test]
fn command_dispatcher_test_partial_exception_in_forked_redirect() {
    let subject = CommandDispatcher::new();
    let original = Source::new();
    let rejected = Source::new();
    let other = Source::new();
    let command_calls = Rc::new(RefCell::new(Vec::new()));
    let consumer_calls = Rc::new(RefCell::new(Vec::new()));
    subject.set_consumer(recording_consumer(consumer_calls.clone()));
    subject
        .register(literal("run").executes(recording_command(3, command_calls.clone())))
        .unwrap();
    subject
        .register(
            literal("split")
                .fork(subject.root(), {
                    let original = original.clone();
                    let rejected = rejected.clone();
                    let other = other.clone();
                    Rc::new(move |_| {
                        Ok(vec![
                            Rc::new(original.clone()),
                            Rc::new(rejected.clone()),
                            Rc::new(other.clone()),
                        ])
                    })
                })
                .unwrap(),
        )
        .unwrap();
    let exception = BUILT_IN_EXCEPTIONS.reader_expected_bool().create();
    subject
        .register(
            literal("filter")
                .fork(subject.root(), {
                    let rejected = rejected.clone();
                    Rc::new(move |context| {
                        if context.source().same(&rejected) {
                            Err(exception.clone())
                        } else {
                            Ok(vec![context.shared_source()])
                        }
                    })
                })
                .unwrap(),
        )
        .unwrap();

    assert_eq!(
        subject
            .execute("split filter run", original.clone())
            .unwrap(),
        2
    );
    let executed = command_calls.borrow();
    assert_eq!(executed.len(), 2);
    assert!(executed.iter().any(|source| source.same(&original)));
    assert!(executed.iter().any(|source| source.same(&other)));
    let consumed = consumer_calls.borrow();
    assert_eq!(consumed.len(), 3);
    assert!(
        consumed
            .iter()
            .any(|(source, success, result)| source.same(&rejected) && !success && *result == 0)
    );
    assert!(
        consumed
            .iter()
            .any(|(source, success, result)| source.same(&original) && *success && *result == 3)
    );
    assert!(
        consumed
            .iter()
            .any(|(source, success, result)| source.same(&other) && *success && *result == 3)
    );
}

fn usage_subject() -> CommandDispatcher<Source> {
    let subject = CommandDispatcher::new();
    let command: Command<Source> = Rc::new(|_| Ok(1));
    let denied = Rc::new(|_: &Source| false);
    subject
        .register(
            literal("a")
                .then(
                    literal("1")
                        .then(literal("i").executes(command.clone()))
                        .unwrap()
                        .then(literal("ii").executes(command.clone()))
                        .unwrap(),
                )
                .unwrap()
                .then(
                    literal("2")
                        .then(literal("i").executes(command.clone()))
                        .unwrap()
                        .then(literal("ii").executes(command.clone()))
                        .unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
    subject
        .register(
            literal("b")
                .then(literal("1").executes(command.clone()))
                .unwrap(),
        )
        .unwrap();
    subject
        .register(literal("c").executes(command.clone()))
        .unwrap();
    subject
        .register(
            literal("d")
                .requires(denied.clone())
                .executes(command.clone()),
        )
        .unwrap();
    subject
        .register(
            literal("e")
                .executes(command.clone())
                .then(
                    literal("1")
                        .executes(command.clone())
                        .then(literal("i").executes(command.clone()))
                        .unwrap()
                        .then(literal("ii").executes(command.clone()))
                        .unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
    subject
        .register(
            literal("f")
                .then(
                    literal("1")
                        .then(literal("i").executes(command.clone()))
                        .unwrap()
                        .then(
                            literal("ii")
                                .executes(command.clone())
                                .requires(denied.clone()),
                        )
                        .unwrap(),
                )
                .unwrap()
                .then(
                    literal("2")
                        .then(
                            literal("i")
                                .executes(command.clone())
                                .requires(denied.clone()),
                        )
                        .unwrap()
                        .then(literal("ii").executes(command.clone()))
                        .unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
    subject
        .register(
            literal("g")
                .executes(command.clone())
                .then(
                    literal("1")
                        .then(literal("i").executes(command.clone()))
                        .unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
    subject
        .register(
            literal("h")
                .executes(command.clone())
                .then(
                    literal("1")
                        .then(literal("i").executes(command.clone()))
                        .unwrap(),
                )
                .unwrap()
                .then(
                    literal("2")
                        .then(
                            literal("i")
                                .then(literal("ii").executes(command.clone()))
                                .unwrap(),
                        )
                        .unwrap(),
                )
                .unwrap()
                .then(literal("3").executes(command.clone()))
                .unwrap(),
        )
        .unwrap();
    subject
        .register(
            literal("i")
                .executes(command.clone())
                .then(literal("1").executes(command.clone()))
                .unwrap()
                .then(literal("2").executes(command.clone()))
                .unwrap(),
        )
        .unwrap();
    subject
        .register(literal("j").redirect(subject.root()).unwrap())
        .unwrap();
    let h = node_for(&subject, "h");
    subject.register(literal("k").redirect(h).unwrap()).unwrap();
    subject
}

fn node_for(subject: &CommandDispatcher<Source>, input: &str) -> Node<Source> {
    subject
        .parse(input, Source::new())
        .context()
        .nodes()
        .last()
        .expect("input must parse a node")
        .node()
        .clone()
}

fn named_usage(entries: Vec<(Node<Source>, String)>) -> Vec<(String, String)> {
    entries
        .into_iter()
        .map(|(node, usage)| (node.name(), usage))
        .collect()
}

#[test]
fn command_dispatcher_usages_test_all_usage_no_commands() {
    let subject = CommandDispatcher::<Source>::new();
    assert!(
        subject
            .all_usage(&subject.root(), &Source::new(), true)
            .is_empty()
    );
}

#[test]
fn command_dispatcher_usages_test_smart_usage_no_commands() {
    let subject = CommandDispatcher::<Source>::new();
    assert!(
        subject
            .smart_usage(&subject.root(), &Source::new())
            .is_empty()
    );
}

#[test]
fn command_dispatcher_usages_test_all_usage_root() {
    let subject = usage_subject();
    assert_eq!(
        subject.all_usage(&subject.root(), &Source::new(), true),
        [
            "a 1 i", "a 1 ii", "a 2 i", "a 2 ii", "b 1", "c", "e", "e 1", "e 1 i", "e 1 ii",
            "f 1 i", "f 2 ii", "g", "g 1 i", "h", "h 1 i", "h 2 i ii", "h 3", "i", "i 1", "i 2",
            "j ...", "k -> h",
        ]
    );
}

#[test]
fn command_dispatcher_usages_test_smart_usage_root() {
    let subject = usage_subject();
    assert_eq!(
        named_usage(subject.smart_usage(&subject.root(), &Source::new())),
        [
            ("a".to_owned(), "a (1|2)".to_owned()),
            ("b".to_owned(), "b 1".to_owned()),
            ("c".to_owned(), "c".to_owned()),
            ("e".to_owned(), "e [1]".to_owned()),
            ("f".to_owned(), "f (1|2)".to_owned()),
            ("g".to_owned(), "g [1]".to_owned()),
            ("h".to_owned(), "h [1|2|3]".to_owned()),
            ("i".to_owned(), "i [1|2]".to_owned()),
            ("j".to_owned(), "j ...".to_owned()),
            ("k".to_owned(), "k -> h".to_owned()),
        ]
    );
}

#[test]
fn command_dispatcher_usages_test_smart_usage_h() {
    let subject = usage_subject();
    let h = node_for(&subject, "h");
    assert_eq!(
        named_usage(subject.smart_usage(&h, &Source::new())),
        [
            ("1".to_owned(), "[1] i".to_owned()),
            ("2".to_owned(), "[2] i ii".to_owned()),
            ("3".to_owned(), "[3]".to_owned()),
        ]
    );
}

#[test]
fn command_dispatcher_usages_test_smart_usage_offset_h() {
    let subject = usage_subject();
    let mut reader = StringReader::new("/|/|/h");
    reader.set_cursor(5);
    let h = subject
        .parse_reader(reader, Source::new())
        .context()
        .nodes()
        .last()
        .unwrap()
        .node()
        .clone();
    assert_eq!(
        named_usage(subject.smart_usage(&h, &Source::new())),
        [
            ("1".to_owned(), "[1] i".to_owned()),
            ("2".to_owned(), "[2] i ii".to_owned()),
            ("3".to_owned(), "[3]".to_owned()),
        ]
    );
}
