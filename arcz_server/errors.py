from __future__ import annotations
from dataclasses import dataclass, field
from typing import Any
import uuid

@dataclass(slots=True)
class ApiError(Exception):
    code: str
    message: str
    status: int = 400
    retryable: bool = False
    details: dict[str, Any] = field(default_factory=dict)
    trace_id: str = field(default_factory=lambda: uuid.uuid4().hex)

    def __post_init__(self) -> None:
        Exception.__init__(self, self.message)
        if not self.code or not self.code.replace("_", "").isalnum() or self.code.upper() != self.code:
            raise ValueError(f"invalid error code: {self.code!r}")

    def payload(self) -> dict[str, Any]:
        return {"error": {"code": self.code, "message": self.message, "retryable": self.retryable,
                          "details": self.details, "trace_id": self.trace_id}}

    # Compatibility with the legacy local HTTP gateway. New code should use
    # ``payload()``, but keeping this alias prevents an error handler from
    # crashing while it is trying to report the original structured error.
    def to_dict(self) -> dict[str, Any]:
        return self.payload()


def as_api_error(error: BaseException, *, default_code: str = "INTERNAL_ERROR") -> ApiError:
    if isinstance(error, ApiError):
        return error
    return ApiError(default_code, str(error) or error.__class__.__name__, status=500, retryable=False,
                    details={"type": error.__class__.__name__})
