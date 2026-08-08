import { validateRenderPassPlan } from "./pass-plan.js";
export class LocalDiffusionClient {
  constructor({ localAI }) { if(!localAI?.request)throw new Error("LocalAIClient obrigatório");this.localAI=localAI; }
  async enhance(plan,{mode="material enhancement",modelId=null,signal}={}) {
    validateRenderPassPlan(plan);
    return this.localAI.request("render-diffusion", { mode, ...plan }, { modelId, signal });
  }
}
