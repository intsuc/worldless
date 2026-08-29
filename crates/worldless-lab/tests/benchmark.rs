use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use worldless::CompoundTag;
use worldless_lab::{BenchmarkEntry, BenchmarkOptions};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct BenchmarkFixture {
    root: PathBuf,
    pack: PathBuf,
    model_storage: PathBuf,
}

impl BenchmarkFixture {
    fn new(changing_response: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "worldless-lab-benchmark-test-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        let pack = root.join("pack");
        let functions = pack.join("data/transformer/function");
        fs::create_dir_all(functions.join("infer")).unwrap();
        fs::create_dir_all(functions.join("model")).unwrap();
        fs::write(
            pack.join("pack.mcmeta"),
            r#"{"pack":{"description":"benchmark test","min_format":[118,0],"max_format":[118,0]}}"#,
        )
        .unwrap();
        fs::write(
            functions.join("setup.mcfunction"),
            "scoreboard objectives add benchmark dummy\nscoreboard players set #count benchmark 0\ndata modify storage transformer:state setup set value 1b\n",
        )
        .unwrap();
        fs::write(
            functions.join("model/activate.mcfunction"),
            "execute unless data storage transformer:model {marker:7} run return fail\ndata modify storage transformer:state active set value 1b\nreturn 1\n",
        )
        .unwrap();
        let inference = if changing_response {
            "execute unless data storage transformer:state {setup:1b,active:1b} run return fail\nscoreboard players add #count benchmark 1\nexecute store result storage transformer:response count int 1 run scoreboard players get #count benchmark\nreturn 1\n"
        } else {
            "execute unless data storage transformer:state {setup:1b,active:1b} run return fail\ndata modify storage transformer:response request set from storage transformer:request\nreturn 1\n"
        };
        fs::write(functions.join("infer/text.mcfunction"), inference).unwrap();

        let model_storage = root.join("model.dat");
        fs::write(&model_storage, model_storage_file()).unwrap();
        Self {
            root,
            pack,
            model_storage,
        }
    }
}

impl Drop for BenchmarkFixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!("failed to remove {}: {error}", self.root.display());
        }
    }
}

fn push_modified_utf8(output: &mut Vec<u8>, value: &str) {
    output.extend_from_slice(&u16::try_from(value.len()).unwrap().to_be_bytes());
    output.extend_from_slice(value.as_bytes());
}

fn push_named_header(output: &mut Vec<u8>, tag_type: u8, name: &str) {
    output.push(tag_type);
    push_modified_utf8(output, name);
}

fn model_storage_file() -> Vec<u8> {
    let mut output = Vec::new();
    push_named_header(&mut output, 10, "");
    push_named_header(&mut output, 3, "DataVersion");
    output.extend_from_slice(&5015_i32.to_be_bytes());
    push_named_header(&mut output, 10, "data");
    push_named_header(&mut output, 10, "contents");
    push_named_header(&mut output, 10, "model");
    push_named_header(&mut output, 3, "marker");
    output.extend_from_slice(&7_i32.to_be_bytes());
    output.extend_from_slice(&[0, 0, 0, 0]);
    output
}

fn options<'a>(
    fixture: &'a BenchmarkFixture,
    request: &'a CompoundTag,
    warmup: usize,
    samples: usize,
) -> BenchmarkOptions<'a> {
    BenchmarkOptions {
        pack: &fixture.pack,
        model_storage: &fixture.model_storage,
        entry: BenchmarkEntry::Text,
        request,
        warmup,
        samples,
        command_limit: 64,
    }
}

#[test]
fn production_benchmark_reuses_one_vm_and_reports_stable_invocations() {
    let fixture = BenchmarkFixture::new(false);
    let request = CompoundTag::from_snbt(r#"{prefix:"Once",max_new_tokens:1}"#).unwrap();

    let report = worldless_lab::benchmark(options(&fixture, &request, 2, 3)).unwrap();

    assert_eq!(report.execution.vm_state, "persistent");
    assert_eq!(report.execution.macro_cache, "warm");
    assert_eq!(report.entry, BenchmarkEntry::Text);
    assert_eq!(report.warmup_discarded, 2);
    assert_eq!(report.measured_samples, 3);
    assert_eq!(report.response.verified_invocations, 5);
    assert!(report.response.identical);
    assert!(report.response.escaped_snbt.contains("Once"));
    assert_eq!(report.timing.durations_ns.len(), 3);
    assert!(report.timing.min_ns as f64 <= report.timing.median_ns);
    assert!(report.timing.median_ns <= report.timing.max_ns as f64);
    assert!(report.timing.p95_ns <= report.timing.max_ns);
}

#[test]
fn production_benchmark_rejects_a_response_change() {
    let fixture = BenchmarkFixture::new(true);
    let request = CompoundTag::from_snbt(r#"{prefix:"Once",max_new_tokens:1}"#).unwrap();

    let error = worldless_lab::benchmark(options(&fixture, &request, 1, 1)).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("response changed between invocations"),
        "{error}"
    );
}

#[test]
fn production_benchmark_requires_a_model_storage_entry() {
    let fixture = BenchmarkFixture::new(false);
    let missing = fixture.root.join("missing-model.dat");
    fs::write(&missing, empty_storage_file()).unwrap();
    let request = CompoundTag::from_snbt("{}").unwrap();
    let mut options = options(&fixture, &request, 0, 1);
    options.model_storage = &missing;

    let error = worldless_lab::benchmark(options).unwrap_err();

    assert!(error.to_string().contains("transformer:model"), "{error}");
}

#[test]
fn production_benchmark_requires_successful_model_activation() {
    let fixture = BenchmarkFixture::new(false);
    fs::write(
        fixture
            .pack
            .join("data/transformer/function/model/activate.mcfunction"),
        "return fail\n",
    )
    .unwrap();
    let request = CompoundTag::from_snbt("{}").unwrap();

    let error = worldless_lab::benchmark(options(&fixture, &request, 0, 1)).unwrap_err();

    assert!(
        error.to_string().contains("transformer:model/activate"),
        "{error}"
    );
}

#[cfg(not(debug_assertions))]
#[test]
fn release_cli_runs_the_production_benchmark_path() {
    let fixture = BenchmarkFixture::new(false);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_worldless-lab"))
        .args([
            "benchmark",
            "--pack",
            fixture.pack.to_str().unwrap(),
            "--model-storage",
            fixture.model_storage.to_str().unwrap(),
            "--entry",
            "text",
            "--request",
            r#"{prefix:"Once",max_new_tokens:1}"#,
            "--warmup",
            "1",
            "--samples",
            "2",
            "--quota",
            "64",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["execution"]["vm_state"], "persistent");
    assert_eq!(report["entry"], "text");
    assert_eq!(report["warmup_discarded"], 1);
    assert_eq!(report["measured_samples"], 2);
    assert_eq!(report["response"]["verified_invocations"], 3);
    assert_eq!(report["response"]["identical"], true);
}

fn empty_storage_file() -> Vec<u8> {
    let mut output = Vec::new();
    push_named_header(&mut output, 10, "");
    push_named_header(&mut output, 3, "DataVersion");
    output.extend_from_slice(&5015_i32.to_be_bytes());
    push_named_header(&mut output, 10, "data");
    push_named_header(&mut output, 10, "contents");
    output.extend_from_slice(&[0, 0, 0]);
    output
}
