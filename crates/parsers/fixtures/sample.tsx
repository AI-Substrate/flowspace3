import type { ReactNode } from "react";

interface CardProps {
  title: string;
  children?: ReactNode;
}

export function Card({ title, children }: CardProps) {
  const local = () => title;
  return (
    <section data-title={local()}>
      <header><h1>{title}</h1></header>
      <article>{children ?? <span>empty</span>}</article>
    </section>
  );
}
