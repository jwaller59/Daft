pub mod agg;
pub mod function_args;
#[cfg(test)]
mod macro_tests;
pub mod map;
pub mod partitioning;
pub mod prelude;
pub mod python;
pub mod scalar;
pub mod sketch;
pub mod struct_;

use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result, Write},
    hash::Hash,
    sync::{Arc, LazyLock, RwLock},
};

use common_error::DaftResult;
use daft_core::prelude::*;
pub use function_args::{FunctionArg, FunctionArgs, UnaryArg};
use python::PythonUDF;
pub use scalar::*;
use serde::{Deserialize, Serialize};

use self::{map::MapExpr, partitioning::PartitioningExpr, sketch::SketchExpr, struct_::StructExpr};
use crate::ExprRef;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FunctionExpr {
    Map(MapExpr),
    Sketch(SketchExpr),
    Struct(StructExpr),
    Python(PythonUDF),
    Partitioning(PartitioningExpr),
}

pub trait FunctionEvaluator {
    fn fn_name(&self) -> &'static str;
    fn to_field(
        &self,
        inputs: &[ExprRef],
        schema: &Schema,
        expr: &FunctionExpr,
    ) -> DaftResult<Field>;
    fn evaluate(&self, inputs: &[Series], expr: &FunctionExpr) -> DaftResult<Series>;
}

impl FunctionExpr {
    #[inline]
    fn get_evaluator(&self) -> &dyn FunctionEvaluator {
        match self {
            Self::Map(expr) => expr.get_evaluator(),
            Self::Sketch(expr) => expr.get_evaluator(),
            Self::Struct(expr) => expr.get_evaluator(),
            Self::Python(expr) => expr,
            Self::Partitioning(expr) => expr.get_evaluator(),
        }
    }
}

impl Display for FunctionExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.fn_name())
    }
}

impl FunctionEvaluator for FunctionExpr {
    fn fn_name(&self) -> &'static str {
        self.get_evaluator().fn_name()
    }

    fn to_field(
        &self,
        inputs: &[ExprRef],
        schema: &Schema,
        expr: &FunctionExpr,
    ) -> DaftResult<Field> {
        self.get_evaluator().to_field(inputs, schema, expr)
    }

    fn evaluate(&self, inputs: &[Series], expr: &FunctionExpr) -> DaftResult<Series> {
        self.get_evaluator().evaluate(inputs, expr)
    }
}

pub fn function_display(f: &mut Formatter, func: &FunctionExpr, inputs: &[ExprRef]) -> Result {
    write!(f, "{}(", func)?;
    for (i, input) in inputs.iter().enumerate() {
        if i != 0 {
            write!(f, ", ")?;
        }
        write!(f, "{input}")?;
    }
    write!(f, ")")?;
    Ok(())
}

pub fn function_display_without_formatter(
    func: &FunctionExpr,
    inputs: &[ExprRef],
) -> std::result::Result<String, std::fmt::Error> {
    let mut f = String::default();
    write!(&mut f, "{}(", func)?;
    for (i, input) in inputs.iter().enumerate() {
        if i != 0 {
            write!(&mut f, ", ")?;
        }
        write!(&mut f, "{input}")?;
    }
    write!(&mut f, ")")?;
    Ok(f)
}

pub fn function_semantic_id(func: &FunctionExpr, inputs: &[ExprRef], schema: &Schema) -> FieldID {
    let inputs = inputs
        .iter()
        .map(|expr| expr.semantic_id(schema).id.to_string())
        .collect::<Vec<String>>()
        .join(", ");
    // TODO: check for function idempotency here.
    FieldID::new(format!("Function_{func:?}({inputs})"))
}

#[derive(Default)]
pub struct FunctionRegistry {
    // Todo: Use the Bindings object instead, so we can get aliases and case handling.
    map: HashMap<String, Arc<dyn ScalarUDF>>,
}
pub trait FunctionModule {
    /// Register this module to the given [SQLFunctions] table.
    fn register(_parent: &mut FunctionRegistry);
}

impl FunctionRegistry {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
    pub fn register<Mod: FunctionModule>(&mut self) {
        Mod::register(self);
    }

    pub fn add_fn<UDF: ScalarUDF + 'static>(&mut self, func: UDF) {
        let func = Arc::new(func);
        // todo: use bindings instead of hashmap so we don't need duplicate entries.
        for alias in func.aliases() {
            self.map.insert((*alias).to_string(), func.clone());
        }
        self.map.insert(func.name().to_string(), func);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn ScalarUDF>> {
        self.map.get(name).cloned()
    }

    pub fn entries(&self) -> impl Iterator<Item = (&String, &Arc<dyn ScalarUDF>)> {
        self.map.iter()
    }
}

pub static FUNCTION_REGISTRY: LazyLock<RwLock<FunctionRegistry>> =
    LazyLock::new(|| RwLock::new(FunctionRegistry::new()));
