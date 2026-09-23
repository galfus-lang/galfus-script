use super::*;

use crate::driver::{CooperativeDriver, ExecutionDriver};
use crate::module_resolver::{ModuleProducer, ModuleResolver};
use galfus_bytecode::{
    BytecodeModule, BytecodeNode, ConstantPool, ModuleCatalog, ModuleDescriptor,
    ModuleResolveContext, ModuleResolveError,
};
use galfus_contract::{ContentHash, KernelDriver};
use galfus_core::{ModuleId, ModulePath, SemanticRevision};
use galfus_vm::thread::VmThreadState;
use galfus_vm::{Continuation, VirtualMachine, VmValue};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingProducer {
    node: Arc<BytecodeNode>,
    calls: AtomicUsize,
}

struct FailingProducer {
    calls: AtomicUsize,
    error: ModuleResolveError,
}

impl ModuleProducer for CountingProducer {
    fn produce(&self, _module_id: ModuleId) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.node.clone())
    }
}

impl ModuleProducer for FailingProducer {
    fn produce(&self, _module_id: ModuleId) -> Result<Arc<BytecodeNode>, ModuleResolveError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(self.error.clone())
    }
}

fn module_node(module_id: ModuleId) -> Arc<BytecodeNode> {
    Arc::new(BytecodeNode {
        id: module_id,
        path: ModulePath::new("target.gfs").expect("valid module path"),
        semantic_revision: SemanticRevision::new(0),
        module: BytecodeModule {
            name: "target".to_string(),
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

fn resolver(module_id: ModuleId, producer: Arc<dyn ModuleProducer>) -> Arc<ModuleResolver> {
    let catalog = ModuleCatalog::new(vec![
        ModuleDescriptor::new(
            module_id,
            ModulePath::new("target.gfs").expect("valid module path"),
            Vec::new(),
            Vec::new(),
            false,
            ContentHash::of(b"interface"),
            ContentHash::of(b"chunk"),
        )
        .expect("valid module descriptor"),
    ])
    .expect("valid module catalog");
    Arc::new(ModuleResolver::new(&catalog, producer))
}

fn running_thread(orchestrator: &mut Orchestrator) -> (galfus_core::ThreadId, VmThreadState) {
    let mut thread = VmThreadState::test_new();
    thread.registers.push(VmValue::Null);
    let thread_id = orchestrator
        .kernel_mut()
        .spawn(thread, None)
        .expect("thread registers");
    assert!(orchestrator.kernel_mut().mark_running(thread_id));
    let thread = orchestrator
        .kernel_mut()
        .take_thread(thread_id)
        .expect("running thread is available");
    (thread_id, thread)
}

#[test]
fn concurrent_module_load_effects_share_one_production_and_resume_every_waiter() {
    let module_id = ModuleId::new(31);
    let producer = Arc::new(CountingProducer {
        node: module_node(module_id),
        calls: AtomicUsize::new(0),
    });
    let resolver = resolver(module_id, producer.clone());
    let driver = Rc::new(CooperativeDriver::new());
    let mut orchestrator = Orchestrator::test_new();
    orchestrator.set_event_sink(driver.event_sink());
    orchestrator.set_driver(driver.clone());
    orchestrator.set_vm(Arc::new(VirtualMachine::from_ready_modules([])));
    orchestrator.set_module_resolver(resolver);

    let (first_id, first_thread) = running_thread(&mut orchestrator);
    let (second_id, second_thread) = running_thread(&mut orchestrator);
    orchestrator.handle_module_load(
        first_id,
        first_thread,
        Continuation::for_future_handle(galfus_bytecode::Reg(0)).with_origin(first_id),
        module_id,
    );
    orchestrator.handle_module_load(
        second_id,
        second_thread,
        Continuation::for_future_handle(galfus_bytecode::Reg(0)).with_origin(second_id),
        module_id,
    );

    assert_eq!(producer.calls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        driver.step(),
        galfus_contract::ExecutorStepResult::Running
    ));
    orchestrator.process_events();

    assert_eq!(producer.calls.load(Ordering::SeqCst), 1);
    assert!(orchestrator.module_load_waiters.is_empty());
    assert!(
        orchestrator
            .vm
            .as_ref()
            .expect("VM remains configured")
            .get_module(module_id)
            .is_ok()
    );
    assert!(orchestrator.kernel().is_running(first_id));
    assert!(orchestrator.kernel().is_running(second_id));
}

#[test]
fn failed_module_load_is_cached_and_reports_the_same_module_id_to_waiters() {
    let module_id = ModuleId::new(31);
    let producer = Arc::new(FailingProducer {
        calls: AtomicUsize::new(0),
        error: ModuleResolveError::ProducerFailed {
            context: ModuleResolveContext::new(module_id),
        },
    });
    let resolver = resolver(module_id, producer.clone());
    let driver = Rc::new(CooperativeDriver::new());
    let mut orchestrator = Orchestrator::test_new();
    orchestrator.set_event_sink(driver.event_sink());
    orchestrator.set_driver(driver.clone());
    orchestrator.set_vm(Arc::new(VirtualMachine::from_ready_modules([])));
    orchestrator.set_module_resolver(resolver.clone());

    let (first_id, first_thread) = running_thread(&mut orchestrator);
    let (second_id, second_thread) = running_thread(&mut orchestrator);
    orchestrator.handle_module_load(
        first_id,
        first_thread,
        Continuation::for_future_handle(galfus_bytecode::Reg(0)).with_origin(first_id),
        module_id,
    );
    orchestrator.handle_module_load(
        second_id,
        second_thread,
        Continuation::for_future_handle(galfus_bytecode::Reg(0)).with_origin(second_id),
        module_id,
    );

    assert!(matches!(
        driver.step(),
        galfus_contract::ExecutorStepResult::Running
    ));
    orchestrator.process_events();

    assert_eq!(producer.calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        resolver.request_module_load(module_id),
        Ok(ModuleLoadRequest::Failed(ModuleResolveError::ProducerFailed { context }))
            if context == ModuleResolveContext::new(module_id)
    ));
    let failure = orchestrator
        .failure
        .as_ref()
        .expect("load failure is recorded");
    assert_eq!(failure.module_id, Some(module_id.raw().into()));
    assert_eq!(
        failure.kind,
        galfus_contract::ExecutionFailureKind::InternalRuntimeFailure
    );
}
