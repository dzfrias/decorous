from pathlib import Path
import os
import shutil


def main():
    input = Path(os.environ["DECOR_INPUT"])
    outdir = Path(os.environ["DECOR_OUT"])
    name = input.stem

    os.system(f"zig build-lib {input} -target wasm32-freestanding -dynamic")
    shutil.move(f"{name}.wasm", outdir)
    os.remove(f"{name}.wasm.o")

    print(
        f'let wasm = (await WebAssembly.instantiateStreaming(fetch("./{outdir}/{name}.wasm"))).instance.exports;'
    )


if __name__ == "__main__":
    main()
