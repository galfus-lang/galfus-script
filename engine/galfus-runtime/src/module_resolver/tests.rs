use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use galfus_bytecode::{
    BytecodeGraph, BytecodeModule, BytecodeNode, ConstantPool, ModuleCatalog, ModuleDependency,
    ModuleDescriptor, ModuleResolveContext, ModuleResolveError, derive_module_catalog,
};
use galfus_contract::ContentHash;
use galfus_core::{ModuleId, ModulePath, SemanticRevision};

use super::{GraphModuleProducer, ModuleInitializationPlanError, ModuleProducer, ModuleResolver};

struct StaticProducer {
    result: Result<Arc<BytecodeNode>, ModuleResolveError>,
    calls: AtomicUsize,
}

impl StaticProducer {
    fn new(result: Result<Arc<BytecodeNode>, ModuleResolveError>) -> Self {
        Self {
            result,
            calls: AtomicUsize::new(0),
        }
    }
}

impl ModuleProducer for StaticProducer {
    fn produce(&self, _module_id: ModuleId) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.result.clone()
    }
}

struct BlockingProducer {
    node: Arc<BytecodeNode>,
    calls: AtomicUsize,
    started: (Mutex<bool>, Condvar),
    released: (Mutex<bool>, Condvar),
}

impl BlockingProducer {
    fn new(node: Arc<BytecodeNode>) -> Self {
        Self {
            node,
            calls: AtomicUsize::new(0),
            started: (Mutex::new(false), Condvar::new()),
            released: (Mutex::new(false), Condvar::new()),
        }
    }

    fn wait_until_started(&self) {
        let mut started = self
            .started
            .0
            .lock()
            .expect("producer start state is available");
        while !*started {
            started = self
                .started
                .1
                .wait(started)
                .expect("producer start state is available");
        }
    }

    fn release(&self) {
        let mut released = self
            .released
            .0
            .lock()
            .expect("producer release state is available");
        *released = true;
        self.released.1.notify_all();
    }
}

impl ModuleProducer for BlockingProducer {
    fn produce(&self, _module_id: ModuleId) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut started = self
            .started
            .0
            .lock()
            .expect("producer start state is available");
        *started = true;
        self.started.1.notify_all();
        drop(started);

        let mut released = self
            .released
            .0
            .lock()
            .expect("producer release state is available");
        while !*released {
            released = self
                .released
                .1
                .wait(released)
                .expect("producer release state is available");
        }
        Ok(self.node.clone())
    }
}

struct RecordingProducer {
    nodes: HashMap<ModuleId, Arc<BytecodeNode>>,
    requests: Mutex<Vec<ModuleId>>,
}

impl RecordingProducer {
    fn new(nodes: impl IntoIterator<Item = Arc<BytecodeNode>>) -> Self {
        Self {
            nodes: nodes.into_iter().map(|node| (node.id(), node)).collect(),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<ModuleId> {
        self.requests
            .lock()
            .expect("request list is available")
            .clone()
    }
}

impl ModuleProducer for RecordingProducer {
    fn produce(&self, module_id: ModuleId) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        self.requests
            .lock()
            .expect("request list is available")
            .push(module_id);
        self.nodes
            .get(&module_id)
            .cloned()
            .ok_or_else(|| producer_error(module_id))
    }
}

fn catalog(module_ids: &[ModuleId]) -> ModuleCatalog {
    ModuleCatalog::new(
        module_ids
            .iter()
            .copied()
            .map(|module_id| {
                ModuleDescriptor::new(
                    module_id,
                    module_path(module_id),
                    Vec::new(),
                    Vec::new(),
                    false,
                    ContentHash::of(b"interface"),
                    ContentHash::of(b"chunk"),
                )
                .expect("valid descriptor")
            })
            .collect(),
    )
    .expect("valid catalog")
}

fn module_path(module_id: ModuleId) -> ModulePath {
    let path = format!("src/{}.gfs", module_id.raw());
    ModulePath::new(path.as_str()).expect("valid module path")
}

fn node(module_id: ModuleId) -> Arc<BytecodeNode> {
    Arc::new(BytecodeNode {
        id: module_id,
        path: module_path(module_id),
        semantic_revision: SemanticRevision::new(0),
        module: BytecodeModule {
            name: format!("{}.gfs", module_id.raw()),
            global_count: 0,
            constants: ConstantPool::default(),
            functions: Vec::new(),
            types: Vec::new(),
            struct_layouts: Vec::new(),
            choice_layouts: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            init_func_idx: None,
        },
        metadata: None,
    })
}

fn graph(nodes: impl IntoIterator<Item = Arc<BytecodeNode>>) -> Arc<BytecodeGraph> {
    Arc::new(
        BytecodeGraph::from_modules(
            SemanticRevision::new(0),
            nodes.into_iter().map(|node| (*node).clone()).collect(),
            Vec::new(),
        )
        .expect("valid graph"),
    )
}

fn catalog_from_graph(graph: &BytecodeGraph) -> Arc<ModuleCatalog> {
    Arc::new(derive_module_catalog(graph).expect("catalog derives from graph"))
}

fn producer_error(module_id: ModuleId) -> ModuleResolveError {
    ModuleResolveError::ProducerFailed {
        context: ModuleResolveContext::new(module_id),
    }
}

fn initialization_resolver(descriptors: Vec<(ModuleId, Vec<ModuleId>, bool)>) -> ModuleResolver {
    let catalog = ModuleCatalog::new(
        descriptors
            .into_iter()
            .map(|(module_id, dependencies, has_initializer)| {
                ModuleDescriptor::new(
                    module_id,
                    module_path(module_id),
                    dependencies
                        .into_iter()
                        .map(ModuleDependency::new)
                        .collect(),
                    Vec::new(),
                    has_initializer,
                    ContentHash::of(b"interface"),
                    ContentHash::of(b"chunk"),
                )
                .expect("valid initialization descriptor")
            })
            .collect(),
    )
    .expect("valid initialization catalog");
    let nodes = catalog
        .iter()
        .map(|descriptor| node(descriptor.module_id()));
    ModuleResolver::new(&catalog, Arc::new(RecordingProducer::new(nodes)))
}

#[test]
fn ensure_module_produces_a_catalog_module_once() {
    let module_id = ModuleId::new(7);
    let expected = node(module_id);
    let producer = Arc::new(StaticProducer::new(Ok(expected.clone())));
    let resolver = ModuleResolver::new(&catalog(&[module_id]), producer.clone());

    let first = resolver.ensure_module(module_id).expect("module resolves");
    let second = resolver
        .ensure_module(module_id)
        .expect("module stays ready");

    assert!(Arc::ptr_eq(&first, &expected));
    assert!(Arc::ptr_eq(&second, &expected));
    assert_eq!(producer.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn ensure_module_rejects_a_catalog_absent_id_without_producing() {
    let declared_id = ModuleId::new(7);
    let missing_id = ModuleId::new(9);
    let producer = Arc::new(StaticProducer::new(Ok(node(declared_id))));
    let resolver = ModuleResolver::new(&catalog(&[declared_id]), producer.clone());

    let error = resolver
        .ensure_module(missing_id)
        .expect_err("absent module is rejected");

    assert_eq!(
        error,
        ModuleResolveError::UnknownModule {
            context: ModuleResolveContext::new(missing_id),
        }
    );
    assert_eq!(producer.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn concurrent_ensure_module_calls_share_one_production() {
    let module_id = ModuleId::new(7);
    let expected = node(module_id);
    let producer = Arc::new(BlockingProducer::new(expected.clone()));
    let resolver = Arc::new(ModuleResolver::new(
        &catalog(&[module_id]),
        producer.clone(),
    ));

    let first_resolver = resolver.clone();
    let first = thread::spawn(move || first_resolver.ensure_module(module_id));
    producer.wait_until_started();

    let waiters = (0..4)
        .map(|_| {
            let resolver = resolver.clone();
            thread::spawn(move || resolver.ensure_module(module_id))
        })
        .collect::<Vec<_>>();
    producer.release();

    let first = first
        .join()
        .expect("producer thread finishes")
        .expect("module resolves");
    assert!(Arc::ptr_eq(&first, &expected));
    for waiter in waiters {
        let resolved = waiter
            .join()
            .expect("waiter thread finishes")
            .expect("module resolves");
        assert!(Arc::ptr_eq(&resolved, &expected));
    }
    assert_eq!(producer.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_production_is_cached() {
    let module_id = ModuleId::new(7);
    let expected = producer_error(module_id);
    let producer = Arc::new(StaticProducer::new(Err(expected.clone())));
    let resolver = ModuleResolver::new(&catalog(&[module_id]), producer.clone());

    assert_eq!(resolver.ensure_module(module_id), Err(expected.clone()));
    assert_eq!(resolver.ensure_module(module_id), Err(expected));
    assert_eq!(producer.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn preload_all_requests_catalog_modules_in_canonical_order() {
    let module_ids = [ModuleId::new(19), ModuleId::new(3), ModuleId::new(11)];
    let nodes = module_ids.into_iter().map(node).collect::<Vec<_>>();
    let producer = Arc::new(RecordingProducer::new(nodes));
    let resolver = ModuleResolver::new(&catalog(&module_ids), producer.clone());

    resolver.preload_all().expect("all modules preload");

    assert_eq!(
        producer.requests(),
        vec![ModuleId::new(3), ModuleId::new(11), ModuleId::new(19)]
    );
}

#[test]
fn graph_producer_preloads_every_graph_module() {
    let module_ids = [ModuleId::new(19), ModuleId::new(3), ModuleId::new(11)];
    let graph = graph(module_ids.into_iter().map(node));
    let catalog = catalog_from_graph(graph.as_ref());
    let producer = Arc::new(
        GraphModuleProducer::new(graph.clone(), catalog.clone()).expect("graph catalog derives"),
    );
    let resolver = ModuleResolver::new(catalog.as_ref(), producer);

    resolver.preload_all().expect("all graph modules preload");

    for module_id in module_ids {
        let resolved = resolver
            .ensure_module(module_id)
            .expect("module stays ready");
        let graph_node = graph.node_handle(module_id).expect("graph contains module");
        assert!(Arc::ptr_eq(&resolved, &graph_node));
    }
}

#[test]
fn graph_producer_rejects_a_catalog_interface_mismatch() {
    let module_id = ModuleId::new(7);
    let graph = graph([node(module_id)]);
    let graph_catalog = catalog_from_graph(graph.as_ref());
    let expected_hash = ContentHash::of(b"different interface");
    let declared_catalog = Arc::new(
        ModuleCatalog::new(vec![
            ModuleDescriptor::new(
                module_id,
                ModulePath::new("src/other.gfs").expect("valid module path"),
                Vec::new(),
                Vec::new(),
                false,
                expected_hash,
                ContentHash::of(b"chunk"),
            )
            .expect("valid descriptor"),
        ])
        .expect("valid catalog"),
    );
    let producer = Arc::new(
        GraphModuleProducer::new(graph, declared_catalog.clone()).expect("graph catalog derives"),
    );
    let resolver = ModuleResolver::new(declared_catalog.as_ref(), producer);

    let error = resolver
        .ensure_module(module_id)
        .expect_err("mismatched graph interface is rejected");

    assert!(matches!(
        error,
        ModuleResolveError::InterfaceMismatch {
            context,
            expected,
            actual,
        } if context.module_id() == module_id
            && context.module_path().map(ModulePath::as_str) == Some("src/other.gfs")
            && expected == expected_hash
            && actual == graph_catalog
                .get(module_id)
                .expect("graph catalog contains module")
                .interface_hash()
    ));
}

#[test]
fn graph_producer_reports_a_missing_eager_node_without_panicking() {
    let module_id = ModuleId::new(7);
    let declared_catalog = Arc::new(catalog(&[module_id]));
    let producer = Arc::new(
        GraphModuleProducer::new(Arc::new(BytecodeGraph::new()), declared_catalog.clone())
            .expect("empty graph catalog derives"),
    );
    let resolver = ModuleResolver::new(declared_catalog.as_ref(), producer);

    let error = resolver
        .ensure_module(module_id)
        .expect_err("missing graph node is unavailable");

    assert_eq!(
        error,
        ModuleResolveError::UnavailableInEager {
            context: ModuleResolveContext::with_path(module_id, module_path(module_id)),
        }
    );
}

#[test]
fn initialization_plan_orders_a_chain_dependency_first() {
    let leaf = ModuleId::new(3);
    let middle = ModuleId::new(19);
    let entry = ModuleId::new(41);
    let resolver = initialization_resolver(vec![
        (entry, vec![middle], true),
        (leaf, Vec::new(), true),
        (middle, vec![leaf], true),
    ]);

    assert_eq!(
        resolver.initialization_plan(entry),
        Ok(vec![leaf, middle, entry])
    );
}

#[test]
fn initialization_plan_orders_a_diamond_independent_of_descriptor_insertion() {
    let root = ModuleId::new(5);
    let left = ModuleId::new(11);
    let right = ModuleId::new(23);
    let entry = ModuleId::new(47);
    let resolver = initialization_resolver(vec![
        (entry, vec![right, left], true),
        (right, vec![root], true),
        (left, vec![root], true),
        (root, Vec::new(), true),
    ]);

    assert_eq!(
        resolver.initialization_plan(entry),
        Ok(vec![root, left, right, entry])
    );
}

#[test]
fn initialization_plan_includes_a_module_without_an_initializer() {
    let dependency = ModuleId::new(7);
    let entry = ModuleId::new(31);
    let resolver = initialization_resolver(vec![
        (entry, vec![dependency], true),
        (dependency, Vec::new(), false),
    ]);

    assert_eq!(
        resolver.initialization_plan(entry),
        Ok(vec![dependency, entry])
    );
}

#[test]
fn initialization_plan_reports_a_self_cycle_with_exact_ids() {
    let module_id = ModuleId::new(7);
    let resolver = initialization_resolver(vec![(module_id, vec![module_id], true)]);

    assert_eq!(
        resolver.initialization_plan(module_id),
        Err(ModuleInitializationPlanError::DependencyCycle {
            cycle: vec![module_id, module_id],
        })
    );
}

#[test]
fn initialization_plan_reports_a_multimodule_cycle_with_exact_ids() {
    let first = ModuleId::new(7);
    let second = ModuleId::new(19);
    let third = ModuleId::new(31);
    let resolver = initialization_resolver(vec![
        (third, vec![first], true),
        (first, vec![second], true),
        (second, vec![third], true),
    ]);

    assert_eq!(
        resolver.initialization_plan(first),
        Err(ModuleInitializationPlanError::DependencyCycle {
            cycle: vec![first, second, third, first],
        })
    );
}
