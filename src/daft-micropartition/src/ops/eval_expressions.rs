use std::{collections::HashSet, sync::Arc};

use common_error::{DaftError, DaftResult};
use daft_core::prelude::Schema;
use daft_dsl::{expr::bound_expr::BoundExpr, ExprRef};
use daft_io::IOStatsContext;
use daft_stats::{ColumnRangeStatistics, TableStatistics};
use snafu::ResultExt;

use crate::{micropartition::MicroPartition, DaftCoreComputeSnafu};

fn infer_schema(exprs: &[ExprRef], schema: &Schema) -> DaftResult<Schema> {
    let fields = exprs
        .iter()
        .map(|e| e.to_field(schema).context(DaftCoreComputeSnafu))
        .collect::<crate::Result<Vec<_>>>()?;

    let mut seen: HashSet<String> = HashSet::new();
    for field in &fields {
        let name = &field.name;
        if seen.contains(name) {
            return Err(DaftError::ValueError(format!(
                "Duplicate name found when evaluating expressions: {name}"
            )));
        }
        seen.insert(name.clone());
    }
    Ok(Schema::new(fields))
}

impl MicroPartition {
    pub fn eval_expression_list(&self, exprs: &[ExprRef]) -> DaftResult<Self> {
        let io_stats = IOStatsContext::new("MicroPartition::eval_expression_list");

        let expected_schema = infer_schema(exprs, &self.schema)?;

        let tables = self.tables_or_read(io_stats)?;

        let bound_exprs = exprs
            .iter()
            .map(|expr| BoundExpr::try_new(expr.clone(), &self.schema))
            .try_collect::<Vec<_>>()?;

        let evaluated_tables: Vec<_> = tables
            .iter()
            .map(|table| table.eval_expression_list(&bound_exprs))
            .try_collect()?;

        let eval_stats = self
            .statistics
            .as_ref()
            .map(|table_statistics| table_statistics.eval_expression_list(&bound_exprs))
            .transpose()?;

        Ok(Self::new_loaded(
            expected_schema.into(),
            Arc::new(evaluated_tables),
            eval_stats,
        ))
    }

    pub async fn par_eval_expression_list(
        &self,
        exprs: &[ExprRef],
        num_parallel_tasks: usize,
    ) -> DaftResult<Self> {
        let io_stats = IOStatsContext::new("MicroPartition::eval_expression_list");

        let expected_schema = infer_schema(exprs, &self.schema)?;

        let tables = self.tables_or_read(io_stats)?;

        let bound_exprs = exprs
            .iter()
            .map(|expr| BoundExpr::try_new(expr.clone(), &self.schema))
            .try_collect::<Vec<_>>()?;

        let evaluated_table_futs = tables
            .iter()
            .map(|table| table.par_eval_expression_list(&bound_exprs, num_parallel_tasks));

        let evaluated_tables = futures::future::try_join_all(evaluated_table_futs).await?;

        let eval_stats = self
            .statistics
            .as_ref()
            .map(|table_statistics| table_statistics.eval_expression_list(&bound_exprs))
            .transpose()?;

        Ok(Self::new_loaded(
            expected_schema.into(),
            Arc::new(evaluated_tables),
            eval_stats,
        ))
    }

    pub fn explode(&self, exprs: &[ExprRef]) -> DaftResult<Self> {
        let io_stats = IOStatsContext::new("MicroPartition::explode");

        let bound_exprs = exprs
            .iter()
            .map(|expr| BoundExpr::try_new(expr.clone(), &self.schema))
            .try_collect::<Vec<_>>()?;

        let tables = self.tables_or_read(io_stats)?;
        let evaluated_tables = tables
            .iter()
            .map(|t| t.explode(&bound_exprs))
            .collect::<DaftResult<Vec<_>>>()?;
        let expected_new_columns = infer_schema(exprs, &self.schema)?;

        let expected_schema = Arc::new(self.schema.non_distinct_union(&expected_new_columns)?);

        let eval_stats = if let Some(stats) = &self.statistics {
            let new_stats = expected_schema
                .field_names()
                .map(|name| {
                    if expected_new_columns.has_field(name) {
                        ColumnRangeStatistics::Missing
                    } else if let [(index, _)] = self.schema.get_fields_with_name(name)[..] {
                        stats[index].clone()
                    } else {
                        ColumnRangeStatistics::Missing
                    }
                })
                .collect();

            Some(TableStatistics::new(new_stats, expected_schema.clone()))
        } else {
            None
        };

        Ok(Self::new_loaded(
            expected_schema,
            Arc::new(evaluated_tables),
            eval_stats,
        ))
    }
}
