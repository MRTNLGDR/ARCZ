export class ShellNavbar {
  constructor({ element, modes, onSelect }) { this.element=element; this.modes=modes; this.onSelect=onSelect; this.buttons=new Map(); }
  mount() {
    this.element.replaceChildren();
    for (const mode of this.modes) {
      const button=document.createElement("button"); button.type="button"; button.dataset.mode=mode.id;
      button.textContent=mode.label || mode.id; button.addEventListener("click",()=>this.onSelect(mode.id));
      this.element.appendChild(button); this.buttons.set(mode.id,button);
    }
  }
  setActive(id) { for (const [key,button] of this.buttons) button.setAttribute("aria-pressed",key===id?"true":"false"); }
  dispose() { this.element.replaceChildren(); this.buttons.clear(); }
}
