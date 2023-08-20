import os
import sys
import subprocess
from pathlib import Path


def main():
    input = Path(os.environ["DECOR_INPUT"])
    outdir = os.environ["DECOR_OUT"]
    cache = os.environ["DECOR_CACHE"]
    outdir_abs = os.environ["DECOR_OUT_DIR"]
    name = input.stem

    PROJECT_NAME = "decor-out"

    if not os.path.isdir(PROJECT_NAME):
        create_wasm_bindgen_project(PROJECT_NAME)

    lib_path = os.path.join("src", "lib.rs")
    with open(input, "r") as file:
        contents = file.read()
    with open(lib_path, "w") as f:
        f.write(contents)

    subprocess.run(
        [
            "wasm-pack",
            "build",
            "--target",
            "web",
            "--out-name",
            name,
            "--out-dir",
            outdir_abs,
            "--color",
            "always",
            "--target-dir",
            cache,
            *sys.argv[1:],
        ],
        check=True,
    )

    print(
        f"""import init, * as wasm from "/{outdir}/__tmp.js";
await init();"""
    )


def create_wasm_bindgen_project(name: str):
    subprocess.run(["cargo", "init", "--lib", "--name", name], check=True)
    with open(f"Cargo.toml", "w") as f:
        contents = f"""[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
wasm-bindgen = "0.2"
"""
        f.write(contents)


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(f"\nerror occurred: {e}", file=sys.stderr)
        sys.exit(1)
