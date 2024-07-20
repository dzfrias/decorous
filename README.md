# Decorous

**Decorous** makes [WebAssembly](https://webassembly.org/) ↔ JavaScript interop
**seamless**, from _any_ language. It's as simple as:

```text
---rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn add(x: i32, y: i32) -> i32 {
    x + y
}
---

---js
console.log("hello");
---

#button[@click={() => console.log(wasm.add(1, 2))}] Hello! /button
```

Run `decorous build test.decor`, and you'll get a set of static files that you
can embed into anything:

- A framework
- A static site
- Another decorous component

Want Wasm optimizations powered by
[wasm-opt](https://github.com/WebAssembly/binaryen)?
`decorous build test.decor -O3 --strip`. Want to build as an
[ES6 module](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Modules)
that you can import into the rest of your app?
`decorous build test.decor --modularize`.

Importantly, Decorous **is not meant a framework**. The compiler generates
lightweight, embeddable code that can be easily integrated into the rest of your
application. Most websites don't need a WebAssembly framework to dominate their
codebase, or to poorly integrate with a frontend JS framework already being
used.

## Enhanced JavaScript

With semantics inspired by [Svelte](https://github.com/sveltejs/svelte),
Decorous allows you to write simple and concise JavaScript to complement your
WebAssembly component:

```text
---zig
export fn add(a: i32, b: i32) i32 {
    return a + b;
}
---

---js
let counter = 0;
---

#button[@click={() => counter = wasm.add(counter, 1)}] {counter} /button
```

The compiler only generates **what you need**, so you don't have to load massive
JavaScript files client-side.

Significant efforts were also made to make sure that the generated JS is
performant.

## Powerful Markup

Decorous provides **dynamic templating** to further supercharge your components:

```text
---js
let stuff = [];
---

#button[@click={() => stuff = [...stuff, "thing"]}] Click me! /button
{#if stuff.length >= 10}
  #span You win! /span
{:else}
  #span You're losing... /span
{/if}
{#for thing in stuff}
  #span {thing} /span
{/for}
```

Note the way we "pushed" to `stuff`! `stuff.push("thing")` _wouldn't_ actually
work, because Decorous can _only_ update a template variable when it's been
assigned to.

### Scoped CSS

CSS is automatically scoped to the current component, meaning styles won't leak
to other parts of your website:

```text
---css
p {
  color: red;
}
---

#p Red /p
```

## Rendering Backends

Decorous _does not_ create fully JavaScript-generated DOMs, like a
[single-paged application](https://developer.mozilla.org/en-US/docs/Glossary/SPA)
would, so don't worry about that either. Any markup you write is translated to
_static HTML_ at compile-time, which the JavaScript/Wasm attaches to. In this
way, Decorous aims to be a **zero-cost abstraction**.

But, if you want a DOM created entirely by JS, you can absolutely do that! Just
pass `--render-method csr`, and you'll be good to go!

### Modularization

You can also build your component into a
[ES6 module](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Modules),
though the `--modularize` flag. Once you have the generated `mjs` file, you use
it like so:

```javascript
import initialize from "./out.mjs";

const element = document.getElementById("my-element");
initialize(element);
```

This will anchor your component to `element`.

## Language Support

Decourous has built-in support for the following languages:

- C (emscriptem)
- C++ (emscriptem)
- Rust
- Go
- [TinyGo](https://tinygo.org/)
- [WAT](https://developer.mozilla.org/en-US/docs/WebAssembly/Understanding_the_text_format)
- Zig (0.13+)

Don't see your favorite language? If you want to write your own custom script,
you can! And, if applicable, feel free to contribute it to this repo!

## Documentation

⚠️ Complete documentation is in progress! ⚠️

## License

`decorous` is licensed under the [MIT License](./LICENSE).
