import type { TileCacheStats } from "./datasource";

export class TileCache<T> {
  private readonly entries = new Map<string, T>();
  private bytes = 0;
  private hits = 0;
  private misses = 0;
  private evictions = 0;

  constructor(
    private capacity: number,
    private readonly dispose: (value: T) => void,
    private readonly sizeOf: (value: T) => number = () => 1,
  ) {
    if (!Number.isInteger(capacity) || capacity <= 0) {
      throw new Error("Tile cache capacity must be a positive integer");
    }
  }

  public get(key: string): T | undefined {
    const value = this.entries.get(key);
    if (value !== undefined) {
      this.hits++;
      this.entries.delete(key);
      this.entries.set(key, value);
    }
    else this.misses++;
    return value;
  }

  public has(key: string): boolean {
    return this.entries.has(key);
  }

  public set(key: string, value: T): void {
    const previous = this.entries.get(key);
    if (previous !== undefined) {
      this.bytes -= this.sizeOf(previous);
      this.dispose(previous);
    }
    this.entries.delete(key);
    this.entries.set(key, value);
    this.bytes += this.sizeOf(value);

    while (this.entries.size > this.capacity) {
      const oldestKey = this.entries.keys().next().value as string | undefined;
      if (oldestKey === undefined) break;
      const oldest = this.entries.get(oldestKey);
      this.entries.delete(oldestKey);
      if (oldest !== undefined) {
        this.bytes -= this.sizeOf(oldest);
        this.evictions++;
        this.dispose(oldest);
      }
    }
  }

  public delete(key: string): void {
    const value = this.entries.get(key);
    if (value !== undefined) {
      this.entries.delete(key);
      this.bytes -= this.sizeOf(value);
      this.dispose(value);
    }
  }

  public clear(): void {
    for (const value of this.entries.values()) this.dispose(value);
    this.entries.clear();
    this.bytes = 0;
  }

  public size(): number {
    return this.entries.size;
  }

  public stats(): TileCacheStats {
    return {
      items: this.entries.size,
      bytes: this.bytes,
      hits: this.hits,
      misses: this.misses,
      evictions: this.evictions,
    };
  }
}

export function throwIfAborted(signal?: AbortSignal): void {
  if (signal?.aborted) throw new DOMException("The tile request was aborted", "AbortError");
}

export async function retryTileRequest<T>(
  operation: () => Promise<T>,
  maxRetries: number,
  signal?: AbortSignal,
): Promise<T> {
  let attempt = 0;
  while (true) {
    throwIfAborted(signal);
    try {
      return await operation();
    } catch (error) {
      throwIfAborted(signal);
      if (attempt >= maxRetries) throw error;
      attempt++;
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(resolve, 50 * (2 ** (attempt - 1)));
        signal?.addEventListener("abort", () => {
          clearTimeout(timer);
          reject(new DOMException("The tile request was aborted", "AbortError"));
        }, { once: true });
      });
    }
  }
}
