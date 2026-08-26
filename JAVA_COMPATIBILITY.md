# Java Compatibility Contract

This document is the sole project-wide authority for deciding whether behavior
derived from Java is compatible. Implementation code remains the owner of each
concrete behavior choice.

For Minecraft data-pack runtime behavior, first determine whether the behavior
is in scope under [`SUPPORTED_SCOPE.md`](SUPPORTED_SCOPE.md). This contract also
applies independently to public Java-compatible component crates.

## Target and normative sources

Worldless targets Java SE 25, not a particular JDK vendor, distribution, build,
or host platform. Determine the behavior permitted for a targeted Java
component from all applicable sources:

- the source and published contract of the exact targeted component version;
- the [Java Language Specification, Java SE 25 Edition][jls];
- the [Java Virtual Machine Specification, Java SE 25 Edition][jvms]; and
- the [Java SE 25 API Specification][api].

A standard or data version incorporated by those specifications, including a
specified Unicode version, is normative only to the extent incorporated.
Upstream tests, examples, and executions on a reference JDK are evidence for
interpreting these sources; they do not create an independent compatibility
requirement.

## Required compatibility

Evaluate the complete Java operation against the normative sources, including
the component's use of Java SE APIs:

1. If they permit exactly one Java-observable result, worldless must produce
   that result.
2. If they permit a set of results, worldless may produce any member of that
   set.
3. If they do not constrain an observation, that observation is not a Java
   compatibility requirement.

Java-observable results include specified return values, exceptions, mutations,
ordering, identity relationships, and encodings. Internal algorithms and data
layouts are not observable by themselves. Never promote a repeatable detail of
one JDK, such as an unspecified collection iteration order, identity hash,
allocation pattern, scheduling choice, exception message, or sorting
algorithm, into a requirement without a normative source or an approved
exception below.

## Permitted choices

When Java SE permits multiple results, leaves behavior unspecified or
implementation-dependent, or makes behavior optional, worldless must select a
conforming result deterministically for the same worldless version,
configuration, and Java-observable input and state. Implement only optional
behavior required by the current supported scope; reject unsupported optional
behavior explicitly when rejection is permitted.

The implementation owns the selected result; determinism tests protect it.
Matching a reference JDK is not a reason to reproduce its private algorithm.

## Reference-runtime exceptions

Exact emulation of vendor- or build-specific JDK behavior is required only when
an exception is recorded in this section. Add an exception only when supported
Minecraft or a supported component observably relies on that exact behavior and
no implementation-independent conforming result satisfies it. Each exception
must identify:

- the vendor, distribution, version, and build being matched;
- the affected Minecraft or component versions and observable behavior;
- evidence that the supported software relies on it; and
- the regression test that enforces it.

Do not add an implementation-specific emulation before adding its exception.

There are currently no approved reference-runtime exceptions.

## Invalid preconditions

Compatibility is required for inputs and states admitted by the applicable
component and Java SE contracts. A specified response to an invalid-looking
input, such as a documented exception for an out-of-range argument, remains
required. When a caller violates a true precondition and the normative sources
do not specify the outcome, worldless may reject it early and deterministically;
it need not reproduce a reference JDK's failure, message, partial effects, or
accidental acceptance.

## Verification

Classify tests by what they establish:

- **Conformance tests** cover behavior required by the normative sources and
  are release gates.
- **Upstream component tests** are ported as conformance tests when their
  expectation remains required. If a test is stale for the targeted component
  or Java SE 25, use the current specified behavior. If it fixes one result
  where the specifications permit several, test the permitted invariant rather
  than that result.
- **Reference-JDK differential tests** discover differences. A mismatch is a
  bug only after the contract classifies the expected result as required; exact
  equality is a release gate only for required behavior or an approved
  reference-runtime exception.
- **Determinism tests** protect worldless's selected result where Java permits
  alternatives and the selection is externally observable.

[api]: https://docs.oracle.com/en/java/javase/25/docs/api/index.html
[jls]: https://docs.oracle.com/javase/specs/jls/se25/html/index.html
[jvms]: https://docs.oracle.com/javase/specs/jvms/se25/html/index.html
