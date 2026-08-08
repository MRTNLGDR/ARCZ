export class MountOnceRegistry {
  constructor() { this.nodes = new Map(); }
  mount(id, factory, host) {
    if (this.nodes.has(id)) {
      const existing = this.nodes.get(id);
      if (existing.parentNode !== host) host.appendChild(existing);
      return existing;
    }
    const node = factory();
    if (!(node instanceof Element)) throw new TypeError(`Factory de ${id} não retornou Element`);
    if (node.id && document.getElementById(node.id)) throw new Error(`ID global duplicado: ${node.id}`);
    host.appendChild(node); this.nodes.set(id, node); return node;
  }
  unmount(id) { const node=this.nodes.get(id); node?.remove(); return !!node; }
  dispose() { for (const node of this.nodes.values()) node.remove(); this.nodes.clear(); }
}
