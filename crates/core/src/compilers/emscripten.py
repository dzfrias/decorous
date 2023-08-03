from pathlib import Path
import os


def main():
    input = Path(os.environ["DECOR_INPUT"])
    outdir = Path(os.environ["DECOR_OUT"])
    name = input.stem
    pre = outdir / "__pre.js"

    # The contents of this file will run before the JavaScript glue
    with open(pre, "w") as f:
        f.write(
            f"""var Module = {{
  locateFile: function(s) {{
    return '{outdir}/' + s;
  }}
}};"""
        )
    out_name = (outdir / name).with_suffix(".js")
    os.system(
        f'emcc \
            --pre-js "{pre}" \
            {input} -o {out_name} \
            -s NO_EXIT_RUNTIME=1 \
            -s MODULARIZE=1 \
            -s EXPORT_ES6=1 \
            -s EXPORT_NAME="initModule" \
            -s ASYNCIFY \
            -s EXPORTED_RUNTIME_METHODS=\'["ccall"]\''
    )
    # Clean up __pre.js file
    os.remove(pre)

    print(
        f"""import init from "./{outdir}/{name}.js";
let wasm = await init();"""
    )


if __name__ == "__main__":
    main()
