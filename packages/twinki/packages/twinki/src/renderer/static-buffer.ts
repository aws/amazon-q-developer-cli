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
  private readonly frameBuffers = [
    { generation: -1, lines: [] as string[] },
    { generation: -1, lines: [] as string[] },
  ];
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

  /** Composes into a frame that is not the current diff shadow. */
  compose(
    liveLines: string[],
    previousLines: string[]
  ): { lines: string[]; copiedPrefixLines: number } {
    const target =
      this.frameBuffers[0]!.lines === previousLines
        ? this.frameBuffers[1]!
        : this.frameBuffers[0]!;
    let copiedPrefixLines = 0;

    if (target.generation !== this.gen) {
      target.lines.length = this.lines.length;
      for (let index = 0; index < this.lines.length; index++) {
        target.lines[index] = this.lines[index]!;
      }
      target.generation = this.gen;
      copiedPrefixLines = this.lines.length;
    }

    const staticLength = this.lines.length;
    target.lines.length = staticLength + liveLines.length;
    for (let index = 0; index < liveLines.length; index++) {
      target.lines[staticLength + index] = liveLines[index]!;
    }

    return { lines: target.lines, copiedPrefixLines };
  }

  append(lines: string[]): void {
    for (const line of lines) this.lines.push(line);
    this.gen++;
  }

  replace(lines: string[]): void {
    if (lines.length === 0) {
      this.clear();
      return;
    }
    this.lines = lines;
    this.gen++;
  }

  clear(): void {
    this.lines = [];
    for (const buffer of this.frameBuffers) {
      buffer.lines = [];
      buffer.generation = -1;
    }
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
