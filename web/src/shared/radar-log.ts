export const ENTRY_SEPARATOR = "▸";

/**
 * Fixed-size circular character log. Entries are written as one stream
 * without line breaks. When the grid is full, writing wraps to the top-left
 * and overwrites the oldest characters, clearing a gap ahead of the write
 * position like a radar sweep.
 */
export class RadarLog {
  private readonly cells: string[];
  private position = 0;

  constructor(
    readonly columns: number,
    readonly rows: number,
    private readonly gap: number,
  ) {
    this.cells = new Array<string>(columns * rows).fill(" ");
  }

  write(entry: string): void {
    for (const char of `${ENTRY_SEPARATOR}${entry} `) {
      this.cells[this.position] = char;
      this.position = (this.position + 1) % this.cells.length;
    }
    for (let i = 0; i < this.gap; i += 1) {
      this.cells[(this.position + i) % this.cells.length] = " ";
    }
  }

  /** Grid lines and the write position as an index into the joined text. */
  view(): { lines: string[]; cursor: number } {
    const lines: string[] = [];
    for (let row = 0; row < this.rows; row += 1) {
      lines.push(this.cells.slice(row * this.columns, (row + 1) * this.columns).join(""));
    }
    const cursor = this.position + Math.floor(this.position / this.columns);
    return { lines, cursor };
  }
}
