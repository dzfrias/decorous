function replace(node) {{
  const text = document.createTextNode("");
  node.replaceWith(text);
  return text;
}}
