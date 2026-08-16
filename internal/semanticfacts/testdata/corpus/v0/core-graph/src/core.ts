type Primitive = string | number | boolean | bigint | symbol | null | undefined;
type Literal = "ready" | 42 | true;
type Pair<T = string> = readonly [value: T, count?: number, ...flags: boolean[]];
type Shape = { title: string } & { count: number };

interface RecursiveNode {
  primitive: Primitive;
  literal: Literal;
  pair: Pair;
  next?: RecursiveNode;
}

class Store<T extends object = Shape> {
  constructor(readonly value: T) {}
  read(): T {
    return this.value;
  }
}

const literal = {
  tag: "ready",
  list: [1, 2],
  pair: ["x", 1, true] as const,
} as const;
declare const node: RecursiveNode;
declare const store: Store;

node;
store;
literal;
