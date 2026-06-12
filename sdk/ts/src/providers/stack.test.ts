import { describe, it, expect, vi } from "vitest";
import { MapDataStack } from "./stack";
import { MapDataSource } from "./datasource";

class MockSource implements MapDataSource {
  public id: string;
  public cacheSize: number = 0;
  public cleared: boolean = false;

  constructor(id: string) {
    this.id = id;
  }

  async loadTile(_x: number, _y: number, _z?: number): Promise<void> {
    this.cacheSize++;
  }

  unloadTile(_x: number, _y: number, _z?: number): void {
    this.cacheSize = Math.max(0, this.cacheSize - 1);
  }

  clearCache(): void {
    this.cacheSize = 0;
    this.cleared = true;
  }

  getCacheSize(): number {
    return this.cacheSize;
  }
}

describe("MapDataStack", () => {
  it("should register and retrieve sources", () => {
    const stack = new MapDataStack();
    const source = new MockSource("test");
    stack.registerSource(source);

    expect(stack.getSource("test")).toBe(source);
    expect(stack.getSource("missing")).toBeNull();
  });

  it("should clear all caches", () => {
    const stack = new MapDataStack();
    const s1 = new MockSource("s1");
    const s2 = new MockSource("s2");
    stack.registerSource(s1);
    stack.registerSource(s2);

    s1.cacheSize = 5;
    s2.cacheSize = 3;

    stack.clearCache();
    expect(s1.cleared).toBe(true);
    expect(s2.cleared).toBe(true);
    expect(stack.getCacheSize()).toBe(0);
  });

  it("should return aggregate cache size", () => {
    const stack = new MapDataStack();
    const s1 = new MockSource("s1");
    const s2 = new MockSource("s2");
    stack.registerSource(s1);
    stack.registerSource(s2);

    s1.cacheSize = 4;
    s2.cacheSize = 6;

    expect(stack.getCacheSize()).toBe(10);
  });

  it("should destroy and remove all sources", () => {
    const stack = new MapDataStack();
    const s1 = new MockSource("s1");
    stack.registerSource(s1);

    stack.destroy();
    expect(s1.cleared).toBe(true);
    expect(stack.getSource("s1")).toBeNull();
  });
});
