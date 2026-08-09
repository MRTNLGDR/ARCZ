import { GovernanceClient } from "./governance-client.js";

function n(tag, cls = "", text = "") {
  const x = document.createElement(tag);
  if (cls) x.className = cls;
  if (text) x.textContent = text;
  return x;
}

function card(label, ready, detail = "") {
  const row = n("div", `arcz-runtime-row ${ready ? "is-ready" : "is-blocked"}`);
  const state = n("span", "arcz-runtime-state", ready ? "PRONTO" : "BLOQUEADO");
  const body = n("div", "arcz-runtime-copy");
  body.append(n("strong", "", label));
  if (detail) body.append(n("span", "", detail));
  row.append(state, body);
  return row;
}

function installedModels(value) {
  if (!Array.isArray(value)) return [];
  return value.filter(model => model?.status?.installed === true);
}

function aedifexDetail(value) {
  if (!value || typeof value !== "object") return "Status indisponível";
  const runtime = value.runtime || {};
  const build = value.build || {};
  const parts = [];
  if (build.upstream_commit) parts.push(`SHA ${String(build.upstream_commit).slice(0, 10)}`);
  if (runtime.healthy === true) parts.push("sidecar saudável");
  else if (runtime.error || runtime.health?.error) parts.push(String(runtime.error || runtime.health?.error));
  if (runtime.authenticated_tool_bridge === true) parts.push("MCP autenticado");
  return parts.join(" · ") || "Build/sidecar ainda não prontos";
}

export class GovernancePanel {
  constructor({ client = new GovernanceClient() } = {}) {
    this.client = client;
  }

  async mount(host) {
    this.host = host;
    const refresh = n("button", "arcz-button", "Atualizar diagnóstico");
    refresh.type = "button";
    this.body = n("div", "arcz-stack");
    host.append(refresh, this.body);
    refresh.addEventListener("click", () => this.refresh());
    await this.refresh();
  }

  async refresh() {
    this.body.replaceChildren(n("div", "arcz-panel-state", "Lendo runtime local real…"));
    const [governance, runtime] = await Promise.allSettled([
      this.client.snapshot(),
      this.client.runtime(),
    ]);
    this.body.replaceChildren();

    const runtimeSection = n("section", "arcz-runtime-truth");
    runtimeSection.append(n("h3", "", "Runtime local"));
    if (runtime.status === "fulfilled") {
      const values = runtime.value;
      const health = values.health?.ok ? values.health.value : null;
      const healthReady = Boolean(health?.ok) && health?.network_mode === "offline_strict";
      runtimeSection.append(card(
        "API + política de rede",
        healthReady,
        health
          ? `API v${health.api || "?"} · ${health.network_mode || "modo desconhecido"} · ${(health.job_kinds || []).length} workers`
          : values.health?.error || "Health indisponível",
      ));

      const aedifex = values.aedifex?.ok ? values.aedifex.value : null;
      runtimeSection.append(card(
        "Modelador Aedifex",
        Boolean(aedifex?.ready),
        aedifex ? aedifexDetail(aedifex) : values.aedifex?.error || "Status indisponível",
      ));

      const models = values.models?.ok ? values.models.value : null;
      const installed = installedModels(models);
      runtimeSection.append(card(
        "IA local",
        installed.length > 0,
        Array.isArray(models)
          ? `${installed.length}/${models.length} modelos com pesos verificados`
          : values.models?.error || "Registro de modelos indisponível",
      ));

      const diagnostics = values.diagnostics?.ok ? values.diagnostics.value : null;
      const diagnosticReady = Boolean(diagnostics) && diagnostics?.ok !== false;
      runtimeSection.append(card(
        "Diagnóstico de dependências",
        diagnosticReady,
        diagnostics
          ? `fonte ${diagnostics.schema_version ? `v${diagnostics.schema_version}` : "local"}`
          : values.diagnostics?.error || "Diagnóstico indisponível",
      ));
    } else {
      runtimeSection.append(card("Runtime local", false, runtime.reason?.message || String(runtime.reason)));
    }
    this.body.append(runtimeSection);

    if (governance.status === "fulfilled") {
      const s = governance.value;
      this.body.append(
        n("h3", "", "Governança"),
        n("div", "arcz-metric", `Estado ${s.state} · ${s.summary.progressPercent.toFixed(1)}%`),
        n("div", "arcz-panel-state", `${s.summary.doneTasks}/${s.summary.totalTasks} tarefas · ${s.summary.openAlerts} alertas`),
      );
      const tasks = n("div", "arcz-list");
      for (const t of s.tasks.filter(x => x.status !== "DONE").slice(0, 12)) {
        tasks.append(n("div", "arcz-governance-row", `${t.id} · ${t.title}`));
      }
      this.body.append(n("h3", "", "Pendências reais"), tasks);
      const alerts = n("div", "arcz-list");
      for (const a of s.alerts.filter(x => x.status === "OPEN").slice(0, 10)) {
        alerts.append(n("div", `arcz-alert arcz-alert--${a.severity.toLowerCase()}`, `${a.id} · ${a.fact}`));
      }
      this.body.append(n("h3", "", "Alertas"), alerts);
    } else {
      this.body.append(n("div", "arcz-panel-error", governance.reason?.message || String(governance.reason)));
    }
  }
}
