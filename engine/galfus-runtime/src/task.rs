#[cfg(test)]
mod tests;

use crate::driver::RuntimeEventSink;
use crate::registry;
use galfus_contract::{
    ExecutionFailure, ExecutionFailureKind, ExecutionFrame, RunnableTask, ThreadResult,
};
use galfus_vm::VirtualMachine;

use std::sync::Arc;

pub(crate) fn decode_surface_from_thread_heap(
    heap: &galfus_vm::thread::PrivateHeap,
    schema: &galfus_contract::SurfaceSchema,
    value: galfus_vm::VmValue,
    expected: galfus_bytecode::instruction::TypeIdx,
    module: &galfus_bytecode::BytecodeModule,
) -> Result<galfus_contract::SurfaceValue, String> {
    use galfus_bytecode::BytecodeType;
    use galfus_contract::{SurfaceCodecError, SurfaceSchema, SurfaceValue};

    let expected_type = module
        .types
        .get(expected.raw() as usize)
        .ok_or_else(|| "missing expected bytecode type".to_string())?;
    let mismatch = || format!("surface schema {schema:?} does not match {expected_type:?}");
    match (schema, value, expected_type) {
        (SurfaceSchema::Null, galfus_vm::VmValue::Null, BytecodeType::Null) => {
            Ok(SurfaceValue::Null)
        }
        (SurfaceSchema::Bool, galfus_vm::VmValue::Bool(value), BytecodeType::Bool) => {
            Ok(SurfaceValue::Bool(value))
        }
        (SurfaceSchema::U16, galfus_vm::VmValue::Uint16(value), BytecodeType::Uint16) => {
            Ok(SurfaceValue::U16(value))
        }
        (SurfaceSchema::I32, galfus_vm::VmValue::Int32(value), BytecodeType::Int32) => {
            Ok(SurfaceValue::I32(value))
        }
        (SurfaceSchema::I64, galfus_vm::VmValue::Int64(value), BytecodeType::Int64) => {
            Ok(SurfaceValue::I64(value))
        }
        (SurfaceSchema::U32, galfus_vm::VmValue::Uint32(value), BytecodeType::Uint32) => {
            Ok(SurfaceValue::U32(value))
        }
        (SurfaceSchema::U64, galfus_vm::VmValue::Uint64(value), BytecodeType::Uint64) => {
            Ok(SurfaceValue::U64(value))
        }
        (SurfaceSchema::F32, galfus_vm::VmValue::Float32(value), BytecodeType::Float32) => {
            Ok(SurfaceValue::F32(galfus_core::normalize_f32(value)))
        }
        (SurfaceSchema::F64, galfus_vm::VmValue::Float64(value), BytecodeType::Float64) => {
            Ok(SurfaceValue::F64(galfus_core::normalize_f64(value)))
        }
        (
            SurfaceSchema::Bytes,
            galfus_vm::VmValue::Object(reference),
            BytecodeType::Array(item),
        ) if matches!(
            module.types.get(item.raw() as usize),
            Some(BytecodeType::Uint8)
        ) =>
        {
            let galfus_vm::HeapObject::Array { elements, .. } = heap
                .get_object(reference)
                .map_err(|_| "surface bytes reference is invalid".to_string())?
            else {
                return Err(mismatch());
            };
            let bytes = elements
                .iter()
                .map(|value| match value {
                    galfus_vm::VmValue::Uint8(value) => Ok(*value),
                    _ => Err(mismatch()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SurfaceValue::Bytes(bytes))
        }
        (SurfaceSchema::Optional(_), galfus_vm::VmValue::Null, BytecodeType::Nullable(_)) => {
            Ok(SurfaceValue::Null)
        }
        (SurfaceSchema::Optional(schema), value, BytecodeType::Nullable(item)) => {
            decode_surface_from_thread_heap(heap, schema, value, *item, module)
        }
        (
            SurfaceSchema::List(schema),
            galfus_vm::VmValue::Object(reference),
            BytecodeType::Array(item),
        ) => {
            let galfus_vm::HeapObject::Array { elements, .. } = heap
                .get_object(reference)
                .map_err(|_| "surface list reference is invalid".to_string())?
            else {
                return Err(mismatch());
            };
            let values = elements
                .iter()
                .cloned()
                .map(|value| decode_surface_from_thread_heap(heap, schema, value, *item, module))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SurfaceValue::List(values))
        }
        (
            SurfaceSchema::Tuple(schemas),
            galfus_vm::VmValue::Object(reference),
            BytecodeType::Tuple(item_types),
        ) => {
            let galfus_vm::HeapObject::Tuple { elements } = heap
                .get_object(reference)
                .map_err(|_| "surface tuple reference is invalid".to_string())?
            else {
                return Err(mismatch());
            };
            if schemas.len() != elements.len() || elements.len() != item_types.len() {
                return Err(mismatch());
            }
            let values = schemas
                .iter()
                .zip(elements.iter().cloned().zip(item_types))
                .map(|(schema, (value, item_type))| {
                    decode_surface_from_thread_heap(heap, schema, value, *item_type, module)
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SurfaceValue::Tuple(values))
        }
        (
            SurfaceSchema::Struct { fields, .. },
            galfus_vm::VmValue::Object(reference),
            BytecodeType::Struct(layout_idx),
        ) => {
            let galfus_vm::HeapObject::Struct { fields: values, .. } =
                heap.get_object(reference)
                    .map_err(|_| "surface struct reference is invalid".to_string())?
            else {
                return Err(mismatch());
            };
            let layout = module
                .struct_layouts
                .get(layout_idx.raw() as usize)
                .ok_or_else(|| "missing struct layout".to_string())?;
            if fields.len() != values.len() || values.len() != layout.fields.len() {
                return Err(mismatch());
            }
            let values = fields
                .iter()
                .zip(values.iter().cloned().zip(layout.fields.iter()))
                .map(|(field, (value, layout))| {
                    Ok((
                        field.name.clone(),
                        decode_surface_from_thread_heap(
                            heap,
                            &field.schema,
                            value,
                            layout.ty,
                            module,
                        )?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(SurfaceValue::Struct(values))
        }
        (
            SurfaceSchema::Choice { variants, .. },
            galfus_vm::VmValue::Object(reference),
            BytecodeType::Choice(layout_idx),
        ) => {
            let galfus_vm::HeapObject::Choice {
                variant_idx,
                payload,
                ..
            } = heap
                .get_object(reference)
                .map_err(|_| "surface choice reference is invalid".to_string())?
            else {
                return Err(mismatch());
            };
            let schema_variant = variants
                .get(*variant_idx as usize)
                .ok_or_else(|| "surface choice has too many variants".to_string())?;
            let layout = module
                .choice_layouts
                .get(layout_idx.raw() as usize)
                .ok_or_else(|| "missing choice layout".to_string())?;
            let variant_layout = layout
                .variants
                .get(*variant_idx as usize)
                .ok_or_else(|| "surface choice layout has too many variants".to_string())?;
            let payload = match (schema_variant.payload.as_ref(), variant_layout.payload_ty) {
                (None, None) => None,
                (Some(schema), Some(payload_type)) => Some(Box::new(
                    decode_surface_from_thread_heap(heap, schema, *payload, payload_type, module)?,
                )),
                _ => return Err(mismatch()),
            };
            Ok(SurfaceValue::Choice {
                variant: schema_variant.name.clone(),
                payload,
            })
        }
        (
            SurfaceSchema::Handle { .. },
            galfus_vm::VmValue::Object(reference),
            BytecodeType::AdapterHandle(type_id),
        ) => {
            let galfus_vm::HeapObject::AdapterHandle {
                type_id: actual,
                id,
                ..
            } = heap
                .get_object(reference)
                .map_err(|_| "surface handle reference is invalid".to_string())?
            else {
                return Err(mismatch());
            };
            if actual != type_id {
                return Err(SurfaceCodecError::InvalidHandle.to_string());
            }
            Ok(SurfaceValue::Handle(galfus_contract::SurfaceHandle {
                type_id: actual.clone(),
                id: *id,
            }))
        }
        _ => Err(mismatch()),
    }
}

pub(crate) fn encode_future_value_into_thread_heap(
    heap: &mut galfus_vm::thread::PrivateHeap,
    value: crate::event::FutureValue,
    expected: galfus_bytecode::instruction::TypeIdx,
    module_id: galfus_core::ModuleId,
    module: &galfus_bytecode::BytecodeModule,
) -> Result<galfus_vm::VmValue, String> {
    match value {
        crate::event::FutureValue::I32(code) => Ok(galfus_vm::VmValue::Int32(code)),
        crate::event::FutureValue::I64(val) => Ok(galfus_vm::VmValue::Int64(val)),
        crate::event::FutureValue::F64(val) => Ok(galfus_vm::VmValue::Float64(val)),
        crate::event::FutureValue::Bool(val) => Ok(galfus_vm::VmValue::Bool(val)),
        crate::event::FutureValue::Null => Ok(galfus_vm::VmValue::Null),
        crate::event::FutureValue::Function {
            module_id: id,
            func_idx: idx,
        } => Ok(galfus_vm::VmValue::Function {
            module_id: galfus_core::ModuleId::new(id),
            func_idx: galfus_bytecode::instruction::FuncIdx(idx.try_into().unwrap()),
        }),
        crate::event::FutureValue::Bytes(bytes) => {
            let element_ty = match module.types.get(expected.raw() as usize) {
                Some(galfus_bytecode::BytecodeType::Array(ty)) => *ty,
                _ => return Err("expected array type".to_string()),
            };
            let elements = bytes.into_iter().map(galfus_vm::VmValue::Uint8).collect();
            let reference = heap
                .alloc(galfus_vm::HeapObject::Array {
                    module_id,
                    element_ty,
                    elements,
                })
                .map_err(|_| "bytes exceed heap quota".to_string())?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        crate::event::FutureValue::Surface { contract, value } => {
            if !contract.validates() {
                return Err("surface contract fingerprint is invalid".to_string());
            }
            encode_surface_into_thread_heap(
                heap,
                &contract.schema,
                value,
                expected,
                module_id,
                module,
            )
        }
        crate::event::FutureValue::Aggregate(values) => {
            encode_aggregate_into_thread_heap(heap, values, expected, module_id, module)
        }
    }
}

pub(crate) fn encode_surface_into_thread_heap(
    heap: &mut galfus_vm::thread::PrivateHeap,
    schema: &galfus_contract::SurfaceSchema,
    value: galfus_contract::SurfaceValue,
    expected: galfus_bytecode::instruction::TypeIdx,
    module_id: galfus_core::ModuleId,
    module: &galfus_bytecode::BytecodeModule,
) -> Result<galfus_vm::VmValue, String> {
    use galfus_bytecode::BytecodeType;
    use galfus_contract::{SurfaceCodecError, SurfaceSchema, SurfaceValue};

    schema
        .validate_value(&value)
        .map_err(|error| format!("surface value: {error}"))?;
    let expected_type = module
        .types
        .get(expected.raw() as usize)
        .ok_or_else(|| "missing expected bytecode type".to_string())?;
    let mismatch = || format!("surface schema {schema:?} does not match {expected_type:?}");
    match (schema, value, expected_type) {
        (SurfaceSchema::Null, SurfaceValue::Null, BytecodeType::Null) => {
            Ok(galfus_vm::VmValue::Null)
        }
        (SurfaceSchema::Bool, SurfaceValue::Bool(value), BytecodeType::Bool) => {
            Ok(galfus_vm::VmValue::Bool(value))
        }
        (SurfaceSchema::U16, SurfaceValue::U16(value), BytecodeType::Uint16) => {
            Ok(galfus_vm::VmValue::Uint16(value))
        }
        (SurfaceSchema::I32, SurfaceValue::I32(value), BytecodeType::Int32) => {
            Ok(galfus_vm::VmValue::Int32(value))
        }
        (SurfaceSchema::I64, SurfaceValue::I64(value), BytecodeType::Int64) => {
            Ok(galfus_vm::VmValue::Int64(value))
        }
        (SurfaceSchema::U32, SurfaceValue::U32(value), BytecodeType::Uint32) => {
            Ok(galfus_vm::VmValue::Uint32(value))
        }
        (SurfaceSchema::U64, SurfaceValue::U64(value), BytecodeType::Uint64) => {
            Ok(galfus_vm::VmValue::Uint64(value))
        }
        (SurfaceSchema::F32, SurfaceValue::F32(value), BytecodeType::Float32) => Ok(
            galfus_vm::VmValue::Float32(galfus_core::normalize_f32(value)),
        ),
        (SurfaceSchema::F64, SurfaceValue::F64(value), BytecodeType::Float64) => Ok(
            galfus_vm::VmValue::Float64(galfus_core::normalize_f64(value)),
        ),
        (SurfaceSchema::Bytes, SurfaceValue::Bytes(bytes), BytecodeType::Array(element_type))
            if matches!(
                module.types.get(element_type.raw() as usize),
                Some(BytecodeType::Uint8)
            ) =>
        {
            let elements = bytes.into_iter().map(galfus_vm::VmValue::Uint8).collect();
            let reference = heap
                .alloc(galfus_vm::HeapObject::Array {
                    module_id,
                    element_ty: *element_type,
                    elements,
                })
                .map_err(|_| "surface bytes exceed heap quota".to_string())?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        (SurfaceSchema::Optional(_), SurfaceValue::Null, BytecodeType::Nullable(_)) => {
            Ok(galfus_vm::VmValue::Null)
        }
        (SurfaceSchema::Optional(inner), value, BytecodeType::Nullable(inner_type)) => {
            encode_surface_into_thread_heap(heap, inner, value, *inner_type, module_id, module)
        }
        (
            SurfaceSchema::List(item_schema),
            SurfaceValue::List(values),
            BytecodeType::Array(item_type),
        ) => {
            let elements = values
                .into_iter()
                .map(|value| {
                    encode_surface_into_thread_heap(
                        heap,
                        item_schema,
                        value,
                        *item_type,
                        module_id,
                        module,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let reference = heap
                .alloc(galfus_vm::HeapObject::Array {
                    module_id,
                    element_ty: *item_type,
                    elements,
                })
                .map_err(|_| "surface list exceeds heap quota".to_string())?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        (
            SurfaceSchema::Tuple(schemas),
            SurfaceValue::Tuple(values),
            BytecodeType::Tuple(item_types),
        ) if schemas.len() == values.len() && values.len() == item_types.len() => {
            let elements = schemas
                .iter()
                .zip(values.into_iter().zip(item_types))
                .map(|(schema, (value, item_type))| {
                    encode_surface_into_thread_heap(
                        heap, schema, value, *item_type, module_id, module,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let reference = heap
                .alloc(galfus_vm::HeapObject::Tuple { elements })
                .map_err(|_| "surface tuple exceeds heap quota".to_string())?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        (
            SurfaceSchema::Struct { fields, .. },
            SurfaceValue::Struct(values),
            BytecodeType::Struct(layout_idx),
        ) => {
            let layout = module
                .struct_layouts
                .get(layout_idx.raw() as usize)
                .ok_or_else(|| "missing struct layout".to_string())?;
            if fields.len() != layout.fields.len() {
                return Err(mismatch());
            }
            let fields = fields
                .iter()
                .zip(layout.fields.iter())
                .map(|(field_schema, field_layout)| {
                    let value = values
                        .iter()
                        .find_map(|(name, value)| (name == &field_schema.name).then_some(value))
                        .cloned()
                        .ok_or_else(|| {
                            SurfaceCodecError::MissingField(field_schema.name.clone()).to_string()
                        })?;
                    encode_surface_into_thread_heap(
                        heap,
                        &field_schema.schema,
                        value,
                        field_layout.ty,
                        module_id,
                        module,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let reference = heap
                .alloc(galfus_vm::HeapObject::Struct {
                    module_id,
                    layout_idx: *layout_idx,
                    fields,
                })
                .map_err(|_| "surface struct exceeds heap quota".to_string())?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        (
            SurfaceSchema::Choice { variants, .. },
            SurfaceValue::Choice { variant, payload },
            BytecodeType::Choice(layout_idx),
        ) => {
            let schema_variant = variants
                .iter()
                .enumerate()
                .find(|(_, candidate)| candidate.name == variant)
                .ok_or_else(|| SurfaceCodecError::InvalidTag(variant.clone()).to_string())?;
            let layout = module
                .choice_layouts
                .get(layout_idx.raw() as usize)
                .ok_or_else(|| "missing choice layout".to_string())?;
            let variant_layout = layout
                .variants
                .get(schema_variant.0)
                .ok_or_else(|| "surface choice has too many variants".to_string())?;
            let payload = match (
                schema_variant.1.payload.as_ref(),
                variant_layout.payload_ty,
                payload,
            ) {
                (None, None, None) => galfus_vm::VmValue::Null,
                (Some(schema), Some(payload_type), Some(value)) => encode_surface_into_thread_heap(
                    heap,
                    schema,
                    *value,
                    payload_type,
                    module_id,
                    module,
                )?,
                _ => return Err(mismatch()),
            };
            let reference = heap
                .alloc(galfus_vm::HeapObject::Choice {
                    module_id,
                    layout_idx: *layout_idx,
                    variant_idx: schema_variant.0 as u16,
                    payload,
                })
                .map_err(|_| "surface choice exceeds heap quota".to_string())?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        (SurfaceSchema::Handle { .. }, SurfaceValue::Handle(_), _) => {
            Err("surface handles require a provider handle runtime representation".to_string())
        }
        _ => Err(mismatch()),
    }
}

fn encode_aggregate_into_thread_heap(
    heap: &mut galfus_vm::thread::PrivateHeap,
    values: Vec<crate::event::FutureValue>,
    expected: galfus_bytecode::instruction::TypeIdx,
    module_id: galfus_core::ModuleId,
    module: &galfus_bytecode::BytecodeModule,
) -> Result<galfus_vm::VmValue, String> {
    use galfus_bytecode::BytecodeType;

    let expected_type = module
        .types
        .get(expected.raw() as usize)
        .ok_or_else(|| "missing aggregate bytecode type".to_string())?;
    match expected_type {
        BytecodeType::Array(element_type) => {
            let elements = values
                .into_iter()
                .map(|value| {
                    encode_future_value_into_thread_heap(
                        heap,
                        value,
                        *element_type,
                        module_id,
                        module,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let reference = heap
                .alloc(galfus_vm::HeapObject::Array {
                    module_id,
                    element_ty: *element_type,
                    elements,
                })
                .map_err(|_| "aggregate exceeds heap quota".to_string())?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        BytecodeType::Tuple(element_types) if element_types.len() == values.len() => {
            let elements = values
                .into_iter()
                .zip(element_types)
                .map(|(value, element_type)| {
                    encode_future_value_into_thread_heap(
                        heap,
                        value,
                        *element_type,
                        module_id,
                        module,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let reference = heap
                .alloc(galfus_vm::HeapObject::Tuple { elements })
                .map_err(|_| "aggregate exceeds heap quota".to_string())?;
            Ok(galfus_vm::VmValue::Object(reference))
        }
        _ => Err(format!("aggregate result does not match {expected_type:?}")),
    }
}

pub struct RuntimeTask {
    pub thread_id: registry::ThreadId,
    pub thread: Option<galfus_vm::thread::VmThreadState>,
    pub vm: Arc<VirtualMachine>,
    pub events: Arc<dyn RuntimeEventSink>,
    pub future_completion: Option<(registry::ThreadId, galfus_core::FutureLease)>,
}

pub(crate) struct QuotaTask<T: galfus_contract::RunnableTask> {
    inner: Option<T>,
    quota: Arc<std::sync::Mutex<galfus_vm::quota::GlobalQuota>>,
}

impl<T: galfus_contract::RunnableTask> QuotaTask<T> {
    pub(crate) fn new(
        inner: T,
        quota: Arc<std::sync::Mutex<galfus_vm::quota::GlobalQuota>>,
    ) -> Self {
        Self {
            inner: Some(inner),
            quota,
        }
    }
}

impl<T: galfus_contract::RunnableTask> galfus_contract::RunnableTask for QuotaTask<T> {
    fn run(mut self: Box<Self>, budget: usize) -> galfus_contract::ThreadResult {
        let inner = self.inner.take().unwrap();
        Box::new(inner).run(budget)
    }
}

impl<T: galfus_contract::RunnableTask> Drop for QuotaTask<T> {
    fn drop(&mut self) {
        self.quota.lock().unwrap().release_kernel_tasks(1);
    }
}

impl RuntimeTask {
    pub(crate) fn new(
        thread_id: registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        vm: Arc<VirtualMachine>,
        events: Arc<dyn RuntimeEventSink>,
        future_completion: Option<(registry::ThreadId, galfus_core::FutureLease)>,
    ) -> Self {
        Self {
            thread_id,
            thread: Some(thread),
            vm,
            events,
            future_completion,
        }
    }
}

fn with_initialization_context(
    thread: &galfus_vm::thread::VmThreadState,
    failure: ExecutionFailure,
) -> ExecutionFailure {
    match thread.initializing_module() {
        Some(module_id) => {
            let mut initialization_failure = ExecutionFailure::new(
                ExecutionFailureKind::InitializationFailure,
                "module initializer failed",
            )
            .with_module_id(module_id.raw().into());
            if let Some(thread_id) = failure.thread_id {
                initialization_failure = initialization_failure.with_thread_id(thread_id);
            }
            initialization_failure.with_cause(failure)
        }
        None => failure,
    }
}

pub(crate) fn execution_stack(thread: &galfus_vm::thread::VmThreadState) -> Vec<ExecutionFrame> {
    thread
        .call_stack
        .iter()
        .rev()
        .map(|frame| ExecutionFrame {
            module_id: frame.module_id.raw().into(),
            function_id: frame.func_idx.raw().into(),
            instruction_offset: frame.pc.saturating_sub(1) as u32,
        })
        .collect()
}

pub(crate) fn with_execution_stack(
    failure: ExecutionFailure,
    stack: Vec<ExecutionFrame>,
) -> ExecutionFailure {
    if failure.stack.is_empty() {
        failure.with_stack(stack)
    } else {
        failure
    }
}

fn panic_stack(panic: &galfus_vm::VmPanic) -> Vec<ExecutionFrame> {
    panic
        .stack_trace
        .iter()
        .map(|frame| ExecutionFrame {
            module_id: frame.module_id.raw().into(),
            function_id: frame.func_idx.raw().into(),
            instruction_offset: frame.instruction_offset as u32,
        })
        .collect()
}

impl RunnableTask for RuntimeTask {
    fn run(mut self: Box<Self>, budget: usize) -> ThreadResult {
        let mut thread = self.thread.take().unwrap();
        const MAX_LOCAL_CPU_QUANTA: usize = 4;
        let mut local_quanta = 0;
        let step = loop {
            match self.vm.execute_with_budget(&mut thread, budget) {
                Ok(galfus_vm::VmStep::Continue)
                    if local_quanta + 1 < MAX_LOCAL_CPU_QUANTA
                        && !self.events.has_pending_events() =>
                {
                    local_quanta += 1;
                }
                Ok(step) => break step,
                Err(e) => {
                    let failure = with_initialization_context(
                        &thread,
                        ExecutionFailure::new(ExecutionFailureKind::VmPanic, e.to_string())
                            .with_thread_id(self.thread_id)
                            .with_stack(panic_stack(&e)),
                    );
                    let _ = self.events.submit(crate::event::RuntimeEvent::Failed {
                        thread_id: self.thread_id,
                        error: failure.clone(),
                    });
                    return ThreadResult::Discarded;
                }
            }
        };

        match step {
            galfus_vm::VmStep::Continue => {
                let _ = self.events.submit(crate::event::RuntimeEvent::Yielded {
                    thread_id: self.thread_id,
                    thread,
                });
                ThreadResult::Discarded
            }
            galfus_vm::VmStep::Return {
                value,
                module_id,
                return_type: _,
            } => {
                if let Some(module_id) = thread.finish_module_initialization() {
                    let _ = self.events.submit(crate::event::RuntimeEvent::Initialized {
                        thread_id: self.thread_id,
                        thread,
                        module_id,
                    });
                    return ThreadResult::Completed(Ok(0));
                }
                let result = match value {
                    galfus_vm::VmValue::Int32(code) => Ok(code),
                    _ => Err(ExecutionFailure::new(
                        ExecutionFailureKind::BoundaryCodecFailure,
                        format!("failed to extract i32 from thread return value (got {value:?})"),
                    )
                    .with_thread_id(self.thread_id)
                    .with_module_id(module_id.raw().into())
                    .with_stack(execution_stack(&thread))),
                };

                if let Some((owner_thread_id, future_lease)) = self.future_completion {
                    let _ = self
                        .events
                        .submit(crate::event::RuntimeEvent::FutureWorkerCompleted {
                            worker_thread_id: self.thread_id,
                            owner_thread_id,
                            future_lease,
                            thread,
                            result: result.clone(),
                        });
                } else {
                    let _ = self.events.submit(crate::event::RuntimeEvent::Exited {
                        thread_id: self.thread_id,
                        thread,
                        result: result.clone(),
                    });
                }
                ThreadResult::Completed(result)
            }
            galfus_vm::VmStep::Suspend {
                effect,
                mut continuation,
            } => {
                continuation.origin_thread_id = Some(self.thread_id);
                let _ = self.events.submit(crate::event::RuntimeEvent::Syscall {
                    thread_id: self.thread_id,
                    thread,
                    effect,
                    continuation,
                });
                ThreadResult::Blocked { timeout: None }
            }
            galfus_vm::VmStep::Failed(err) => {
                let err = with_initialization_context(
                    &thread,
                    with_execution_stack(
                        err.with_thread_id(self.thread_id),
                        execution_stack(&thread),
                    ),
                );
                let _ = self.events.submit(crate::event::RuntimeEvent::Failed {
                    thread_id: self.thread_id,
                    error: err.clone(),
                });
                ThreadResult::Discarded
            }
        }
    }

    fn into_any_thread(self: Box<Self>) -> Option<Box<dyn RunnableTask + Send>> {
        Some(self)
    }
}
