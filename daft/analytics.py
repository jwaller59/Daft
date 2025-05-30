from __future__ import annotations

import atexit
import base64
import dataclasses
import datetime
import functools
import json
import logging
import os
import platform
import random
import time
import urllib.error
import urllib.request
from typing import Any, Callable

_ANALYTICS_CLIENT = None
_WRITE_KEY = "ZU2LLq6HFW0kMEY6TiGZoGnRzogXBUwa"
_SEGMENT_BATCH_ENDPOINT = "https://api.segment.io/v1/batch"


logger = logging.getLogger(__name__)


@dataclasses.dataclass(frozen=True)
class AnalyticsEvent:
    session_id: str
    event_name: str
    event_time: datetime.datetime
    data: dict[str, Any]


def _get_session_key() -> str:
    # Restrict the cardinality of keys to 800
    return f"anon-{random.randint(1, 800)}"


def _build_segment_batch_payload(
    events: list[AnalyticsEvent], daft_version: str, daft_build_type: str
) -> dict[str, Any]:
    return {
        "batch": [
            {
                "type": "track",
                "anonymousId": event.session_id,
                "event": event.event_name,
                "properties": event.data,
                "timestamp": event.event_time.isoformat(),
                "context": {
                    "app": {
                        "name": "daft",
                        "version": daft_version,
                        "build": daft_build_type,
                    },
                },
            }
            for event in events
        ],
    }


def _post_segment_track_endpoint(analytics_client: AnalyticsClient, payload: dict[str, Any]) -> None:
    """Posts a batch of JSON data to Segment."""
    req = urllib.request.Request(
        _SEGMENT_BATCH_ENDPOINT,
        method="POST",
        headers={
            "Content-Type": "application/json",
            "User-Agent": "daft-analytics",
            "Authorization": f"Basic {base64.b64encode(f'{_WRITE_KEY}:'.encode()).decode('utf-8')}",
        },
        data=json.dumps(payload).encode("utf-8"),
    )
    try:
        resp = urllib.request.urlopen(
            req,
            timeout=1,
        )
    except urllib.error.URLError:
        analytics_client._is_active = False
    if resp.status != 200:
        raise RuntimeError(f"HTTP request to segment returned status code: {resp.status}")


class AnalyticsClient:
    """Non-threadsafe client for sending analytics events, which is a singleton for each Python process."""

    def __init__(
        self,
        daft_version: str,
        daft_build_type: str,
        enabled: bool,
        publish_payload_function: Callable[[AnalyticsClient, dict[str, Any]], None] = _post_segment_track_endpoint,
        buffer_capacity: int = 100,
    ) -> None:
        self._is_active = enabled
        self._daft_version = daft_version
        self._daft_build_type = daft_build_type
        self._session_key = _get_session_key()

        # Function to publish a payload to Segment
        self._publish = publish_payload_function

        # Buffer for events to be sent to Segment
        self._buffer_capacity = buffer_capacity
        self._buffer: list[AnalyticsEvent] = []

    def _append_to_log(self, event_name: str, data: dict[str, Any]) -> None:
        if self._is_active:
            self._buffer.append(
                AnalyticsEvent(
                    session_id=self._session_key,
                    event_name=event_name,
                    event_time=datetime.datetime.now(datetime.timezone.utc),
                    data=data,
                )
            )
            if len(self._buffer) >= self._buffer_capacity:
                self._flush()
        else:
            logger.debug("Analytics is disabled; not sending data to segment")

    def _flush(self) -> None:
        try:
            payload = _build_segment_batch_payload(self._buffer, self._daft_version, self._daft_build_type)
            self._publish(self, payload)
        except Exception as e:
            # No-op on failure to avoid crashing the program - TODO: add retries for more robust logging
            logger.debug("Error in analytics publisher thread: %s", e)
        finally:
            self._buffer = []

    def track_import(self) -> None:
        self._append_to_log(
            "Imported Daft",
            {
                "platform": platform.platform(),
                "python_version": platform.python_version(),
                "DAFT_ANALYTICS_ENABLED": os.getenv("DAFT_ANALYTICS_ENABLED"),
            },
        )

    def track_df_method_call(self, method_name: str, duration_seconds: float, error: str | None = None) -> None:
        optionals = {}
        if error is not None:
            optionals["error"] = error
        self._append_to_log(
            "DataFrame Method Call",
            {
                "method_name": method_name,
                "duration_seconds": duration_seconds,
                **optionals,
            },
        )

    def track_fn_call(self, fn_name: str, duration_seconds: float, error: str | None = None) -> None:
        optionals = {}
        if error is not None:
            optionals["error"] = error
        self._append_to_log(
            "daft API Call",
            {
                "fn_name": fn_name,
                "duration_seconds": duration_seconds,
                **optionals,
            },
        )


def init_analytics(daft_version: str, daft_build_type: str, user_opted_out: bool) -> AnalyticsClient:
    """Initialize the analytics module.

    Returns:
        AnalyticsClient: initialized singleton AnalyticsClient
    """
    enabled = (not user_opted_out) and daft_build_type != "dev"

    global _ANALYTICS_CLIENT

    if _ANALYTICS_CLIENT is not None:
        return _ANALYTICS_CLIENT

    _ANALYTICS_CLIENT = AnalyticsClient(daft_version, daft_build_type, enabled)
    atexit.register(_ANALYTICS_CLIENT._flush)
    return _ANALYTICS_CLIENT


def time_df_method(method: Callable[..., Any]) -> Callable[..., Any]:
    """Decorator to track metrics about Dataframe method calls."""

    @functools.wraps(method)
    def tracked_method(*args: Any, **kwargs: Any) -> Any:
        if _ANALYTICS_CLIENT is None:
            return method(*args, **kwargs)

        start = time.time()
        try:
            result = method(*args, **kwargs)
        except Exception as e:
            _ANALYTICS_CLIENT.track_df_method_call(
                method_name=method.__name__, duration_seconds=time.time() - start, error=str(type(e).__name__)
            )
            raise

        _ANALYTICS_CLIENT.track_df_method_call(
            method_name=method.__name__,
            duration_seconds=time.time() - start,
        )
        return result

    return tracked_method


def time_func(fn: Callable[..., Any]) -> Callable[..., Any]:
    """Decorator to track metrics for daft API calls."""

    @functools.wraps(fn)
    def tracked_fn(*args: Any, **kwargs: Any) -> Any:
        __tracebackhide__ = True
        if _ANALYTICS_CLIENT is None:
            return fn(*args, **kwargs)
        start = time.time()
        try:
            result = fn(*args, **kwargs)
        except Exception as e:
            _ANALYTICS_CLIENT.track_fn_call(
                fn_name=fn.__name__, duration_seconds=time.time() - start, error=str(type(e).__name__)
            )
            raise

        _ANALYTICS_CLIENT.track_fn_call(
            fn_name=fn.__name__,
            duration_seconds=time.time() - start,
        )
        return result

    return tracked_fn
