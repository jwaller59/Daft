from __future__ import annotations

from abc import abstractmethod
from typing import TYPE_CHECKING, Any, ClassVar, Generic, Iterator, Literal

from daft.runners.partitioning import (
    MaterializedResult,
    PartitionCacheEntry,
    PartitionSet,
    PartitionSetCache,
    PartitionT,
)

if TYPE_CHECKING:
    from daft.logical.builder import LogicalPlanBuilder
    from daft.recordbatch import MicroPartition
    from daft.runners.runner_io import RunnerIO

LOCAL_PARTITION_SET_CACHE = PartitionSetCache()


class Runner(Generic[PartitionT]):
    name: ClassVar[Literal["ray", "py", "native"]]

    def __init__(self) -> None:
        self._part_set_cache = self.initialize_partition_set_cache()

    def get_partition_set_from_cache(self, pset_id: str) -> PartitionCacheEntry:
        return self._part_set_cache.get_partition_set(pset_id=pset_id)

    def put_partition_set_into_cache(self, pset: PartitionSet[Any]) -> PartitionCacheEntry:
        return self._part_set_cache.put_partition_set(pset=pset)

    @abstractmethod
    def initialize_partition_set_cache(self) -> PartitionSetCache: ...

    @abstractmethod
    def runner_io(self) -> RunnerIO: ...

    @abstractmethod
    def run(self, builder: LogicalPlanBuilder) -> PartitionCacheEntry: ...

    @abstractmethod
    def run_iter(
        self, builder: LogicalPlanBuilder, results_buffer_size: int | None = None
    ) -> Iterator[MaterializedResult[PartitionT]]:
        """Similar to run(), but yield the individual partitions as they are completed.

        Args:
            builder: the builder for the LogicalPlan that is to be executed
            results_buffer_size: if the plan is executed asynchronously, this is the maximum size of the number of results
                that can be buffered before execution should pause and wait.
        """
        ...

    @abstractmethod
    def run_iter_tables(
        self, builder: LogicalPlanBuilder, results_buffer_size: int | None = None
    ) -> Iterator[MicroPartition]:
        """Similar to run_iter(), but always dereference and yield MicroPartition objects.

        Args:
            builder: the builder for the LogicalPlan that is to be executed
            results_buffer_size: if the plan is executed asynchronously, this is the maximum size of the number of results
                that can be buffered before execution should pause and wait.
        """
        ...
