use std::thread;

use super::{RuntimeTask, copy_thread_args};
use crate::queue::BlockedQueue;
use crate::registry::{ThreadId, ThreadRegistry};
use galfus_bytecode::instruction::{FuncIdx, Reg, TypeIdx};
use galfus_bytecode::{
    BytecodeFunction, BytecodeGraph, BytecodeModule, BytecodeNode, BytecodeType, Instruction,
};
use galfus_contract::{KernelDriver, RunnableTask, ThreadResult};
use galfus_core::{ModuleId, ModulePath, SemanticRevision};
use galfus_vm::thread::VirtualThread;
use galfus_vm::{HeapObject, VirtualMachine, VmEffect, VmStep, VmValue};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct TestExecutor {
    tasks: Mutex<VecDeque<galfus_contract::KernelTask>>,
}

impl TestExecutor {
    fn take_task(&self) -> Option<galfus_contract::KernelTask> {
        self.tasks.lock().unwrap().pop_front()
    }
}

impl KernelDriver for TestExecutor {
    fn on_exit(&self, _cb: Box<dyn Fn(Result<i32, String>) + Send + Sync>) {}
    fn run(&self) {}

    fn dispatch(&self, task: galfus_contract::KernelTask) {
        self.tasks.lock().unwrap().push_back(task);
    }
}

#[test]
fn thread_arguments_copy_only_byte_sequences() {
    let mut source_heap = galfus_vm::thread::PrivateHeap::new();
    let bytes_ref = source_heap.alloc(HeapObject::Array {
        element_ty: TypeIdx(0),
        elements: vec![VmValue::Uint8(b'a'), VmValue::Uint8(b'b')],
    });
    let args = VmValue::Object(source_heap.alloc(HeapObject::Array {
        element_ty: TypeIdx(1),
        elements: vec![VmValue::Object(bytes_ref)],
    }));
    let mut target_heap = galfus_vm::thread::PrivateHeap::new();

    let copied = copy_thread_args(&source_heap, &mut target_heap, &args)
        .expect("byte-sequence arguments are copied into the target heap");

    let VmValue::Object(copied_args_ref) = copied else {
        panic!("expected an argument array");
    };
    let HeapObject::Array { elements, .. } = target_heap
        .get_object(copied_args_ref)
        .expect("copied argument array exists")
    else {
        panic!("expected an argument array");
    };
    let VmValue::Object(copied_bytes_ref) = elements[0] else {
        panic!("expected a byte sequence");
    };
    assert!(matches!(
        target_heap
            .get_object(copied_bytes_ref)
            .expect("copied bytes exist"),
        HeapObject::Array { elements, .. }
            if elements == &vec![VmValue::Uint8(b'a'), VmValue::Uint8(b'b')]
    ));
}

#[test]
fn thread_arguments_reject_non_byte_values() {
    let mut source_heap = galfus_vm::thread::PrivateHeap::new();
    let args = VmValue::Object(source_heap.alloc(HeapObject::Array {
        element_ty: TypeIdx(1),
        elements: vec![VmValue::Int32(7)],
    }));
    let mut target_heap = galfus_vm::thread::PrivateHeap::new();

    assert!(copy_thread_args(&source_heap, &mut target_heap, &args).is_none());
    assert!(target_heap.objects.is_empty());
}

#[test]
fn receive_timeout_resumes_with_null() {
    let module_id = ModuleId::new(0);
    let module = BytecodeModule {
        name: "test.gfs".to_string(),
        constants: Default::default(),
        functions: vec![BytecodeFunction {
            name: "wait".to_string(),
            param_count: 0,
            local_count: 3,
            temp_count: 0,
            return_ty: TypeIdx(1),
            instructions: vec![
                Instruction::ReceiveFilter {
                    dest: Reg(0),
                    sender: Reg(1),
                    timeout: Reg(2),
                },
                Instruction::Ret { src: Reg(0) },
            ],
        }],
        types: vec![
            BytecodeType::Uint8,
            BytecodeType::Array(TypeIdx(0)),
            BytecodeType::Null,
        ],
        struct_layouts: vec![],
        choice_layouts: vec![],
        imports: vec![],
        exports: vec![],
        init_func_idx: None,
    };
    let graph = BytecodeGraph::from_modules(
        SemanticRevision::new(0),
        vec![BytecodeNode {
            id: module_id,
            path: ModulePath::new("test.gfs").expect("valid path"),
            semantic_revision: SemanticRevision::new(0),
            module,
            metadata: None,
        }],
        vec![],
    )
    .expect("valid graph");
    let vm = VirtualMachine::new(Arc::new(graph));
    let thread_id = ThreadId::from_executor(1).expect("non-zero ID");
    let mut waiting_thread = VirtualThread::new();
    vm.prepare_function(&mut waiting_thread, module_id, FuncIdx(0), vec![])
        .expect("function prepares");
    waiting_thread
        .write_reg(Reg(1), VmValue::Int64(7))
        .expect("sender register exists");
    waiting_thread
        .write_reg(Reg(2), VmValue::Int32(1))
        .expect("timeout register exists");
    assert!(matches!(
        vm.execute_with_budget(&mut waiting_thread, 10),
        Ok(VmStep::Suspend {
            effect: VmEffect::ReceiveFilter { .. },
            ..
        })
    ));

    let registry = Arc::new(Mutex::new(ThreadRegistry::new()));
    registry.lock().unwrap().register(thread_id, waiting_thread);
    let blocked = Arc::new(Mutex::new(BlockedQueue::new()));
    blocked.lock().unwrap().block_with_timeout(thread_id, 1);
    let executor = Arc::new(TestExecutor {
        tasks: Mutex::new(VecDeque::new()),
    });
    let task = RuntimeTask {
        thread_id,
        thread: VirtualThread::new(),
        vm,
        kernel: Arc::new(Mutex::new(crate::kernel::VirtualKernel::new())),
        driver: executor.clone(),
    };

    task.schedule_receive_timeout(Reg(0), 1);
    thread::sleep(Duration::from_millis(20));

    let timed_out_task = executor.take_task().expect("timeout wakes the task");
    let runnable = match timed_out_task {
        galfus_contract::KernelTask::Main(t) => t,
        galfus_contract::KernelTask::Any(t) => t,
    };
    assert!(matches!(runnable.run(10), ThreadResult::Completed(0)));
}
