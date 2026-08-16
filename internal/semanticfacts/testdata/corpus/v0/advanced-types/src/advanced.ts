export {};

type Uppercase<S extends string> = intrinsic;
type Capitalize<S extends string> = intrinsic;
type Conditional<T> = T extends { value: infer U } ? U : never;
type Mapped<T> = {
  readonly [K in keyof T as `get${Capitalize<string & K>}`]?: T[K];
};
type Indexed<T, K extends keyof T> = T[K];
type Keys<T> = keyof T;
type Template<T extends string> = `event:${Uppercase<T>}`;

const source = { value: "ready", count: 1 } as const;
type Snapshot = typeof source;

Conditional;
Mapped;
Indexed;
Keys;
Template;
Snapshot;
