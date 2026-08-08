import json
from pathlib import Path

from arcz_server.schema_validation import SchemaRegistry


ROOT = Path(__file__).resolve().parents[1]


def test_prompt_and_upscale_tasks_are_installable_by_schema() -> None:
    schema = json.loads((ROOT / "schemas/ai-model-manifest.schema.json").read_text(encoding="utf-8"))
    tasks = set(schema["properties"]["task"]["enum"])
    assert {"chat.global", "prompt.enhance", "prompt.translate", "render-diffusion", "upscale"} <= tasks


def test_prompt_task_manifest_validates() -> None:
    registry = SchemaRegistry(ROOT / "schemas")
    for task in ("chat.global", "prompt.enhance", "prompt.translate", "upscale"):
        registry.validate(
            "ai-model-manifest.schema.json",
            {
                "schema_version": 1,
                "id": f"arcz-{task.replace('.', '-')}",
                "version": "1.0.0",
                "task": task,
                "backend": "command",
                "license": "LOCAL-USER-SUPPLIED",
                "files": [],
                "requirements": {"ram_mb": 0, "vram_mb": 0, "devices": ["cpu"]},
                "input_contract": {},
                "output_contract": {},
                "fallback": "disabled",
                "command": ["local-adapter", "{input}", "{output}"],
            },
        )
