export const SHELL_MODES = Object.freeze(["globo", "floorplanner", "render", "walk"]);
export class ModeRegistry {
  constructor() { this.modes = new Map(); this.active = null; }
  register(mode) {
    if (!SHELL_MODES.includes(mode?.id)) throw new Error(`Modo inválido: ${mode?.id}`);
    if (this.modes.has(mode.id)) throw new Error(`Modo duplicado: ${mode.id}`);
    for (const method of ["mount", "activate", "deactivate", "dispose"]) if (typeof mode[method] !== "function") throw new Error(`${mode.id} sem ${method}()`);
    this.modes.set(mode.id, mode); return mode;
  }
  get(id) { return this.modes.get(id) || null; }
  list() { return [...this.modes.values()]; }
}
