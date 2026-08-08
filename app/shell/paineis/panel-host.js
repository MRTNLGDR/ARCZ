export class PanelHost {
  constructor(element) { if (!(element instanceof Element)) throw new TypeError("PanelHost exige Element"); this.element=element; this.current=null; }
  show(id, node) { if (this.current?.id===id) return node; this.element.replaceChildren(node); this.current={id,node}; return node; }
  clear() { this.element.replaceChildren(); this.current=null; }
}
