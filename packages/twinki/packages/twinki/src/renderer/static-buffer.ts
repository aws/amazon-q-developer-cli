/**
 * Committed scrollback lines, plus a generation counter that changes on every
 * mutation.
 *
 * The renderer skips this content when diffing a frame, which is only sound if
 * it can prove the buffer has not moved since the frame it is comparing
 * against. That proof is the generation: because *every* mutation goes through
 * a method here and every method bumps it, an unchanged generation means an
 * unchanged buffer. Mutating without bumping is unrepresentable.
 *
 * A content comparison would not work: `===` on strings compares by value, so
 * a coincidental match — flushed batches routinely end in a blank spacer line —
 * would wrongly report the buffer unchanged and skip repainting a flush.
 */
export class StaticBuffer {
  private lines: string[] = [];
  /** Changes on every mutation. Compare, never interpret. */
  private gen = 0;

  get generation(): number {
    return this.gen;
  }
  get length(): number {
    return this.lines.length;
  }
  /** Read-only view for row math and frame assembly. */
  get view(): readonly string[] {
    return this.lines;
  }

  /** Returns `lines` with this buffer prepended, without copying the buffer. */
  prepend(lines: string[]): string[] {
    return this.lines.concat(lines);
  }

  append(lines: string[]): void {
    for (const line of lines) this.lines.push(line);
    this.gen++;
  }

  replace(lines: string[]): void {
    this.lines = lines;
    this.gen++;
  }

  clear(): void {
    this.lines = [];
    this.gen++;
  }

  /**
   * Prunes to 75% of `cap` once 10% over, amortizing the cost. Returns whether
   * anything was dropped so the caller can invalidate width-dependent caches.
   */
  trimTo(cap: number): boolean {
    if (this.lines.length <= cap * 1.1) return false;
    this.lines = this.lines.slice(-Math.floor(cap * 0.75));
    this.gen++;
    return true;
  }
}
