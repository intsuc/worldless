use std::{
    collections::HashSet,
    error::Error,
    fmt,
    path::{Path, PathBuf},
    time::Instant,
};

use serde::Serialize;
use worldless::{CompiledProgram, CompoundTag, ExecutionContext, Pack, Position, Rotation};

mod suites;

const POSITION: Position = Position::new(0.0, 0.0, 0.0);
const ROTATION: Rotation = Rotation::new(0.0, 0.0);
const TRANSFORMER_NAMESPACE: &str = "transformer";
const TRANSFORMER_MODEL_STORAGE: &str = "transformer:model";
const TRANSFORMER_REQUEST_STORAGE: &str = "transformer:request";
const TRANSFORMER_RESPONSE_STORAGE: &str = "transformer:response";
const TRANSFORMER_SETUP: &str = "transformer:setup";
const TRANSFORMER_MODEL_ACTIVATE: &str = "transformer:model/activate";

#[derive(Debug, Eq, PartialEq)]
pub struct LabError(String);

impl LabError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn from_message(message: impl Into<String>) -> Self {
        Self::new(message)
    }
}

impl fmt::Display for LabError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for LabError {}

pub(crate) struct Case {
    pub(crate) slug: &'static str,
    pub(crate) input: CompoundTag,
    pub(crate) expected_output: CompoundTag,
}

#[derive(Clone, Copy)]
pub(crate) struct VariantSpec {
    pub(crate) slug: &'static str,
}

pub(crate) struct SuiteSpec {
    pub(crate) slug: &'static str,
    pub(crate) world_seed: i64,
    pub(crate) command_limit: usize,
    pub(crate) variants: &'static [VariantSpec],
    pub(crate) build_cases: fn() -> Result<Vec<Case>, LabError>,
}

#[derive(Debug, Serialize)]
pub struct CheckReport {
    pub execution: ExecutionMode,
    pub suites: Vec<CheckedSuite>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ExecutionMode {
    pub vm_state: &'static str,
    pub macro_cache: &'static str,
}

#[derive(Debug, Serialize)]
pub struct CheckedSuite {
    pub suite: &'static str,
    pub case_count: usize,
    pub variant_count: usize,
    pub invocation_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ComparisonReport {
    pub execution: ExecutionMode,
    pub warmup_discarded: usize,
    pub measured_samples: usize,
    pub rows: Vec<ComparisonRow>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonExecution {
    Fresh,
    Persistent { warmup: usize },
}

#[derive(Debug, Serialize)]
pub struct ComparisonRow {
    pub suite: &'static str,
    pub case: &'static str,
    pub variant: &'static str,
    pub quota: QuotaMeasurement,
    pub timing: TimingMeasurement,
}

#[derive(Debug, Serialize)]
pub struct QuotaMeasurement {
    pub limit: usize,
    pub used: usize,
}

#[derive(Debug, Serialize)]
pub struct TimingMeasurement {
    pub durations_ns: Vec<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BenchmarkEntry {
    Text,
    Tokens,
}

impl BenchmarkEntry {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Tokens => "tokens",
        }
    }

    const fn function(self) -> &'static str {
        match self {
            Self::Text => "transformer:infer/text",
            Self::Tokens => "transformer:infer/tokens",
        }
    }
}

pub struct BenchmarkOptions<'a> {
    pub pack: &'a Path,
    pub model_storage: &'a Path,
    pub entry: BenchmarkEntry,
    pub request: &'a CompoundTag,
    pub warmup: usize,
    pub samples: usize,
    pub command_limit: usize,
}

#[derive(Debug, Serialize)]
pub struct BenchmarkReport {
    pub execution: ExecutionMode,
    pub entry: BenchmarkEntry,
    pub warmup_discarded: usize,
    pub measured_samples: usize,
    pub setup_quota: QuotaMeasurement,
    pub activation_quota: QuotaMeasurement,
    pub quota: QuotaMeasurement,
    pub timing: BenchmarkTimingMeasurement,
    pub response: ResponseVerification,
}

#[derive(Debug, Serialize)]
pub struct BenchmarkTimingMeasurement {
    pub durations_ns: Vec<u64>,
    pub median_ns: f64,
    pub p95_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
}

#[derive(Debug, Serialize)]
pub struct ResponseVerification {
    pub verified_invocations: usize,
    pub identical: bool,
    pub escaped_snbt: String,
}

struct PreparedSuite {
    spec: &'static SuiteSpec,
    cases: Vec<Case>,
    program: CompiledProgram,
}

pub fn check(selected_suite: Option<&str>) -> Result<CheckReport, LabError> {
    let suites = prepare_suites(selected_suite)?;
    check_prepared(&suites)
}

pub fn compare(
    selected_suite: &str,
    execution: ComparisonExecution,
    samples: usize,
) -> Result<ComparisonReport, LabError> {
    if samples == 0 {
        return Err(LabError::new("sample count must be greater than zero"));
    }
    if matches!(execution, ComparisonExecution::Persistent { warmup: 0 }) {
        return Err(LabError::new("warm-up count must be greater than zero"));
    }
    let suites = prepare_suites(Some(selected_suite))?;
    check_prepared(&suites)?;

    let mut rows = Vec::new();
    for suite in &suites {
        for case in &suite.cases {
            for variant in suite.spec.variants {
                let (quota_used, durations_ns) = match execution {
                    ComparisonExecution::Fresh => measure_fresh_row(suite, case, variant, samples)?,
                    ComparisonExecution::Persistent { warmup } => {
                        measure_persistent_row(suite, case, variant, warmup, samples)?
                    }
                };
                rows.push(ComparisonRow {
                    suite: suite.spec.slug,
                    case: case.slug,
                    variant: variant.slug,
                    quota: QuotaMeasurement {
                        limit: suite.spec.command_limit,
                        used: quota_used,
                    },
                    timing: TimingMeasurement { durations_ns },
                });
            }
        }
    }
    Ok(ComparisonReport {
        execution: comparison_execution_mode(execution),
        warmup_discarded: match execution {
            ComparisonExecution::Fresh => 0,
            ComparisonExecution::Persistent { warmup } => warmup,
        },
        measured_samples: samples,
        rows,
    })
}

fn measure_fresh_row(
    suite: &PreparedSuite,
    case: &Case,
    variant: &VariantSpec,
    samples: usize,
) -> Result<(usize, Vec<u64>), LabError> {
    let quota_used = invoke_fresh_and_verify(suite, case, variant, false)?.quota_used;
    let mut durations_ns = Vec::with_capacity(samples);
    for _ in 0..samples {
        let measured = invoke_fresh_and_verify(suite, case, variant, true)?;
        durations_ns.push(
            measured
                .duration_ns
                .expect("timed invocations record their duration"),
        );
    }
    Ok((quota_used, durations_ns))
}

fn measure_persistent_row(
    suite: &PreparedSuite,
    case: &Case,
    variant: &VariantSpec,
    warmup: usize,
    samples: usize,
) -> Result<(usize, Vec<u64>), LabError> {
    let mut vm = suite.program.create_vm(suite.spec.world_seed);
    for _ in 0..warmup {
        invoke_and_verify(&mut vm, suite, case, variant, false)?;
    }

    let mut expected_quota = None;
    let mut durations_ns = Vec::with_capacity(samples);
    for _ in 0..samples {
        let measured = invoke_and_verify(&mut vm, suite, case, variant, true)?;
        if let Some(expected) = expected_quota {
            if measured.quota_used != expected {
                return Err(contextual_error(
                    suite,
                    case,
                    variant,
                    format!(
                        "quota use changed between measured invocations: expected {expected}, actual {}",
                        measured.quota_used
                    ),
                ));
            }
        } else {
            expected_quota = Some(measured.quota_used);
        }
        durations_ns.push(
            measured
                .duration_ns
                .expect("timed invocations record their duration"),
        );
    }
    Ok((
        expected_quota.expect("a positive sample count records quota use"),
        durations_ns,
    ))
}

pub fn benchmark(options: BenchmarkOptions<'_>) -> Result<BenchmarkReport, LabError> {
    if options.samples == 0 {
        return Err(LabError::new("sample count must be greater than zero"));
    }
    if options.command_limit == 0 {
        return Err(LabError::new("command limit must be greater than zero"));
    }
    let verified_invocations = options
        .warmup
        .checked_add(options.samples)
        .ok_or_else(|| LabError::new("warm-up and sample counts overflow usize"))?;
    let mut durations_ns = Vec::new();
    durations_ns
        .try_reserve_exact(options.samples)
        .map_err(|error| {
            LabError::new(format!(
                "cannot allocate timing storage for {} samples: {error}",
                options.samples
            ))
        })?;

    let program =
        CompiledProgram::from_packs([Pack::directory(options.pack)]).map_err(|error| {
            LabError::new(format!(
                "benchmark: failed to load data pack at {}: {error}",
                options.pack.display()
            ))
        })?;
    let mut vm = program.create_vm(0);
    vm.load_command_storage_files([(TRANSFORMER_NAMESPACE, options.model_storage)])
        .map_err(|error| {
            LabError::new(format!(
                "benchmark: failed to load model command storage {} as namespace `{TRANSFORMER_NAMESPACE}`: {error}",
                options.model_storage.display()
            ))
        })?;
    if vm
        .storage(TRANSFORMER_MODEL_STORAGE)
        .expect("the benchmark model storage identifier is valid")
        .is_none()
    {
        return Err(LabError::new(format!(
            "benchmark: command-storage file {} does not contain required storage `{TRANSFORMER_MODEL_STORAGE}`",
            options.model_storage.display()
        )));
    }

    let setup_report = vm.execute_function(
        TRANSFORMER_SETUP,
        None,
        context(),
        options.command_limit,
        drop,
    );
    let setup_quota_used = setup_report.quota_used();
    let setup_outcome = setup_report.into_result().map_err(|error| {
        LabError::new(format!(
            "benchmark: `{TRANSFORMER_SETUP}` failed after using quota {setup_quota_used}: {error}"
        ))
    })?;
    if setup_quota_used == 0 {
        return Err(LabError::new(format!(
            "benchmark: `{TRANSFORMER_SETUP}` executed zero commands; the entry point is missing or empty"
        )));
    }
    if matches!(
        setup_outcome,
        worldless::ExecutionOutcome::Result { success: false, .. }
    ) {
        return Err(LabError::new(format!(
            "benchmark: `{TRANSFORMER_SETUP}` returned failure with outcome {setup_outcome:?}"
        )));
    }

    let activation_report = vm.execute_function(
        TRANSFORMER_MODEL_ACTIVATE,
        None,
        context(),
        options.command_limit,
        drop,
    );
    let activation_quota_used = activation_report.quota_used();
    let activation_outcome = activation_report.into_result().map_err(|error| {
        LabError::new(format!(
            "benchmark: `{TRANSFORMER_MODEL_ACTIVATE}` failed after using quota {activation_quota_used}: {error}"
        ))
    })?;
    let expected_activation_outcome = worldless::ExecutionOutcome::Result {
        success: true,
        value: 1,
    };
    if activation_outcome != expected_activation_outcome {
        return Err(LabError::new(format!(
            "benchmark: unexpected `{TRANSFORMER_MODEL_ACTIVATE}` outcome: expected {expected_activation_outcome:?}, actual {activation_outcome:?}"
        )));
    }

    let mut expected_response = None;
    let mut expected_quota = None;
    for index in 0..options.warmup {
        let measurement = invoke_benchmark(&mut vm, &options, "warm-up", index + 1, false)?;
        verify_benchmark_invocation(
            &mut expected_response,
            &mut expected_quota,
            &measurement,
            "warm-up",
            index + 1,
        )?;
    }

    for index in 0..options.samples {
        let measurement = invoke_benchmark(&mut vm, &options, "sample", index + 1, true)?;
        verify_benchmark_invocation(
            &mut expected_response,
            &mut expected_quota,
            &measurement,
            "sample",
            index + 1,
        )?;
        durations_ns.push(
            measurement
                .duration_ns
                .expect("measured benchmark invocations record their duration"),
        );
    }

    let response = expected_response.expect("a positive sample count records a response");
    let quota_used = expected_quota.expect("a positive sample count records quota use");
    Ok(BenchmarkReport {
        execution: ExecutionMode {
            vm_state: "persistent",
            macro_cache: if options.warmup == 0 {
                "not_pre_warmed"
            } else {
                "warm"
            },
        },
        entry: options.entry,
        warmup_discarded: options.warmup,
        measured_samples: options.samples,
        setup_quota: QuotaMeasurement {
            limit: options.command_limit,
            used: setup_quota_used,
        },
        activation_quota: QuotaMeasurement {
            limit: options.command_limit,
            used: activation_quota_used,
        },
        quota: QuotaMeasurement {
            limit: options.command_limit,
            used: quota_used,
        },
        timing: summarize_benchmark_timings(durations_ns),
        response: ResponseVerification {
            verified_invocations,
            identical: true,
            escaped_snbt: escaped_snbt(&response),
        },
    })
}

struct BenchmarkInvocation {
    quota_used: usize,
    duration_ns: Option<u64>,
    response: CompoundTag,
}

fn invoke_benchmark(
    vm: &mut worldless::Vm,
    options: &BenchmarkOptions<'_>,
    phase: &str,
    index: usize,
    timed: bool,
) -> Result<BenchmarkInvocation, LabError> {
    vm.set_storage(TRANSFORMER_REQUEST_STORAGE, options.request.clone())
        .expect("the benchmark request storage identifier is valid");

    let start = timed.then(Instant::now);
    let report = vm.execute_function(
        options.entry.function(),
        None,
        context(),
        options.command_limit,
        drop,
    );
    let duration_ns = start
        .map(|start| {
            u64::try_from(start.elapsed().as_nanos()).map_err(|_| {
                LabError::new(format!(
                    "benchmark: {phase} {index}: measured duration does not fit in a u64 nanosecond count"
                ))
            })
        })
        .transpose()?;
    let quota_used = report.quota_used();
    let outcome = report.into_result().map_err(|error| {
        LabError::new(format!(
            "benchmark: {phase} {index}: `{}` failed after using quota {quota_used}: {error}",
            options.entry.function()
        ))
    })?;
    let response = vm
        .storage(TRANSFORMER_RESPONSE_STORAGE)
        .expect("the benchmark response storage identifier is valid")
        .cloned()
        .ok_or_else(|| {
            LabError::new(format!(
                "benchmark: {phase} {index}: required response storage `{TRANSFORMER_RESPONSE_STORAGE}` is absent"
            ))
        })?;
    let expected_outcome = worldless::ExecutionOutcome::Result {
        success: true,
        value: 1,
    };
    if outcome != expected_outcome {
        return Err(LabError::new(format!(
            "benchmark: {phase} {index}: unexpected `{}` outcome: expected {expected_outcome:?}, actual {outcome:?}; response `{}`",
            options.entry.function(),
            escaped_snbt(&response)
        )));
    }

    Ok(BenchmarkInvocation {
        quota_used,
        duration_ns,
        response,
    })
}

fn verify_benchmark_invocation(
    expected_response: &mut Option<CompoundTag>,
    expected_quota: &mut Option<usize>,
    actual: &BenchmarkInvocation,
    phase: &str,
    index: usize,
) -> Result<(), LabError> {
    if let Some(expected) = expected_response {
        if expected != &actual.response {
            return Err(LabError::new(format!(
                "benchmark: {phase} {index}: response changed between invocations: expected `{}`, actual `{}`",
                escaped_snbt(expected),
                escaped_snbt(&actual.response)
            )));
        }
    } else {
        *expected_response = Some(actual.response.clone());
    }

    if let Some(expected) = expected_quota {
        if *expected != actual.quota_used {
            return Err(LabError::new(format!(
                "benchmark: {phase} {index}: quota use changed between invocations: expected {expected}, actual {}",
                actual.quota_used
            )));
        }
    } else {
        *expected_quota = Some(actual.quota_used);
    }
    Ok(())
}

fn summarize_benchmark_timings(durations_ns: Vec<u64>) -> BenchmarkTimingMeasurement {
    let mut sorted = durations_ns.clone();
    sorted.sort_unstable();
    let middle = sorted.len() / 2;
    let median_ns = if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] as f64 + sorted[middle] as f64) / 2.0
    } else {
        sorted[middle] as f64
    };
    let p95_index =
        usize::try_from(((u128::try_from(sorted.len()).expect("usize fits u128") * 95) - 1) / 100)
            .expect("nearest-rank index fits usize");
    BenchmarkTimingMeasurement {
        durations_ns,
        median_ns,
        p95_ns: sorted[p95_index],
        min_ns: sorted[0],
        max_ns: sorted[sorted.len() - 1],
    }
}

fn check_prepared(suites: &[PreparedSuite]) -> Result<CheckReport, LabError> {
    let mut checked = Vec::with_capacity(suites.len());
    for suite in suites {
        for case in &suite.cases {
            for variant in suite.spec.variants {
                invoke_fresh_and_verify(suite, case, variant, false)?;
            }
        }
        checked.push(CheckedSuite {
            suite: suite.spec.slug,
            case_count: suite.cases.len(),
            variant_count: suite.spec.variants.len(),
            invocation_count: suite.cases.len() * suite.spec.variants.len(),
        });
    }
    Ok(CheckReport {
        execution: fresh_execution_mode(),
        suites: checked,
    })
}

struct InvocationMeasurement {
    quota_used: usize,
    duration_ns: Option<u64>,
}

fn invoke_and_verify(
    vm: &mut worldless::Vm,
    suite: &PreparedSuite,
    case: &Case,
    variant: &VariantSpec,
    timed: bool,
) -> Result<InvocationMeasurement, LabError> {
    let input_storage = storage_id(suite.spec.slug, "input");
    let output_storage = storage_id(suite.spec.slug, "output");
    let entrypoint = entrypoint(suite.spec.slug, variant.slug);
    vm.set_storage(&input_storage, case.input.clone())
        .map_err(|error| contextual_error(suite, case, variant, format!("set input: {error}")))?;
    vm.set_storage(&output_storage, CompoundTag::default())
        .map_err(|error| {
            contextual_error(suite, case, variant, format!("clear output: {error}"))
        })?;

    let start = timed.then(Instant::now);
    let report = vm.execute_function(&entrypoint, None, context(), suite.spec.command_limit, drop);
    let duration_ns = start
        .map(|start| checked_duration_ns(start.elapsed(), suite, case, variant))
        .transpose()?;
    let quota_used = report.quota_used();
    let outcome = report.into_result().map_err(|error| {
        contextual_error(
            suite,
            case,
            variant,
            format!("execution failed after using quota {quota_used}: {error}"),
        )
    })?;
    let expected_outcome = worldless::ExecutionOutcome::Result {
        success: true,
        value: 1,
    };
    if outcome != expected_outcome {
        return Err(contextual_error(
            suite,
            case,
            variant,
            format!(
                "unexpected invocation outcome: expected {expected_outcome:?}, actual {outcome:?}"
            ),
        ));
    }

    let actual = vm
        .storage(&output_storage)
        .map_err(|error| contextual_error(suite, case, variant, format!("read output: {error}")))?
        .ok_or_else(|| {
            contextual_error(
                suite,
                case,
                variant,
                format!(
                    "required output storage `{output_storage}` is absent; the entrypoint may be missing or may not implement the lab output contract"
                ),
            )
        })?;
    if actual != &case.expected_output {
        return Err(contextual_error(
            suite,
            case,
            variant,
            format!(
                "output mismatch: expected `{}`, actual `{}`",
                escaped_snbt(&case.expected_output),
                escaped_snbt(actual)
            ),
        ));
    }

    Ok(InvocationMeasurement {
        quota_used,
        duration_ns,
    })
}

fn invoke_fresh_and_verify(
    suite: &PreparedSuite,
    case: &Case,
    variant: &VariantSpec,
    timed: bool,
) -> Result<InvocationMeasurement, LabError> {
    let mut vm = suite.program.create_vm(suite.spec.world_seed);
    invoke_and_verify(&mut vm, suite, case, variant, timed)
}

fn checked_duration_ns(
    duration: std::time::Duration,
    suite: &PreparedSuite,
    case: &Case,
    variant: &VariantSpec,
) -> Result<u64, LabError> {
    u64::try_from(duration.as_nanos()).map_err(|_| {
        contextual_error(
            suite,
            case,
            variant,
            "measured duration does not fit in a u64 nanosecond count",
        )
    })
}

fn contextual_error(
    suite: &PreparedSuite,
    case: &Case,
    variant: &VariantSpec,
    reason: impl fmt::Display,
) -> LabError {
    LabError::new(format!(
        "suite `{}`, case `{}`, variant `{}`: {reason}",
        suite.spec.slug, case.slug, variant.slug
    ))
}

fn prepare_suites(selected_suite: Option<&str>) -> Result<Vec<PreparedSuite>, LabError> {
    validate_registry(suites::REGISTRY)?;
    if let Some(selected) = selected_suite
        && !suites::REGISTRY.iter().any(|suite| suite.slug == selected)
    {
        return Err(LabError::new(format!("unknown suite `{selected}`")));
    }

    suites::REGISTRY
        .iter()
        .filter(|suite| selected_suite.is_none_or(|selected| suite.slug == selected))
        .map(|spec| {
            let cases = (spec.build_cases)()?;
            validate_cases(spec, &cases)?;
            let pack_path = pack_path(spec.slug);
            let program =
                CompiledProgram::from_packs([Pack::directory(&pack_path)]).map_err(|error| {
                    LabError::new(format!(
                        "suite `{}`: failed to load pack at {}: {error}",
                        spec.slug,
                        pack_path.display()
                    ))
                })?;
            Ok(PreparedSuite {
                spec,
                cases,
                program,
            })
        })
        .collect()
}

fn validate_registry(registry: &[SuiteSpec]) -> Result<(), LabError> {
    if registry.is_empty() {
        return Err(LabError::new("suite registry must not be empty"));
    }
    let mut suites = HashSet::new();
    for suite in registry {
        validate_slug("suite", suite.slug)?;
        if !suites.insert(suite.slug) {
            return Err(LabError::new(format!("duplicate suite `{}`", suite.slug)));
        }
        if suite.command_limit == 0 {
            return Err(LabError::new(format!(
                "suite `{}` has a zero command limit",
                suite.slug
            )));
        }
        if suite.variants.is_empty() {
            return Err(LabError::new(format!(
                "suite `{}` must declare at least one variant",
                suite.slug
            )));
        }
        let mut variants = HashSet::new();
        for variant in suite.variants {
            validate_slug("variant", variant.slug)?;
            if !variants.insert(variant.slug) {
                return Err(LabError::new(format!(
                    "suite `{}` has duplicate variant `{}`",
                    suite.slug, variant.slug
                )));
            }
        }
    }
    Ok(())
}

fn validate_cases(spec: &SuiteSpec, cases: &[Case]) -> Result<(), LabError> {
    if cases.is_empty() {
        return Err(LabError::new(format!(
            "suite `{}` must declare at least one case",
            spec.slug
        )));
    }
    let mut names = HashSet::new();
    for case in cases {
        validate_slug("case", case.slug)?;
        if !names.insert(case.slug) {
            return Err(LabError::new(format!(
                "suite `{}` has duplicate case `{}`",
                spec.slug, case.slug
            )));
        }
    }
    Ok(())
}

fn validate_slug(kind: &str, slug: &str) -> Result<(), LabError> {
    if slug.is_empty()
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(LabError::new(format!(
            "invalid {kind} slug `{slug}`; expected one or more lowercase ASCII letters, digits, or underscores"
        )));
    }
    Ok(())
}

fn context() -> ExecutionContext {
    ExecutionContext::new(POSITION, ROTATION)
}

fn fresh_execution_mode() -> ExecutionMode {
    ExecutionMode {
        vm_state: "fresh",
        macro_cache: "cold",
    }
}

fn comparison_execution_mode(execution: ComparisonExecution) -> ExecutionMode {
    match execution {
        ComparisonExecution::Fresh => fresh_execution_mode(),
        ComparisonExecution::Persistent { .. } => ExecutionMode {
            vm_state: "persistent",
            macro_cache: "warm",
        },
    }
}

fn escaped_snbt(value: &CompoundTag) -> String {
    let mut escaped = String::new();
    for unit in char::decode_utf16(value.to_compact_snbt_utf16()) {
        match unit {
            Ok(character) => escaped.extend(character.escape_default()),
            Err(error) => escaped.push_str(&format!("\\u{{{:x}}}", error.unpaired_surrogate())),
        }
    }
    escaped
}

fn pack_path(suite: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("packs")
        .join(suite)
}

fn storage_id(suite: &str, role: &str) -> String {
    format!("worldless_lab:{suite}/{role}")
}

fn entrypoint(suite: &str, variant: &str) -> String {
    format!("worldless_lab:{suite}/{variant}/run")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTRACT_VARIANT: VariantSpec = VariantSpec { slug: "variant" };
    const CONTRACT_SPEC: SuiteSpec = SuiteSpec {
        slug: "contract",
        world_seed: 0,
        command_limit: 8,
        variants: &[CONTRACT_VARIANT],
        build_cases: one_case,
    };

    fn one_case() -> Result<Vec<Case>, LabError> {
        Ok(vec![Case {
            slug: "case",
            input: CompoundTag::from_snbt("{}").unwrap(),
            expected_output: CompoundTag::from_snbt("{value:1}").unwrap(),
        }])
    }

    const VARIANT: VariantSpec = VariantSpec { slug: "variant" };

    fn prepared_contract(source: &str) -> PreparedSuite {
        let function = worldless::MemoryResource::new(
            worldless::ResourceKind::Function,
            "worldless_lab:contract/variant/run",
            source,
        );
        PreparedSuite {
            spec: &CONTRACT_SPEC,
            cases: one_case().unwrap(),
            program: CompiledProgram::from_packs([Pack::memory([function])]).unwrap(),
        }
    }

    #[test]
    fn benchmark_timing_uses_conventional_median_and_nearest_rank_p95() {
        let timing = summarize_benchmark_timings((1..=20).rev().collect());
        assert_eq!(timing.median_ns, 10.5);
        assert_eq!(timing.p95_ns, 19);
        assert_eq!(timing.min_ns, 1);
        assert_eq!(timing.max_ns, 20);

        let odd = summarize_benchmark_timings(vec![3, 1, 2]);
        assert_eq!(odd.median_ns, 2.0);
    }

    #[test]
    fn invocation_contract_rejects_missing_mismatched_and_wrongly_returned_outputs() {
        for (source, expected_reason) in [
            ("return 1\n", "required output storage"),
            (
                "data modify storage worldless_lab:contract/output value set value 2\nreturn 1\n",
                "output mismatch",
            ),
            (
                "data modify storage worldless_lab:contract/output value set value 1\nreturn 2\n",
                "unexpected invocation outcome",
            ),
        ] {
            let suite = prepared_contract(source);
            let error =
                match invoke_fresh_and_verify(&suite, &suite.cases[0], &CONTRACT_VARIANT, false) {
                    Ok(_) => panic!("the invalid invocation contract was accepted"),
                    Err(error) => error,
                };
            assert!(
                error.to_string().contains(expected_reason),
                "{error} did not contain {expected_reason:?}"
            );
        }
    }

    #[test]
    fn persistent_measurement_reuses_one_vm_for_warmups_and_samples() {
        let suite = prepared_contract(
            "scoreboard objectives add contract dummy\n\
             scoreboard players add #runs contract 1\n\
             data modify storage worldless_lab:contract/output value set value 1\n\
             execute if score #runs contract matches 3.. run return 0\n\
             return 1\n",
        );
        let error =
            measure_persistent_row(&suite, &suite.cases[0], &CONTRACT_VARIANT, 1, 2).unwrap_err();
        assert!(
            error.to_string().contains("unexpected invocation outcome"),
            "{error}"
        );
    }

    #[test]
    fn persistent_measurement_clears_output_before_every_invocation() {
        let suite = prepared_contract(
            "scoreboard objectives add contract dummy\n\
             scoreboard players add #runs contract 1\n\
             execute if score #runs contract matches 1 run data modify storage worldless_lab:contract/output value set value 1\n\
             return 1\n",
        );
        let error =
            measure_persistent_row(&suite, &suite.cases[0], &CONTRACT_VARIANT, 1, 1).unwrap_err();
        assert!(
            error.to_string().contains("required output storage"),
            "{error}"
        );
    }

    #[test]
    fn comparison_counts_are_validated_before_suite_loading() {
        assert_eq!(
            compare("missing", ComparisonExecution::Fresh, 0).unwrap_err(),
            LabError::new("sample count must be greater than zero")
        );
        assert_eq!(
            compare("missing", ComparisonExecution::Persistent { warmup: 0 }, 1).unwrap_err(),
            LabError::new("warm-up count must be greater than zero")
        );
    }

    #[test]
    fn registry_rejects_duplicate_suites() {
        let registry = [
            SuiteSpec {
                slug: "same",
                world_seed: 0,
                command_limit: 1,
                variants: &[VARIANT],
                build_cases: one_case,
            },
            SuiteSpec {
                slug: "same",
                world_seed: 0,
                command_limit: 1,
                variants: &[VARIANT],
                build_cases: one_case,
            },
        ];
        assert_eq!(
            validate_registry(&registry).unwrap_err(),
            LabError::new("duplicate suite `same`")
        );
    }

    #[test]
    fn registry_rejects_invalid_and_empty_declarations() {
        for (slug, reason) in [
            ("Bad", "invalid suite slug"),
            ("has-hyphen", "invalid suite slug"),
            ("", "invalid suite slug"),
        ] {
            let registry = [SuiteSpec {
                slug,
                world_seed: 0,
                command_limit: 1,
                variants: &[VARIANT],
                build_cases: one_case,
            }];
            assert!(
                validate_registry(&registry)
                    .unwrap_err()
                    .to_string()
                    .contains(reason)
            );
        }

        let no_variants = [SuiteSpec {
            slug: "suite",
            world_seed: 0,
            command_limit: 1,
            variants: &[],
            build_cases: one_case,
        }];
        assert!(
            validate_registry(&no_variants)
                .unwrap_err()
                .to_string()
                .contains("at least one variant")
        );
    }

    #[test]
    fn registry_rejects_zero_limits_and_duplicate_variants() {
        let zero_limit = [SuiteSpec {
            slug: "suite",
            world_seed: 0,
            command_limit: 0,
            variants: &[VARIANT],
            build_cases: one_case,
        }];
        assert!(
            validate_registry(&zero_limit)
                .unwrap_err()
                .to_string()
                .contains("zero command limit")
        );

        let duplicate_variants = [SuiteSpec {
            slug: "suite",
            world_seed: 0,
            command_limit: 1,
            variants: &[VARIANT, VARIANT],
            build_cases: one_case,
        }];
        assert!(
            validate_registry(&duplicate_variants)
                .unwrap_err()
                .to_string()
                .contains("duplicate variant `variant`")
        );
    }

    #[test]
    fn cases_must_be_present_and_unique() {
        let spec = SuiteSpec {
            slug: "suite",
            world_seed: 0,
            command_limit: 1,
            variants: &[VARIANT],
            build_cases: one_case,
        };
        assert!(
            validate_cases(&spec, &[])
                .unwrap_err()
                .to_string()
                .contains("at least one case")
        );

        let duplicate = one_case().unwrap().pop().unwrap();
        let cases = [
            Case {
                slug: duplicate.slug,
                input: duplicate.input.clone(),
                expected_output: duplicate.expected_output.clone(),
            },
            duplicate,
        ];
        assert!(
            validate_cases(&spec, &cases)
                .unwrap_err()
                .to_string()
                .contains("duplicate case `case`")
        );
    }
}
