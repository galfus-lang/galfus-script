//! Galfus Virtual Machine
//!
//! See the Runtime Ownership Matrix in the Architecture Reference (`docs/Galfus_Architecture_Reference.md`)
//! for authoritative details on the lifecycle and ownership of VM-level thread states and objects.

pub mod error;
pub mod quota;
pub mod runtime;
#[cfg(test)]
mod tests;
pub mod thread;

pub use error::{StackFrameInfo, VmError, VmPanic};
pub use runtime::{
    CallFrame, Continuation, HeapObject, RuntimeModuleState, VirtualMachine, VmContext, VmEffect,
    VmObjectRef, VmStep, VmValue,
};
pub mod heap;
