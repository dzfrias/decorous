function __schedule_update(ctx_idx, val) {{
  ctx[ctx_idx] = val;
  dirty[Math.max(Math.ceil(ctx_idx / 8) - 1, 0)] |= 1 << (ctx_idx % 8);
  if (updating) return;
  updating = true;
  Promise.resolve().then(() => {{
    __update(dirty, false);
    updating = false;
    dirty.fill(0);
  }});
}}
