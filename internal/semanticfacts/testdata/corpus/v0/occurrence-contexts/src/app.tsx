import { payload as imported } from "./values";

interface CardProps {
  title: string;
  count?: number;
}

declare namespace JSX {
  interface IntrinsicElements {
    card: CardProps;
  }
}

const config = {
  title: "ready",
  count: 1,
} as const satisfies CardProps;
const rendered = <card title={config.title} count={config.count} />;

function consume(value: string | number) {
  if (typeof value === "string") {
    const contextual: CardProps = { title: value };
    contextual;
    return value;
  }
  return String(value);
}

const asserted = config as CardProps;
imported;
config;
rendered;
asserted;
