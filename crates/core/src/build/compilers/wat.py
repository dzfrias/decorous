from pathlib import Path
import os
import subprocess
import sys


def main():
    input = Path(os.environ["DECOR_INPUT"])
    outdir = os.environ["DECOR_OUT"]
    outdir_abs = os.environ["DECOR_OUT_DIR"]
    exports = os.environ["DECOR_EXPORTS"]
    name = input.stem

    subprocess.run(
        [
            "wat2wasm",
            input,
            "-o",
            os.path.join(outdir_abs, f"{name}.wasm"),
            *sys.argv[1:],
        ],
        check=True,
    )

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
