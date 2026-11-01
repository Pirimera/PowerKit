"""Fast, offline repository contracts. Python 3.11+."""
from pathlib import Path
import re
import subprocess
import sys
import tomllib
from urllib.parse import unquote, urlsplit
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
REQUIRED = (
    "LICENSE", "README.md", "CONTRIBUTING.md", "CODE_OF_CONDUCT.md",
    "SECURITY.md", "SUPPORT.md", "CHANGELOG.md", "AGENTS.md",
    "rust-toolchain.toml", ".editorconfig", ".github/CODEOWNERS",
    ".github/dependabot.yml", ".github/pull_request_template.md",
    "docs/architecture.md", "docs/development.md", "docs/testing.md",
    "docs/releasing.md", ".agents/skills/powerkit-development/SKILL.md",
    ".agents/skills/powerkit-release/SKILL.md",
)

def local_link_errors(root: Path, source: Path, text: str) -> list[str]:
    # Ignore fenced examples and inline code; validate inline Markdown links.
    tick = chr(96)
    text = re.sub(r"(?ms)^(" + tick + r"{3,}|~{3,}).*?^\1\s*$", "", text)
    text = re.sub(tick + "[^" + tick + r"\n]*" + tick, "", text)
    errors = []
    for target in re.findall(r"\[[^\]\n]*\]\(([^)\n]+)\)", text):
        target = target.strip().strip("<>")
        if not target or target.startswith("#") or urlsplit(target).scheme:
            continue
        path = unquote(target.split("#", 1)[0].split("?", 1)[0])
        resolved = (source.parent / path).resolve()
        if not resolved.is_relative_to(root.resolve()):
            errors.append(f"{source.relative_to(root)}: link escapes repository: {target}")
        elif not resolved.exists():
            errors.append(f"{source.relative_to(root)}: broken link: {target}")
    return errors

def skill_errors(root: Path) -> list[str]:
    errors = []
    for skill in sorted((root / ".agents/skills").glob("*/SKILL.md")):
        text = skill.read_text(encoding="utf-8")
        parts = text.split("---", 2)
        if len(parts) != 3 or parts[0].strip():
            errors.append(f"Missing skill frontmatter: {skill.name}")
            continue
        fields = dict(line.split(":", 1) for line in parts[1].splitlines() if ":" in line)
        name = fields.get("name", "").strip()
        description = fields.get("description", "").strip()
        if name != skill.parent.name or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name):
            errors.append(f"Skill name must match its directory: {skill.parent.name}")
        if not description:
            errors.append(f"Missing skill description: {skill.parent.name}")
    return errors

def metadata_errors(root: Path) -> list[str]:
    errors = []
    cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    package = cargo["package"]
    version = package["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        errors.append("Cargo version must be numeric and stable")
    if package.get("license") != "MIT" or package.get("publish") is not False:
        errors.append("Application must declare MIT and publish = false")
    identity = ET.parse(root / "app.manifest").getroot().find("{urn:schemas-microsoft-com:asm.v1}assemblyIdentity")
    if identity is None or identity.attrib["version"] != version + ".0":
        errors.append("app.manifest version must match Cargo.toml + .0")
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    if not any(x["name"] == "powerkit" and x["version"] == version for x in lock["package"]):
        errors.append("Cargo.lock application version differs")
    if not (root / "docs/releases" / f"{version}.md").is_file():
        errors.append(f"Missing release notes for {version}")
    toolchain = tomllib.loads((root / "rust-toolchain.toml").read_text(encoding="utf-8"))["toolchain"]["channel"]
    if package["rust-version"] != toolchain:
        errors.append("Validated minimum Rust and pinned toolchain differ")
    workflow = (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
    if f"rustup toolchain install {toolchain} " not in workflow:
        errors.append("Workflow Rust version differs")
    for action in re.findall(r"uses:\s+([^\s#]+)", workflow):
        if not action.startswith("./") and not re.fullmatch(r"[^@]+@[0-9a-f]{40}", action):
            errors.append(f"Action must be pinned to a commit SHA: {action}")
    sdk = cargo["target"]["cfg(windows)"]["dependencies"]["velopack"].lstrip("=")
    if f"vpk --version {sdk} " not in workflow:
        errors.append("Velopack CLI and SDK versions differ")
    return errors

def main() -> int:
    errors = [f"Missing {name}" for name in REQUIRED if not (ROOT / name).is_file()]
    errors.extend(metadata_errors(ROOT))
    errors.extend(skill_errors(ROOT))
    names = subprocess.check_output(
        ["git", f"--git-dir={ROOT / '.git'}", f"--work-tree={ROOT}",
         "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
    ).decode("utf-8").split("\0")
    for name in sorted(set(names)):
        source = ROOT / name
        if name.endswith(".md") and source.is_file():
            errors.extend(local_link_errors(ROOT, source, source.read_text(encoding="utf-8")))
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        return 1
    print("Repository contracts and local Markdown links: PASS")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
