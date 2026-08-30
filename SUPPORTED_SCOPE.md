# Worldless Supported Scope

This document is the sole project-wide authority for deciding whether a
Minecraft Java Edition data-pack runtime behavior belongs to Worldless's
intended supported scope. It defines the intended boundary, not the set of
features currently implemented. Current availability is defined by the public
API and implementation, not by this document.

## Goal

Worldless is a lightweight, fast Rust VM for executing the computation-only
subset of data packs without loading, generating, or attaching to a physical
Minecraft world. It is not a headless Minecraft server or a physical-world
simulator.

Loading data-pack resources from a directory or receiving them in memory is
input to the VM, not physical-world access.

## Scope boundary

A behavior is in scope only when every observation, state transition, effect,
and result required by its Minecraft semantics can be represented using only:

- static data-pack resources supplied as program input;
- explicit invocation values whose meaning is independent of a world or live
  server identity, except for the configured level seed described below;
- caller-driven logical normal data-pack and scheduling ticks admitted only as
  described below;
- elapsed monotonic time admitted only through the stopwatch clock described
  below; and
- logical computation state owned entirely by the VM and whose meaning and
  transitions do not require a world or live server.

Physical-world state is state whose Minecraft meaning depends on the identity
or current simulation of a level, dimension, or live server population. It
includes chunks and world generation, blocks and block entities, spatial world
properties, entities and players, and live server lifecycle or networking.

This is a semantic boundary, not a storage boundary. Logical state does not
become out of scope merely because Minecraft persists it in a world save or
stores it on a server object. Conversely, serializing or injecting
physical-world state does not make a Minecraft world operation in scope.

## In scope

Examples include:

- data-pack function parsing, resolution, calls, returns, and other control
  flow;
- arithmetic, comparison, and transformation of values held entirely by the
  VM;
- named structured data and score-like computation state that does not require
  resolving an entity or player; and
- conditions and execution-context transformations determined only from
  in-scope resources, state, and inputs.

These examples classify the intended boundary. They do not assert that a
feature is currently implemented.

## Out of scope

Examples include behavior whose specified observations or effects require:

- loading, generating, inspecting, or changing chunks, blocks, block entities,
  fluids, biomes, structures, lighting, or other spatial world state;
- selecting, inspecting, creating, moving, or modifying live entities or
  players;
- interacting with a running server, its physical-world lifecycle, or its
  network connections; or
- producing physical-world effects such as spawning entities or changing
  blocks.

## Mixed behavior

Classify the smallest command form, subcommand, predicate, or resource
operation that has a separable Minecraft contract. A command or resource name
alone does not determine scope. A composed behavior is in scope only when every
observation and effect required for its result is in scope.

For example, an `execute` form that uses only VM-owned score state may be in
scope while a form that queries a block or entity is out of scope. This example
classifies the boundary, not current implementation status.

If one atomic operation combines in-scope and out-of-scope effects, the entire
operation is out of scope. Worldless must not perform only its in-scope portion.

## Caller-supplied inputs

A caller may initialize VM-owned logical state and provide ordinary values to
an in-scope operation. Providing a world snapshot, entity or player handle,
query callback, or precomputed answer does not reclassify a world-dependent
Minecraft operation. A pure operation over the resulting value can be a
separate in-scope operation, but it is not an implementation of the world
query.

## Configured level seed

Every executable Worldless VM is constructed with exactly one explicitly
configured signed 64-bit level seed. This immutable scalar is part of the VM's
execution environment. An operation may observe or use it only when every
other observation and effect required by that operation satisfies this scope
boundary.

The configured seed does not establish a level identity or represent a loaded
or generated physical world. It does not bring world generation, world
queries, blocks, entities, players, dimensions, biomes, structures, or other
physical-world behavior into scope, even when a result could be derived
deterministically from the seed. It also does not seed the unnamed level random
stream. A composed behavior using the seed remains subject to the
mixed-behavior rule above.

## Logical normal data-pack and scheduling ticks

An executable Worldless VM may be advanced by one logical normal data-pack
tick only through an explicit caller call to `Vm::tick`. The initial
`minecraft:load` function-tag lifecycle runs at the beginning of the first such
tick after VM construction, followed by that tick's `minecraft:tick`
function-tag lifecycle. Each subsequent logical normal tick runs the
`minecraft:tick` lifecycle.

Each `Vm::tick` call also advances exactly one VM-owned logical scheduling tick
after the function-tag lifecycle and invokes due function or function-tag
callbacks registered through the `schedule` command. The VM may retain pending
callbacks between calls, and the admitted `schedule` command forms may register
or clear those callbacks. No other action advances the logical scheduling tick
or causes due callbacks to run.

This discrete lifecycle admits only those function-tag lifecycles, the logical
scheduling state needed for those callbacks, and the resulting transitions of
otherwise in-scope VM-owned state. It does not represent elapsed time or a
running server, and it does not introduce the `time` command, general game
time, world clocks, timelines, real-time pacing, tick-rate control, reload
processing, persistence of scheduled callbacks, or any physical-world tick
behavior.

## Stopwatch clock

Elapsed monotonic time is in scope only as required by Minecraft's stopwatch
semantics. The stopwatch clock advances independently of data-pack commands,
logical normal data-pack ticks, and the logical scheduling tick while an
executable VM exists. Calling `Vm::tick` does not advance the stopwatch except
through actual monotonic elapsed time. Stopwatch time does not cause a logical
tick, advance the logical scheduling tick, or cause a scheduled callback to
become due. The stopwatch clock's origin has no meaning and it does not
represent calendar time, a world clock, or a live server.

This narrow exception does not bring world or server lifecycle, game time,
world clocks, timelines, the `time` command, or other time-dependent
physical-world behavior into scope. It does not itself admit `schedule`;
schedule behavior is in scope only as explicitly described for logical ticks
above. A composed behavior that uses a stopwatch remains subject to the
mixed-behavior rule above.

## Unsupported and unimplemented behavior

Being in scope does not imply that a behavior is currently implemented. This
document therefore does not contain a feature or command support matrix.

Worldless must not report successful execution of unsupported or unimplemented
behavior by substituting an empty world, fabricating a default observation, or
discarding a required effect. When such behavior itself is requested,
Worldless must reject it explicitly. The public API and implementation own the
phase and concrete error used for that rejection.

## Version and compatibility

The target Minecraft version is owned by [`worldless.toml`](worldless.toml) and
is not repeated here. Evaluate scope against that version's Minecraft
semantics.

For Java-derived semantics used to implement an in-scope behavior, apply
[`JAVA_COMPATIBILITY.md`](JAVA_COMPATIBILITY.md). Java compatibility does not
bring otherwise out-of-scope Minecraft behavior into scope.

## Changing the scope

A decision that broadens, narrows, or redefines this boundary must update this
document. Implementing another behavior already admitted by the boundary does
not require an update, and implementation status must not be recorded here.
