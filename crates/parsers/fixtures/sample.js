export function top(input) {
  function nested() {
    return input;
  }
  return nested();
}

export async function asyncTop() {}

export function* generatedTop() {
  yield 1;
}

export default function defaulted() {}

export class Service {
  constructor() {}
  method() {}
  static create() {}
  get current() { return 1; }
  set current(value) { void value; }
  #private() {}
  field = () => {};
}

const NamedExpression = class InnerClass {
  inside() {}
};

const AnonymousExpression = class {};

const arrow = () => {};
let expression = function () {};
export const exportedArrow = async () => {};

const object = {
  shorthand() {},
  key: function () {},
  'quoted': () => {},
  [computed]: function () {},
};

exports.h = async () => {};
Foo.prototype.bar = function () {};
module.exports = function () {};

ig.module('game.main')
  .requires('impact.game')
  .defines(function () {
    MyGame = ig.Game.extend({
      init: function () { this.parent(); },
      update: function () {},
      draw: function () {},
    });
  });

consume(() => {});
(function () {})();
const n = 42;
this.x = 5;
const { destructured } = source;
handlers[computed] = function () {};
exports['x'] = () => {};

const text = "function class";
const template = `function class ${n}`;
// function fake() {}
/* class Fake {} */
