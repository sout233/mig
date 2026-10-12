"""Package native release binaries and write checksums without third-party modules."""

import argparse
import hashlib
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile


ROOT = Path(__file__).resolve().parent.parent


def macos_bundle(binary: Path, directory: Path, version: str) -> Path:
    bundle_version = version.split("-", 1)[0].split("+", 1)[0]
    app = directory / "MIG.app"
    executable_directory = app / "Contents" / "MacOS"
    resources = app / "Contents" / "Resources"
    executable_directory.mkdir(parents=True)
    resources.mkdir()
    shutil.copy2(binary, executable_directory / "mig")
    with (app / "Contents" / "Info.plist").open("wb") as stream:
        plistlib.dump(
            {
                "CFBundleName": "MIG",
                "CFBundleDisplayName": "MIG",
                "CFBundleIdentifier": "io.github.sout233.mig",
                "CFBundleExecutable": "mig",
                "CFBundlePackageType": "APPL",
                "CFBundleShortVersionString": bundle_version,
                "CFBundleVersion": bundle_version,
                "CFBundleGetInfoString": version,
                "CFBundleIconFile": "MIG.icns",
                "NSHighResolutionCapable": True,
                "LSMinimumSystemVersion": os.environ["MACOSX_DEPLOYMENT_TARGET"],
            },
            stream,
        )
    iconset = directory / "MIG.iconset"
    iconset.mkdir()
    source_icon = ROOT / "assets" / "icons" / "mig_icon_512.png"
    for side in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            pixels = side * scale
            suffix = "@2x" if scale == 2 else ""
            subprocess.run(
                ["sips", "-z", str(pixels), str(pixels), str(source_icon),
                 "--out", str(iconset / f"icon_{side}x{side}{suffix}.png")],
                check=True,
            )
    subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(resources / "MIG.icns")], check=True)
    subprocess.run(["codesign", "--force", "--sign", "-", "--timestamp=none", str(app)], check=True)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
    subprocess.run(["otool", "-L", str(executable_directory / "mig")], check=True)
    return app


def package_binary(binary: Path, platform: str, arch: str, output: Path) -> Path:
    if not binary.is_file():
        raise FileNotFoundError(f"Release binary is missing: {binary}")
    output.mkdir(parents=True, exist_ok=True)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    basename = f"mig-{platform}-{arch}"
    if platform == "windows":
        archive = output / f"{basename}.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
            bundle.write(binary, "mig.exe")
    else:
        archive = output / f"{basename}.tar.gz"
        with tempfile.TemporaryDirectory(prefix="mig-release-") as temporary:
            directory = Path(temporary)
            if platform == "macos":
                content = macos_bundle(binary, directory, version)
            else:
                content = directory / basename
                content.mkdir()
                shutil.copy2(binary, content / "mig")
                shutil.copy2(ROOT / "assets" / "icons" / "mig_icon_512.png", content / "mig.png")
                (content / "mig.desktop").write_text(
                    "[Desktop Entry]\nType=Application\nName=MIG\nExec=mig\nIcon=mig\n"
                    "Terminal=false\nCategories=Audio;Music;Player;\n",
                    encoding="utf-8",
                )
                shutil.copy2(ROOT / "docs" / "platforms.md", content / "README.md")
            with tarfile.open(archive, "w:gz") as bundle:
                bundle.add(content, arcname=content.name)
    with archive.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    (output / f"{archive.name}.sha256").write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    print(f"Packaged {archive.name} ({digest})")
    return archive


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=("windows", "macos", "linux"), required=True)
    parser.add_argument("--arch", choices=("x86_64", "amd64", "arm64"), required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    executable = "mig.exe" if args.platform == "windows" else "mig"
    binary = ROOT / "target" / args.target / "release" / executable
    package_binary(binary, args.platform, args.arch, args.output.resolve())


if __name__ == "__main__":
    main()
