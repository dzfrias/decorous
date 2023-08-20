from pathlib import Path
import os
import shutil
import subprocess
import sys


def main():
    input = Path(os.environ["DECOR_INPUT"])
    outdir = Path(os.environ["DECOR_OUT"])
    outdir_abs = Path(os.environ["DECOR_OUT_DIR"])
    exports = os.environ["DECOR_EXPORTS"]
    name = input.stem

    subprocess.run(
        [
            "zig",
            "build-lib",
            input,
            "-target",
            "wasm32-freestanding",
            "-dynamic",
            "--color",
            "on",
            *sys.argv[1:],
        ],
        check=True,
    )
    shutil.move(f"{name}.wasm", outdir_abs)

    if not exports:
        print(
            f'let wasm = (await WebAssembly.instantiateStreaming(fetch("./{outdir}/{name}.wasm"))).instance.exports;'
        )
    else:
        import_inner = ", ".join([str(exp) for exp in exports.split(" ")])
        print(
            f'let wasm = (await WebAssembly.instantiateStreaming(fetch("./{outdir}/{name}.wasm"), {{ env: {{ {import_inner} }} }})).instance.exports;'
        )


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(f"\nerror occurred: {e}", file=sys.stderr)
        sys.exit(1)
