from pathlib import Path

from PIL import Image


def main() -> None:
    assets_dir = Path("autosample-gui") / "assets"
    src = assets_dir / "logo.png"
    ico_out = assets_dir / "logo.ico"
    icns_out = assets_dir / "logo.icns"

    img = Image.open(src).convert("RGBA")
    ico_sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]

    img.save(ico_out, format="ICO", sizes=ico_sizes)
    img.resize((1024, 1024), Image.Resampling.LANCZOS).save(icns_out, format="ICNS")

    print(f"Generated {ico_out}")
    print(f"Generated {icns_out}")


if __name__ == "__main__":
    main()
