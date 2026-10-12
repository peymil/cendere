/** Wipe comparison: original on the left, result on the right, with zoom and pan. */
export class Compare {
  private split = 0.5;
  private scale = 1;
  private tx = 0;
  private ty = 0;
  private drag: { mode: "split" | "pan"; x: number; y: number; tx: number; ty: number } | null = null;

  constructor(
    private stage: HTMLElement,
    private before: HTMLImageElement,
    private after: HTMLImageElement,
    private line: HTMLElement,
  ) {
    this.stage.addEventListener("pointerdown", (e) => this.onDown(e));
    window.addEventListener("pointermove", (e) => this.onMove(e));
    window.addEventListener("pointerup", () => (this.drag = null));
    this.stage.addEventListener("wheel", (e) => this.onWheel(e), { passive: false });
    this.stage.addEventListener("dblclick", () => this.reset());
  }

  setImages(beforeSrc: string, afterSrc: string): void {
    this.before.src = beforeSrc;
    this.after.src = afterSrc;
    this.reset();
  }

  reset(): void {
    this.scale = 1;
    this.tx = 0;
    this.ty = 0;
    this.split = 0.5;
    this.render();
  }

  private onDown(e: PointerEvent): void {
    const rect = this.stage.getBoundingClientRect();
    const nearLine = Math.abs(e.clientX - (rect.left + this.split * rect.width)) < 12;
    this.drag = nearLine
      ? { mode: "split", x: e.clientX, y: e.clientY, tx: this.tx, ty: this.ty }
      : { mode: "pan", x: e.clientX, y: e.clientY, tx: this.tx, ty: this.ty };
    if (!nearLine) this.stage.setPointerCapture(e.pointerId);
  }

  private onMove(e: PointerEvent): void {
    if (!this.drag) return;
    const rect = this.stage.getBoundingClientRect();
    if (this.drag.mode === "split") {
      this.split = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    } else {
      this.tx = this.drag.tx + (e.clientX - this.drag.x);
      this.ty = this.drag.ty + (e.clientY - this.drag.y);
    }
    this.render();
  }

  private onWheel(e: WheelEvent): void {
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
    const next = Math.min(20, Math.max(0.2, this.scale * factor));
    const rect = this.stage.getBoundingClientRect();
    const cx = e.clientX - rect.left - rect.width / 2;
    const cy = e.clientY - rect.top - rect.height / 2;
    this.tx = cx - (cx - this.tx) * (next / this.scale);
    this.ty = cy - (cy - this.ty) * (next / this.scale);
    this.scale = next;
    this.render();
  }

  private render(): void {
    const t = `translate(${this.tx}px, ${this.ty}px) scale(${this.scale})`;
    this.before.style.transform = t;
    this.after.style.transform = t;
    const pct = this.split * 100;
    this.after.style.clipPath = `inset(0 0 0 ${pct}%)`;
    this.line.style.left = `${pct}%`;
  }
}
