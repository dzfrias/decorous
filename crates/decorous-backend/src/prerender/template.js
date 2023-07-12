const dirty = new Uint8Array(new ArrayBuffer({dirty_items}));
function replace(node) {{
const text = document.createTextNode("");
node.replaceWith(text);
return text;
}}
const elems = [{elems}];
function __init_ctx() {{
{ctx_body}}}
const ctx = __init_ctx();
let updating = false;
function __update(dirty) {{
{update_body}}}
function __schedule_update(ctx_idx, val) {{
ctx[ctx_idx] = val;
dirty[Math.max(Math.ceil(ctx_idx / 8) - 1, 0)] |= 1 << (ctx_idx % 8);
if (updating) return;
updating = true;
Promise.resolve().then(() => {{
__update(dirty);
updating = false;
dirty.fill(0);
}});
}}
