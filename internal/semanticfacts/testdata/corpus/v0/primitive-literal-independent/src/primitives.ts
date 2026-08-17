type LiteralUnion = "ready" | 42 | true | 42n | null | undefined;
type Unsupported = { value: string };

declare const literalUnion: LiteralUnion;
declare const booleanPrimitive: boolean;
declare const stringPrimitive: string;
declare const numberPrimitive: number;
declare const bigintPrimitive: bigint;
declare const unsupported: Unsupported;

const contextualString: string = "context";
const contextualNumber: number = 7;
const contextualBoolean: boolean = false;
const contextualBigint: bigint = 7n;
const contextualNull: null = null;

literalUnion;
booleanPrimitive;
stringPrimitive;
numberPrimitive;
bigintPrimitive;
unsupported;

const duplicate = 1;
const duplicate = 2;
