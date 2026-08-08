export const REFERENCE_ROLES = Object.freeze([
  "reference", "style", "composition", "geometry", "material", "lighting",
  "camera", "mask", "negative", "identity", "vegetation", "context",
]);

export function normalizeReferenceRoles(value) {
  const source = Array.isArray(value) ? value : [];
  const result = [...new Set(source.map(item => String(item || "").trim()).filter(Boolean))];
  return result.length ? result : ["reference"];
}

export function previewKind(item) {
  const category = String(item?.category || "");
  const mime = String(item?.mime || "").toLowerCase();
  if (category === "image" && !mime.includes("exr") && !mime.includes("tiff")) return "image";
  if (category === "video") return "video";
  if (category === "audio") return "audio";
  if (mime === "application/pdf") return "pdf";
  return "metadata";
}
