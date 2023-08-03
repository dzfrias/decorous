from pathlib import Path
import os
import shutil
import sys
import subprocess


def main():
    if shutil.which("emcc") is None:
        raise Exception("emscripten not installed")

    input = Path(os.environ["DECOR_INPUT"])
    outdir = Path(os.environ["DECOR_OUT"])
    name = input.stem
    pre = outdir / "__pre.js"

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

    out_name = (outdir / name).with_suffix(".js")
    args = [
        "emcc",
        "--pre-js",
        pre,
        input,
        "-o",
        out_name,
        "-s",
        "NO_EXIT_RUNTIME=1",
        "-s",
        "MODULARIZE=1",
        "-s",
        "EXPORT_ES6=1",
        "-s",
        "EXPORT_NAME='initModule'",
        "-s",
        "ASYNCIFY=1",
        "-s",
        'EXPORTED_RUNTIME_METHODS=["ccall"]',
        *sys.argv[1:],
    ]
    try:
        subprocess.run(args, check=True)
    except:
        raise Exception("emscripten had a problem while compiling")
    try:
        # Clean up __pre.js file
        os.remove(pre)
    except:
        raise Exception("problem removing __pre.js")

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
