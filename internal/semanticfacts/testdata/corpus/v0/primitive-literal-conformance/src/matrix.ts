import { importedLiteral } from "./exports";
import type { ImportedLiteral } from "./exports";

type LiteralUnion = "ready" | 42 | true | 42n | null | undefined;

const literalDeclaration: "declared" = "declared";
let widenedDeclaration = "widened";
const constAssertion = "const-asserted" as const;
const satisfiesLiteral = "satisfied" satisfies string;
const contextualString: string = "context";
enum Mode {
    Ready = "ready",
}
const enumLikeValue = Mode.Ready;
declare const importedType: ImportedLiteral;
declare const literalUnion: LiteralUnion;
declare const voidValue: void;

literalDeclaration;
"expression";
widenedDeclaration;
constAssertion;
enumLikeValue;
importedLiteral;
importedType;
literalUnion;
null;
undefined;
voidValue;
