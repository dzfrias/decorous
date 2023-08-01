from pathlib import Path
import sys
import os


def main():
    input = Path(sys.argv[1])
    outdir = Path(sys.argv[2])
    name = os.path.basename(input)

    out_name = (outdir / name).with_suffix(".js")
    os.system(
        f"emcc {input} -o {out_name} -s NO_EXIT_RUNTIME=1 -s EXPORTED_RUNTIME_METHODS='[\"ccall\"]'"
    )

    print(
        f"""<script async src="{out_name}"></script>
<script defer src="{outdir}.js"></script>"""
    )


if __name__ == "__main__":
    main()
