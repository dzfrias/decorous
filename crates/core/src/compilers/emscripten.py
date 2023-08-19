from pathlib import Path
import os
import sys


def main():
    input = Path(os.environ["DECOR_INPUT"])
    outdir = Path(os.environ["DECOR_OUT"])
    outdir_abs = Path(os.environ["DECOR_OUT_DIR"])
    name = input.stem
    pre = "__pre.js"

    # The contents of this file will run before the JavaScript glue
    try:
        with open(pre, "w") as f:
            f.write(
                f"""var Module = {{
      locateFile: function(s) {{
        return '{outdir}/' + s;
      }}
    }};"""
            )
    except:
        raise Exception("problem writing __pre.js")

    out_name = (outdir_abs / name).with_suffix(".js")
    status = os.system(
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
    if status != 0:
        raise Exception("error compiling emscripten")

    print(
        f"""import init from "./{outdir}/{name}.js";
let wasm = await init();"""
    )


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(f"\nerror occurred: {e}", file=sys.stderr)
        sys.exit(1)
