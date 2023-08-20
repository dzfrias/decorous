import os
import subprocess
import shutil


def main():
    input = os.environ["DECOR_INPUT"]
    outdir = os.environ["DECOR_OUT"]
    outdir_abs = os.environ["DECOR_OUT_DIR"]
    exports = os.environ["DECOR_EXPORTS"]

    os.environ["GOOS"] = "js"
    os.environ["GOARCH"] = "wasm"
    go_root = subprocess.run(
        ["go", "env", "GOROOT"], capture_output=True
    ).stdout.strip()
    wasm_exec = os.path.join(go_root.decode(), "misc", "wasm", "wasm_exec.js")
    shutil.copy(wasm_exec, outdir_abs)
    shutil.copy(input, "main.go")
    subprocess.run(["go", "mod", "init", "github.com/dzfrias/decorous"], check=True)
    subprocess.run(
        ["go", "build", "-o", os.path.join(outdir_abs, "out.wasm")], check=True
    )

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
