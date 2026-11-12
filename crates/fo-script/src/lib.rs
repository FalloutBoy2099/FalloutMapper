//! Lecture des scripts compilés (`.int`) de Fallout 2 : procédures, tables de chaînes, désassemblage et pseudo-source.

pub mod compiler;
mod messages;
pub mod opcode;
mod opcode_table;
mod program;
mod source;
pub mod ssl;
mod structure;

pub use messages::{GameMessages, MessageSource, NoMessages};
pub use program::{
    ExportedVar, GlobalVar, Instruction, Procedure, Program, Result, ScriptError, StringTable,
    proc_flags,
};
