from pathlib import Path
import os
import shutil


def main():
    input = Path(os.environ["DECOR_INPUT"])
    outdir = Path(os.environ["DECOR_OUT"])
    exports = os.environ["DECOR_EXPORTS"]
    name = input.stem

    os.system(f"zig build-lib {input} -target wasm32-freestanding -dynamic")
    shutil.move(f"{name}.wasm", outdir)
    os.remove(f"{name}.wasm.o")

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
    main()
