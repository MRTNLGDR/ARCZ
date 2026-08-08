import { LocalApiClient } from "../core/api-client.js";

export class ChatClient {
  constructor({ api = new LocalApiClient() } = {}) { this.api = api; }
  listSessions() { return this.api.json("/api/v2/chat/sessions"); }
  createSession(payload) { return this.api.json("/api/v2/chat/sessions", { method: "POST", body: payload }); }
  getSession(id) { return this.api.json(`/api/v2/chat/sessions/${encodeURIComponent(id)}`); }
  append(id, payload) {
    return this.api.json(`/api/v2/chat/sessions/${encodeURIComponent(id)}/messages`, { method: "POST", body: payload });
  }
  respond(id, payload) {
    return this.api.json(`/api/v2/chat/sessions/${encodeURIComponent(id)}/respond`, { method: "POST", body: payload });
  }
  continue(id, payload = {}) {
    return this.api.json(`/api/v2/chat/sessions/${encodeURIComponent(id)}/continue`, { method: "POST", body: payload });
  }
  tools() { return this.api.json("/api/v2/chat/tools"); }
  toolRuns(sessionId, status = "") {
    const query = new URLSearchParams();
    if (sessionId) query.set("session_id", sessionId);
    if (status) query.set("status", status);
    return this.api.json(`/api/v2/chat/tool-runs?${query.toString()}`);
  }
  toolRun(id) { return this.api.json(`/api/v2/chat/tool-runs/${encodeURIComponent(id)}`); }
  approveToolRun(id, expectedRevision) {
    const body = {};
    if (Number.isInteger(expectedRevision) && expectedRevision >= 0) body.expected_revision = expectedRevision;
    return this.api.json(`/api/v2/chat/tool-runs/${encodeURIComponent(id)}/approve`, { method: "POST", body });
  }
  rejectToolRun(id, reason = "explicit_user_rejection") {
    return this.api.json(`/api/v2/chat/tool-runs/${encodeURIComponent(id)}/reject`, {
      method: "POST", body: { reason },
    });
  }
  invoke(name, args = {}, context = {}) {
    return this.api.json("/api/v2/chat/tools/invoke", {
      method: "POST", body: { name, arguments: args, context },
    });
  }
}
