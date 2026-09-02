import type { External } from "./external";

export function top(input: string): string {
  function nested(): string {
    return input;
  }
  return nested();
}

export default class Service {
  run(): void {}

  field = () => {};
}

abstract class AbstractStore extends Service {
  abstract load(): Promise<void>;
}

interface Runner {
  run(): void;
}

enum Mode {
  One,
}

type Result = { ok: boolean; value?: External };

function declared(input: string): number;

namespace Tools {
  export function inside(): void {}

  export interface Nested {
    call(): void;
  }
}

const plain = () => {};
let assigned = function () {};
export const exported = () => {};
const asyncTask = async () => {};
const generated = function* () {
  yield 1;
};
const x = 1;
const cfg = {};
