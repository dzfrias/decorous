function make_fragment(ctx) {{
  {}
  return {{
    c() {{
      {}
    }},
    m(target) {{
      {}
    }},
    u(ctx) {{
      {}
    }},
    d() {{
      {}
    }},
  }};
}}
const ctx = {};
const fragment = make_fragment(ctx);
fragment.c();
fragment.m(document.getElementById("app"));
function __schedule_update(ctx_idx, val) {{
  Promise.resolve().then(() => {{
    ctx[ctx_idx] = val;
    fragment.u(ctx);
  }});
}}
