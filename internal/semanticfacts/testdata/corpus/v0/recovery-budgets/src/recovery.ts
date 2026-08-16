type Broken<T> = T extends string ? Missing<T> : never;
type Deep<T> = T extends string
  ? { value: T; next: Deep<`${T}-next`> }
  : never;

const recovered: Broken<string> = { invalid: true };
const syntax = ;
declare const deep: Deep<"root">;

recovered;
syntax;
deep;
