import { useUi } from "./ui";

/** Copy to the clipboard and say so; a refused clipboard says that instead. */
export function copyText(text: string, done: string): void {
  navigator.clipboard.writeText(text).then(
    () => useUi.getState().toast(done),
    () => useUi.getState().toast("The browser refused clipboard access. Select the text and copy it by hand."),
  );
}
