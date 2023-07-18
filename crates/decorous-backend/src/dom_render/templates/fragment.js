function create_{id}_block(target, anchor) {{
function mount(target, newNode, anchor) {{
target.insertBefore(newNode, anchor || null);
}}
{decls}{mounts}return {{
u(dirty) {{
{update_body}}},
d() {{
{detach_body}}}
}};
}}
