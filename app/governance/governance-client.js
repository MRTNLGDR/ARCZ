import { LocalApiClient } from "../core/api-client.js";
export class GovernanceClient {
  constructor({ api = new LocalApiClient() } = {}) { this.api = api; }
  snapshot() { return this.api.json("/api/governance/snapshot"); }
}
