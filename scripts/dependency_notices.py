"""Collect exact locked Cargo dependency license texts for distribution."""
import argparse
import json
from pathlib import Path
import subprocess

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target", required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1",
         "--filter-platform", args.target], cwd=root, text=True, encoding="utf-8"))
    active = {node["id"] for node in metadata["resolve"]["nodes"]}
    sections = ["PowerKit third-party notices",
                "Cargo dependencies for " + args.target + "; includes build dependencies.",
                "Generated from Cargo.lock and upstream license files. No source paths are included."]
    for package in sorted(metadata["packages"], key=lambda x: (x["name"], x["version"])):
        if package["id"] not in active or package["id"] in metadata["workspace_members"]:
            continue
        directory = Path(package["manifest_path"]).parent
        licenses = {f for f in directory.iterdir()
                    if f.is_file() and f.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE"))}
        if package.get("license_file"):
            licenses.add(directory / package["license_file"])
        if not licenses:
            reviewed = root / "docs/licenses" / f"{package['name']}-{package['version']}.txt"
            if reviewed.is_file():
                licenses.add(reviewed)
        if not licenses:
            raise RuntimeError(f"Review missing license text: {package['name']} {package['version']}")
        sections.append(f"\n=== {package['name']} {package['version']} ===\nSPDX: {package.get('license') or 'see license text'}")
        for file in sorted(licenses):
            sections.append(f"--- {file.name} ---\n" + file.read_text(encoding="utf-8"))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n\n".join(sections) + "\n", encoding="utf-8")
    print(f"Dependency notices generated: {args.output.name}")

if __name__ == "__main__":
    main()
