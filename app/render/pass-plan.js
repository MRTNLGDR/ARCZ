const REQUIRED_GEOMETRY_PASSES = Object.freeze(["beauty", "depth", "normals", "object_ids"]);
export function validateRenderPassPlan(plan) {
  if (!plan || typeof plan !== "object") throw new TypeError("Render pass plan obrigatório");
  const passes = plan.passes || {};
  for (const name of REQUIRED_GEOMETRY_PASSES) {
    const value = passes[name];
    if (typeof value !== "string" || !value) throw new Error(`Pass obrigatório ausente: ${name}`);
    if (/^(https?:|data:)/i.test(value) || value.split("/").includes("..")) throw new Error(`Pass não local: ${name}`);
  }
  if (!Number.isInteger(plan.width) || !Number.isInteger(plan.height) || plan.width < 1 || plan.height < 1 || plan.width > 16384 || plan.height > 16384) throw new Error("Resolução inválida");
  return plan;
}
