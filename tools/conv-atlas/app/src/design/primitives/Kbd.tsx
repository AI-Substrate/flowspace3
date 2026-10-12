/** A key cap. Pass one key per Kbd: <Kbd>⌘</Kbd><Kbd>K</Kbd>. */
export function Kbd({ children }: { children: string }) {
  return <kbd className="ca-kbd">{children}</kbd>;
}
