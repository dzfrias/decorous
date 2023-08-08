import os
import shutil
import subprocess
import urllib.request


WASM_EXEC_URL = (
    "https://raw.githubusercontent.com/tinygo-org/tinygo/release/targets/wasm_exec.js"
)


def main():
    input = os.environ["DECOR_INPUT"]
    outdir = os.environ["DECOR_OUT"]
    exports = os.environ["DECOR_EXPORTS"]

    urllib.request.urlretrieve(WASM_EXEC_URL, "wasm_exec.js")
    shutil.move("wasm_exec.js", os.path.join(outdir, "wasm_exec.js"))
    shutil.copy(input, "main.go")
    subprocess.run(
        [
            "tinygo",
            "build",
            "-o",
            os.path.join(outdir, "out.wasm"),
            "-target",
            "wasm",
            "main.go",
        ],
        check=True,
    )
    os.remove("main.go")

    print(f'import "./{outdir}/wasm_exec.js";\nconst go = new Go();')
    if exports:
        import_inner = ", ".join([str(exp) for exp in exports.split(" ")])
        print(f"go.importObject.env = {{ {import_inner} }};")
    print(
        f"""let wasm = await WebAssembly.instantiateStreaming(fetch("{outdir}/out.wasm"), go.importObject);
go.run(wasm.instance);
wasm = wasm.instance.exports;"""
    )


if __name__ == "__main__":
    main()
