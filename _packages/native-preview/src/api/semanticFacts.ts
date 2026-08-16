/** The schema-v1 request accepted by the semantic snapshot API. */
export interface SemanticSnapshotRequest {
    readonly schemaVersion: 1;
    readonly requiredCapabilities?: readonly string[];
    readonly budgets?: SemanticBudgetLimits;
    readonly files?: readonly string[];
    readonly selections?: readonly SemanticSelection[];
}

export interface SemanticSelection {
    readonly file: string;
    /** Zero-based UTF-8 byte offset, inclusive. */
    readonly start: number;
    /** Zero-based UTF-8 byte offset, exclusive. */
    readonly end: number;
}

export interface SemanticSpan {
    readonly start: number;
    readonly end: number;
}

export interface SemanticBudgetLimits {
    readonly maxTypeNodes?: number;
    readonly maxTypeDepth?: number;
}

export interface SemanticBudgetReport {
    readonly limits: Required<SemanticBudgetLimits>;
    readonly typeNodesUsed: number;
    readonly maxTypeDepthObserved: number;
    readonly truncated: boolean;
}

export interface SemanticGraphIssue {
    readonly code: string;
    readonly limit?: number;
}

export type SemanticEntityState = "complete" | "truncated" | "unsupported" | "error";
export type SemanticTypeViewState = "available" | "same-as-actual" | "inapplicable" | "unavailable";

export interface SemanticSnapshotHeader {
    readonly record: "header";
    readonly schemaVersion: 1;
    readonly typeScriptVersion: string;
    readonly typeScriptRevision: string;
    readonly offsetEncoding: "utf8-bytes";
    readonly capabilities: readonly string[];
    readonly budgets: SemanticBudgetReport;
    readonly project: string;
    readonly compilerOptions: Readonly<Record<string, unknown>>;
    readonly diagnosticCount: number;
}

export interface SemanticFileRecord {
    readonly record: "file";
    readonly id: string;
    readonly origin: string;
    readonly selected?: boolean;
    readonly diagnosticCount?: number;
}

export interface SemanticLiteralValue {
    readonly kind: string;
    readonly value: string;
}

export interface SemanticTypeRecord {
    readonly record: "type";
    readonly id: string;
    readonly typeKind: string;
    readonly display: string;
    readonly flags: readonly string[];
    readonly members?: readonly string[];
    readonly symbol?: string;
    readonly target?: string;
    readonly typeArguments?: readonly string[];
    readonly constraint?: string;
    readonly default?: string;
    readonly properties?: readonly string[];
    readonly callSignatures?: readonly string[];
    readonly constructSignatures?: readonly string[];
    readonly indexSignatures?: readonly string[];
    readonly literal?: SemanticLiteralValue;
    readonly array?: { readonly readonly: boolean; };
    readonly tuple?: {
        readonly readonly: boolean;
        readonly elements: readonly { readonly kind: string; readonly label?: string; }[];
    };
    readonly conditional?: {
        readonly checkType: string;
        readonly extendsType: string;
        readonly trueType: string;
        readonly falseType: string;
        readonly inferTypeParameters?: readonly string[];
        readonly distributive?: boolean;
    };
    readonly mapped?: {
        readonly typeParameter: string;
        readonly constraintType: string;
        readonly nameType?: string;
        readonly templateType: string;
        readonly modifiersType?: string;
        readonly readonlyModifier: "add" | "remove" | "preserve";
        readonly optionalModifier: "add" | "remove" | "preserve";
    };
    readonly indexedAccess?: { readonly objectType: string; readonly indexType: string; };
    readonly templateLiteral?: { readonly texts: readonly string[]; readonly types: readonly string[]; };
    readonly substitution?: { readonly baseType: string; readonly constraint: string; };
    readonly state: SemanticEntityState;
    readonly issues?: readonly SemanticGraphIssue[];
    readonly complete: boolean;
    readonly truncated: boolean;
}

export interface SemanticDeclarationRecord {
    readonly record: "declaration";
    readonly id: string;
    readonly file: string;
    readonly span: SemanticSpan;
    readonly syntaxKind: string;
}

export interface SemanticSymbolRecord {
    readonly record: "symbol";
    readonly id: string;
    readonly name: string;
    readonly roles: readonly string[];
    readonly declarations?: readonly string[];
    readonly aliasedSymbol?: string;
    readonly type?: string;
    readonly declaredType?: string;
    readonly members?: readonly string[];
    readonly state: SemanticEntityState;
    readonly issues?: readonly SemanticGraphIssue[];
    readonly complete: boolean;
    readonly truncated: boolean;
}

export interface SemanticSignatureRecord {
    readonly record: "signature";
    readonly id: string;
    readonly signatureKind: string;
    readonly declaration?: string;
    readonly target?: string;
    readonly typeArguments?: readonly string[];
    readonly typeParameters?: readonly string[];
    readonly thisType?: string;
    readonly parameters?: readonly string[];
    readonly minArgumentCount?: number;
    readonly hasRestParameter?: boolean;
    readonly indexKeyType?: string;
    readonly readonly?: boolean;
    readonly returnType: string;
    readonly state: SemanticEntityState;
    readonly issues?: readonly SemanticGraphIssue[];
    readonly complete: boolean;
    readonly truncated: boolean;
}

export interface SemanticFactRecord {
    readonly record: "fact";
    readonly file: string;
    readonly span: SemanticSpan;
    readonly syntaxKind: string;
    readonly actualType: string;
    readonly typeAtLocation: string;
    readonly annotationType?: string;
    readonly inferredType?: string;
    readonly contextualType?: string;
    readonly widenedType?: string;
    readonly apparentType?: string;
    readonly declaredType?: string;
    readonly narrowedType?: string;
    readonly constraintType?: string;
    readonly typeViewStates: {
        readonly actual: SemanticTypeViewState;
        readonly contextual: SemanticTypeViewState;
        readonly widened: SemanticTypeViewState;
        readonly apparent: SemanticTypeViewState;
        readonly declared: SemanticTypeViewState;
    };
    readonly symbol?: string;
    readonly declarations?: readonly string[];
    readonly complete: boolean;
    readonly recovered: boolean;
    readonly truncated: boolean;
}

/** Transport-neutral v0 semantic envelope returned by getSemanticSnapshot. */
export interface SemanticSnapshot {
    readonly header: SemanticSnapshotHeader;
    readonly files: readonly SemanticFileRecord[];
    readonly types: readonly SemanticTypeRecord[];
    readonly declarations: readonly SemanticDeclarationRecord[];
    readonly symbols: readonly SemanticSymbolRecord[];
    readonly signatures: readonly SemanticSignatureRecord[];
    readonly facts: readonly SemanticFactRecord[];
}
