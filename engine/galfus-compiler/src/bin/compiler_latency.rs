use galfus_compiler::{CompiledModule, CompilerState, compile_changed_modules, compile_modules};
use galfus_contract::CapabilityCatalog;
use galfus_core::{ModuleId, ModulePath, Revision, SourceFile, SourceId};
use galfus_frontend::modules::{
    FrontendModuleKind, FrontendRoots, FrontendSession, FrontendSource, FrontendUpdate,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CHAIN_MODULE_COUNT: usize = 32;
const SAMPLE_COUNT: usize = 7;

struct BenchmarkResult {
    name: &'static str,
    samples: Vec<Duration>,
    changed_modules: usize,
}

fn main() {
    let benchmarks = [
        BenchmarkResult {
            name: "full workspace",
            samples: samples(run_full_workspace),
            changed_modules: CHAIN_MODULE_COUNT + 1,
        },
        BenchmarkResult {
            name: "leaf edit with dependents",
            samples: samples(run_leaf_edit),
            changed_modules: CHAIN_MODULE_COUNT,
        },
        BenchmarkResult {
            name: "isolated module edit",
            samples: samples(run_isolated_edit),
            changed_modules: 1,
        },
        BenchmarkResult {
            name: "unchanged workspace",
            samples: samples(run_unchanged_workspace),
            changed_modules: 0,
        },
    ];

    println!(
        "Compiler latency benchmark ({SAMPLE_COUNT} samples, {CHAIN_MODULE_COUNT}-module chain)"
    );
    println!("scenario                         changed   median      mean");
    for benchmark in benchmarks {
        println!(
            "{:<32} {:>7} {:>8.3} ms {:>8.3} ms",
            benchmark.name,
            benchmark.changed_modules,
            median_ms(&benchmark.samples),
            mean_ms(&benchmark.samples),
        );
    }
}

fn samples(run: fn() -> Duration) -> Vec<Duration> {
    (0..SAMPLE_COUNT).map(|_| run()).collect()
}

fn run_full_workspace() -> Duration {
    let sources = workspace_sources(1, 1);
    let mut frontend = FrontendSession::new();
    let started = Instant::now();
    let report = check(&mut frontend, &sources, 1, &[]);
    assert_valid(&report);
    assert_changed_modules(&report, CHAIN_MODULE_COUNT + 1);
    let mut modules = compiled_modules(&frontend);
    compile_modules(
        &mut modules,
        &mut CompilerState::default(),
        frontend.string_table(),
    )
    .expect("full workspace compilation must succeed");
    started.elapsed()
}

fn run_leaf_edit() -> Duration {
    let sources = workspace_sources(1, 1);
    let mut frontend = FrontendSession::new();
    let initial = check(&mut frontend, &sources, 1, &[]);
    assert_valid(&initial);
    let mut modules = compiled_modules(&frontend);
    let mut state = CompilerState::default();
    compile_modules(&mut modules, &mut state, frontend.string_table())
        .expect("initial workspace compilation must succeed");

    let updated = module_source(0, 2, 2);
    let started = Instant::now();
    let report = check(&mut frontend, std::slice::from_ref(&updated), 2, &[]);
    assert_valid(&report);
    assert_changed_modules(&report, CHAIN_MODULE_COUNT);
    let mut modules = compiled_modules(&frontend);
    compile_changed_modules(
        &mut modules,
        &mut state,
        &report.changed_modules,
        frontend.string_table(),
    )
    .expect("incremental leaf compilation must succeed");
    started.elapsed()
}

fn run_isolated_edit() -> Duration {
    let sources = workspace_sources(1, 1);
    let mut frontend = FrontendSession::new();
    let initial = check(&mut frontend, &sources, 1, &[]);
    assert_valid(&initial);
    let mut modules = compiled_modules(&frontend);
    let mut state = CompilerState::default();
    compile_modules(&mut modules, &mut state, frontend.string_table())
        .expect("initial workspace compilation must succeed");

    let updated = isolated_source(2, 2);
    let started = Instant::now();
    let report = check(&mut frontend, std::slice::from_ref(&updated), 2, &[]);
    assert_valid(&report);
    assert_changed_modules(&report, 1);
    let mut modules = compiled_modules(&frontend);
    compile_changed_modules(
        &mut modules,
        &mut state,
        &report.changed_modules,
        frontend.string_table(),
    )
    .expect("incremental isolated compilation must succeed");
    started.elapsed()
}

fn run_unchanged_workspace() -> Duration {
    let sources = workspace_sources(1, 1);
    let mut frontend = FrontendSession::new();
    let initial = check(&mut frontend, &sources, 1, &[]);
    assert_valid(&initial);
    let mut modules = compiled_modules(&frontend);
    let mut state = CompilerState::default();
    compile_modules(&mut modules, &mut state, frontend.string_table())
        .expect("initial workspace compilation must succeed");

    let started = Instant::now();
    let report = check(&mut frontend, &[], 2, &[]);
    assert_valid(&report);
    assert_changed_modules(&report, 0);
    let mut modules = compiled_modules(&frontend);
    compile_changed_modules(
        &mut modules,
        &mut state,
        &report.changed_modules,
        frontend.string_table(),
    )
    .expect("unchanged workspace compilation must succeed");
    started.elapsed()
}

fn check<'a>(
    frontend: &mut FrontendSession,
    sources: &'a [SourceFile],
    revision: u64,
    removed_modules: &'a [ModuleId],
) -> galfus_frontend::modules::FrontendReport {
    let sources = sources
        .iter()
        .map(|source| FrontendSource {
            module_id: ModuleId::new(source.id().raw()),
            path: module_path(source.name()),
            source,
            kind: FrontendModuleKind::Standard,
        })
        .collect::<Vec<_>>();
    frontend.check(FrontendUpdate {
        source_revision: Revision::new(revision),
        sources: &sources,
        removed_modules,
        roots: &FrontendRoots::default(),
        catalog: Arc::new(CapabilityCatalog::default()),
    })
}

fn compiled_modules(frontend: &FrontendSession) -> Vec<CompiledModule> {
    frontend
        .modules()
        .iter()
        .map(|module| {
            CompiledModule::new(
                module.id(),
                module.path().clone(),
                module.semantic_revision(),
                module.source().clone(),
                module.graph().clone(),
                module.type_result().cloned(),
                false,
            )
        })
        .collect()
}

fn workspace_sources(revision: u64, leaf_value: i32) -> Vec<SourceFile> {
    let mut sources = (0..CHAIN_MODULE_COUNT)
        .map(|index| module_source(index, revision, leaf_value))
        .collect::<Vec<_>>();
    sources.push(isolated_source(revision, leaf_value));
    sources
}

fn module_source(index: usize, revision: u64, leaf_value: i32) -> SourceFile {
    let text = if index == 0 {
        format!("export fn value_0(): i32 {{ return {leaf_value} }} // revision {revision}")
    } else {
        let previous = index - 1;
        format!(
            "import {{ value_{previous} }} from './module_{previous}'\nexport fn value_{index}(): i32 {{ return value_{previous}() + 1 }} // revision {revision}"
        )
    };
    SourceFile::new(
        SourceId::new((index + 1) as u32),
        format!("src/module_{index}.gfs"),
        text,
    )
}

fn isolated_source(revision: u64, value: i32) -> SourceFile {
    SourceFile::new(
        SourceId::new((CHAIN_MODULE_COUNT + 1) as u32),
        "src/isolated.gfs".to_string(),
        format!("export fn isolated(): i32 {{ return {value} }} // revision {revision}"),
    )
}

fn module_path(name: &str) -> ModulePath {
    ModulePath::new(name).expect("benchmark source path must be valid")
}

fn assert_valid(report: &galfus_frontend::modules::FrontendReport) {
    assert!(
        !report.diagnostics.has_errors(),
        "benchmark workspace must be valid: {:?}",
        report.diagnostics
    );
}

fn assert_changed_modules(report: &galfus_frontend::modules::FrontendReport, expected: usize) {
    assert_eq!(
        report.changed_modules.len(),
        expected,
        "benchmark scenario changed an unexpected set of modules"
    );
}

fn median_ms(samples: &[Duration]) -> f64 {
    let mut samples = samples
        .iter()
        .map(Duration::as_secs_f64)
        .collect::<Vec<_>>();
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2] * 1_000.0
}

fn mean_ms(samples: &[Duration]) -> f64 {
    samples.iter().map(Duration::as_secs_f64).sum::<f64>() / samples.len() as f64 * 1_000.0
}
