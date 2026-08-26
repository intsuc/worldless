## Work in turn-sized scope

- Treat the active turn, not a hypothetical version, release, or roadmap, as the
  delivery unit. Extract the current contract from the request and applicable
  repository instructions, then complete implementation, verification, and
  cleanup within that scope.
- Do not add behavior for a possible later need. If the current contract does
  not require it, omit it.
- Resolve design choices in this order:

```text
Is the behavior required by the active turn?
+-- No:  Omit it.
`-- Yes: Does the repository already have an authoritative path for it?
    +-- Yes: Change or reuse that path.
    `-- No:  Do multiple current callers express one rule that must change together?
        +-- Yes: Create the smallest shared owner for that rule.
        `-- No:  Implement it directly and locally.
```

- Add an extension point only when the current contract contains multiple known
  variants. Support exactly those variants; do not add hooks, plugin points,
  factories, reserved fields, unused options, generic interfaces, or
  configuration surfaces for hypothetical cases.
- Prefer a direct implementation over a new layer, framework, dependency, or
  generalized mechanism. Similar-looking code is not duplication unless it
  represents the same knowledge and must change for the same reason.

## Keep one owner for each rule

- Give every behavior rule, value, mapping, and state transition exactly one
  executable owner in implementation code, executable configuration, or schema.
  Reuse or derive from that owner; never maintain manually synchronized copies.
- Keep code as the single source of truth. Never make prose an independent
  authority for code-controlled behavior, defaults, or values.
- Apply this decision before writing or retaining a comment or document:

```text
Would deleting this explanation force a future developer to rediscover a
non-obvious reason, or remove search terms needed to find the owning code?
+-- Yes: Keep the shortest explanation that preserves that advantage.
`-- No:  Delete it.
```

- Delete prose that narrates obvious code, duplicates executable rules, has
  become stale, or no longer shortens development work. Preserve non-obvious
  rationale and navigation, not a parallel specification.

## Enforce narrow, explicit contracts

- Change the current contract directly. Do not add backward compatibility
  through aliases, shims, dual paths, deprecation branches, old-format adapters,
  or version switches.
- Implement only the current contract. Do not add forward compatibility by
  accepting, reserving, preserving, or passing through unknown future shapes,
  values, fields, or versions.
- Never invent an implicit fallback. An explicitly defined contract default is
  ordinary behavior; otherwise, do not silently coerce, ignore, retry,
  substitute, swallow an error, or continue with a guessed value.
- Validate known preconditions at the earliest boundary and before side effects.
  On invalid or unspecified state, stop the affected operation with a specific,
  actionable error.
- Practice virtuous intolerance: expose violations instead of recovering around
  them so their source is fixed before accidental behavior becomes a de facto
  contract. If the touched path already hides such a violation, remove the
  concealment instead of adding another layer around it; do not expand into
  unrelated cleanup.

## Apply the supported-scope contract

- Before implementing, reviewing, or testing Minecraft data-pack runtime
  behavior, read and apply `SUPPORTED_SCOPE.md`; classify the concrete behavior
  there before deciding its implementation or test contract.

## Apply the Java compatibility contract

- Before implementing, reviewing, or testing Java-derived behavior, read and
  apply `JAVA_COMPATIBILITY.md`; classify compatibility requirements there
  instead of inferring them from a reference JDK.

## Verify behavior, not merely tests

1. Derive successful, boundary, and invalid cases from the current contract.
2. Exercise them first with disposable, untracked checks such as one-off
   commands, temporary scripts, or direct calls. Remove these artifacts after
   use.
3. Run the repository's relevant existing checks.
4. If an existing test encodes superseded behavior, update or remove that
   assertion instead of adding compatibility code.
5. Add a version-controlled test only when both conditions hold: the same
   property required checks in multiple edit-verify cycles, and it is a stable
   invariant that future changes must protect. Otherwise, keep the verification
   disposable.

## Apply the completion gate

Before finishing, inspect the diff and require all of the following:

- Every changed line serves the active turn.
- No speculative abstraction, extensibility, compatibility path, or implicit
  fallback remains.
- Each behavior rule has one executable owner.
- No stale prose or disposable verification artifact remains.
- Relevant repository checks passed.

Remove anything that fails this gate, then verify again.
