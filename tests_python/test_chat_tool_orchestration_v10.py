from __future__ import annotations

from pathlib import Path
from typing import Any

import pytest

from arcz_server.chat_workspace import ChatWorkspace
from arcz_server.errors import ApiError
from arcz_server.reference_media import ReferenceMediaStore
from arcz_server.schema_validation import SchemaRegistry


ROOT = Path(__file__).resolve().parents[1]


class QueueAI:
    def __init__(self, outputs: list[dict[str, Any]]):
        self.outputs = list(outputs)
        self.requests: list[dict[str, Any]] = []

    def request(self, task: str, payload: dict[str, Any], **_: Any) -> dict[str, Any]:
        assert task == "chat.global"
        self.requests.append(payload)
        if not self.outputs:
            raise AssertionError("modelo fake de teste sem saída configurada")
        return {
            "model": {"id": "test-local", "version": "1"},
            "cache_key": f"cache-{len(self.requests)}",
            "result": self.outputs.pop(0),
        }


def workspace(tmp_path: Path, outputs: list[dict[str, Any]]) -> tuple[ChatWorkspace, QueueAI]:
    schemas = SchemaRegistry(ROOT / "schemas")
    ai = QueueAI(outputs)
    media = ReferenceMediaStore(tmp_path, schemas)
    return ChatWorkspace(tmp_path, schemas, ai, media), ai


def descriptor(name: str, side_effect: str = "read", requires_approval: bool = False) -> dict[str, Any]:
    return {
        "name": name,
        "description": name,
        "inputSchema": {"type": "object"},
        "side_effect": side_effect,
        "requires_approval": requires_approval,
        "available": True,
    }


def create_session(chat: ChatWorkspace, *, revision: int = 3) -> dict[str, Any]:
    return chat.create_session({
        "title": "Teste",
        "scope": "floorplanner",
        "language": "pt-BR",
        "context": {
            "floorplanner_project_id": "project-1",
            "region_id": "region-1",
            "expected_revision": revision,
        },
    })


def test_read_tool_executes_and_model_continues_from_real_tool_message(tmp_path: Path) -> None:
    chat, ai = workspace(tmp_path, [
        {"content": "Vou consultar.", "tool_calls": [{
            "id": "call-read", "name": "arcz.test.read", "arguments": {"value": 7},
        }]},
        {"content": "A leitura real retornou 14.", "tool_calls": []},
    ])
    session = create_session(chat)
    calls: list[tuple[str, dict[str, Any], dict[str, Any]]] = []

    def invoke(name: str, arguments: dict[str, Any], context: dict[str, Any]) -> Any:
        calls.append((name, arguments, context))
        return {"answer": arguments["value"] * 2}

    result = chat.respond(
        session["id"],
        {"content": "Consulte", "metadata": {"expected_revision": 3}},
        tool_catalog=[descriptor("arcz.test.read")],
        invoke_tool=invoke,
    )

    assert len(result["assistant_messages"]) == 2
    assert result["assistant"]["content"] == "A leitura real retornou 14."
    assert calls == [("arcz.test.read", {"value": 7}, {
        "project_id": "project-1", "expected_revision": 3,
        "dry_run": True, "approval_id": None,
    })]
    runs = chat.list_tool_runs(session_id=session["id"])
    assert len(runs) == 1
    assert runs[0]["status"] == "SUCCEEDED"
    assert runs[0]["result"] == {"answer": 14}
    persisted = chat.get_session(session["id"])
    assert [item["role"] for item in persisted["messages"]] == ["user", "assistant", "tool", "assistant"]
    assert ai.requests[1]["messages"][-1]["role"] == "tool"


def test_mutation_is_previewed_then_committed_only_after_revision_bound_approval(tmp_path: Path) -> None:
    chat, _ = workspace(tmp_path, [{
        "content": "Preparei a parede.",
        "tool_calls": [{"id": "call-wall", "name": "aedifex.create_wall", "arguments": {
            "levelId": "level-1", "start": [0, 0], "end": [5, 0],
        }}],
    }])
    session = create_session(chat, revision=3)
    contexts: list[dict[str, Any]] = []

    def invoke(name: str, arguments: dict[str, Any], context: dict[str, Any]) -> Any:
        contexts.append(dict(context))
        if context["dry_run"]:
            return {
                "name": name, "dry_run": True, "changed": True, "expected_revision": 3,
                "diff": {"created": ["wall-1"], "updated": [], "deleted": []},
            }
        return {
            "name": name, "dry_run": False, "changed": True,
            "expected_revision": 3, "current_revision": 4,
            "diff": {"created": ["wall-1"], "updated": [], "deleted": []},
        }

    response = chat.respond(
        session["id"],
        {"content": "Crie parede", "metadata": {"expected_revision": 3}},
        tool_catalog=[descriptor("aedifex.create_wall", "mutate", True)],
        invoke_tool=invoke,
    )
    call = response["assistant"]["tool_calls"][0]
    assert response["pending_approval"] is True
    assert call["status"] == "AWAITING_APPROVAL"
    assert call["preview"]["diff"]["created"] == ["wall-1"]
    assert len(contexts) == 1 and contexts[0]["dry_run"] is True

    with pytest.raises(ApiError) as conflict:
        chat.approve_tool_run(call["run_id"], {"expected_revision": 2}, invoke_tool=invoke)
    assert conflict.value.code == "CHAT_TOOL_APPROVAL_REVISION_MISMATCH"
    assert len(contexts) == 1

    committed = chat.approve_tool_run(call["run_id"], {"expected_revision": 3}, invoke_tool=invoke)
    assert committed["tool_run"]["status"] == "SUCCEEDED"
    assert committed["result"]["current_revision"] == 4
    assert contexts[-1]["dry_run"] is False
    assert contexts[-1]["approval_id"]
    assert chat.get_tool_run(call["run_id"])["approval_id"] == contexts[-1]["approval_id"]
    persisted = chat.get_session(session["id"])
    tool_call = persisted["messages"][1]["tool_calls"][0]
    assert tool_call["status"] == "SUCCEEDED"
    assert persisted["messages"][-1]["role"] == "tool"


def test_rejection_never_calls_mutating_execution(tmp_path: Path) -> None:
    chat, _ = workspace(tmp_path, [{
        "content": "Preview pronto.",
        "tool_calls": [{"id": "call-delete", "name": "aedifex.delete_node", "arguments": {"id": "wall-1"}}],
    }])
    session = create_session(chat)
    executions = 0

    def invoke(_: str, __: dict[str, Any], context: dict[str, Any]) -> Any:
        nonlocal executions
        if not context["dry_run"]:
            executions += 1
        return {"dry_run": True, "changed": True, "expected_revision": 3,
                "diff": {"created": [], "updated": [], "deleted": ["wall-1"]}}

    response = chat.respond(
        session["id"], {"content": "Apague"},
        tool_catalog=[descriptor("aedifex.delete_node", "destructive", True)],
        invoke_tool=invoke,
    )
    run_id = response["assistant"]["tool_calls"][0]["run_id"]
    rejected = chat.reject_tool_run(run_id, {"reason": "usuário mudou de ideia"})
    assert rejected["tool_run"]["status"] == "REJECTED"
    assert executions == 0


def test_session_project_identity_cannot_be_replaced_by_message_metadata(tmp_path: Path) -> None:
    chat, _ = workspace(tmp_path, [{"content": "ok", "tool_calls": []}])
    session = create_session(chat)
    with pytest.raises(ApiError) as captured:
        chat.respond(
            session["id"],
            {"content": "Teste", "metadata": {"floorplanner_project_id": "other-project"}},
            tool_catalog=[], invoke_tool=lambda *_: {},
        )
    assert captured.value.code == "CHAT_TOOL_PROJECT_CONTEXT_CONFLICT"
