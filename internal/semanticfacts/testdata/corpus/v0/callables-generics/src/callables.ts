interface Box<T> {
  value: T;
}

interface Factory<T extends object = { value: string }> {
  (input: T): Box<T>;
  (input: string, ...labels: string[]): Box<{ value: string }>;
  new <U extends T = T>(input: U): Factory<U>;
  readonly [key: string]: T;
}

declare const factory: Factory;
declare function identity<T extends string = string>(value: T): T;
const specialized = identity<"ready">;

factory;
specialized;
