type Deep<T> = T extends string
  ? { value: T; next: Deep<`${T}-next`> }
  : never;

interface SharedNode<T> {
  value: T;
  next?: SharedNode<T>;
}

function inspect(node: SharedNode<string>): string;
function inspect(node: SharedNode<number>): number;
function inspect(node: SharedNode<string> | SharedNode<number>) {
  if (typeof node.value === "string") {
    return node.value;
  }
  return node.value;
}

declare const root: SharedNode<string>;
declare const pressure: Deep<"root">;

inspect(root);
root;
pressure;
