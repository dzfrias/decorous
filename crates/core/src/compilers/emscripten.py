from pathlib import Path
import os


def main():
    input = Path(os.environ["DECOR_INPUT"])
    outdir = Path(os.environ["DECOR_OUT"])
    name = input.stem

    out_name = (outdir / name).with_suffix(".js")
    os.system(
        f"emcc {input} -o {out_name} -s NO_EXIT_RUNTIME=1 -s EXPORTED_RUNTIME_METHODS='[\"ccall\"]'"
    )

    print(
        f"""<script async src="{outdir}/{name}.js"></script>
<script defer src="{outdir}.js"></script>"""
    )


if __name__ == "__main__":
    main()
