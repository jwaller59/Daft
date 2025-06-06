use std::sync::Arc;

use common_error::DaftResult;
use daft_core::prelude::SchemaRef;
use daft_scan::ScanTask;

use crate::micropartition::{MicroPartition, TableState};

impl MicroPartition {
    #[deprecated(note = "name-referenced columns")]
    /// Casts a `MicroPartition` to a schema.
    ///
    /// Note: this method is deprecated because it maps fields by name, which will not work for schemas with duplicate field names.
    /// It should only be used for scans, and once we support reading files with duplicate column names, we should remove this function.
    pub fn cast_to_schema(&self, schema: SchemaRef) -> DaftResult<Self> {
        let pruned_statistics = self
            .statistics
            .as_ref()
            .map(|stats| {
                #[allow(deprecated)]
                stats.cast_to_schema(&schema)
            })
            .transpose()?;

        let guard = self.state.lock().unwrap();
        match &*guard {
            // Replace schema if Unloaded, which should be applied when data is lazily loaded
            TableState::Unloaded(scan_task) => {
                let maybe_new_scan_task = if scan_task.schema == schema {
                    scan_task.clone()
                } else {
                    Arc::new(ScanTask::new(
                        scan_task.sources.clone(),
                        scan_task.file_format_config.clone(),
                        schema,
                        scan_task.storage_config.clone(),
                        scan_task.pushdowns.clone(),
                        scan_task.generated_fields.clone(),
                    ))
                };
                Ok(Self::new_unloaded(
                    maybe_new_scan_task,
                    self.metadata.clone(),
                    pruned_statistics.expect("Unloaded MicroPartition should have statistics"),
                ))
            }
            // If Tables are already loaded, we map `Table::cast_to_schema` on each Table
            TableState::Loaded(tables) => Ok(Self::new_loaded(
                schema.clone(),
                Arc::new(
                    tables
                        .iter()
                        .map(|tbl| {
                            #[allow(deprecated)]
                            tbl.cast_to_schema(schema.as_ref())
                        })
                        .collect::<DaftResult<Vec<_>>>()?,
                ),
                pruned_statistics,
            )),
        }
    }
}
