if ({replaced}) {{
    if (elems["{id}_block"] && elems["{id}_on"]) {{
        elems["{id}_block"].u(dirty);
    }} else {{
        elems["{id}_on"] = true;
        elems["{id}_block"]?.d();
        elems["{id}_block"] = create_{id}_block(elems["{id}"].parentNode, elems["{id}"]);
    }}
}} else if (elems["{id}_on"]) {{
    elems["{id}_on"] = false;
    elems["{id}_block"]?.d();
    elems["{id}_block"] = create_{id}_else_block(elems["{id}"].parentNode, elems["{id}"]);
}}
