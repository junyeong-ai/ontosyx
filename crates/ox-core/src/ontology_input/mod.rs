mod dtos;
mod exchange;
mod transform;

pub use dtos::{
    InputEdgeTypeDef, InputIndexDef, InputNodeConstraint, InputNodeTypeDef, InputPropertyDef,
    OntologyInputIR,
};
pub use exchange::to_exchange_format;
pub use transform::{NormalizeResult, NormalizeWarning, normalize};
