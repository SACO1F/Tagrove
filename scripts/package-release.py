"""Refresh a local delivery snapshot and checksums after building the application.

Run from any directory. Existing assets must match current verified build outputs;
refreshing same-version assets requires explicit --refresh-binaries. This does not build
or publish the application, and never includes user data or local archives.
"""
from pathlib import Path
import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tomllib
import zipfile


def digest(path):
    hasher = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest()


def validate_versions(root, expected):
    values = [json.loads((root / name).read_text(encoding="utf-8"))["version"] for name in ("package.json", "package-lock.json", "src-tauri/tauri.conf.json")]
    lock = json.loads((root / "package-lock.json").read_text(encoding="utf-8"))
    values.append(lock["packages"][""]["version"])
    values.append(tomllib.loads((root / "src-tauri/Cargo.toml").read_text(encoding="utf-8"))["package"]["version"])
    cargo_lock = tomllib.loads((root / "src-tauri/Cargo.lock").read_text(encoding="utf-8"))
    values.extend(p["version"] for p in cargo_lock["package"] if p["name"] == "shiqian")
    if len(values) != 6 or any(version != expected for version in values):
        raise RuntimeError("Version mismatch: requested release must match all six source manifests")


def validate_binary(path, expected):
    if not path.is_file():
        raise FileNotFoundError("Build matching application assets first: " + str(path))
    literal = "'" + str(path).replace("'", "''") + "'"
    version = subprocess.check_output(["powershell.exe", "-NoProfile", "-Command", "(Get-Item -LiteralPath " + literal + ").VersionInfo.ProductVersion"], text=True).strip()
    if version != expected:
        raise RuntimeError("Binary version mismatch: " + str(path) + " (" + version + ")")


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", default=json.loads((root / "package.json").read_text(encoding="utf-8"))["version"])
    parser.add_argument("--refresh-binaries", action="store_true", help="Explicitly replace this version's delivery binaries with matching verified build outputs")
    parser.add_argument("--directory-name", help="Use a separate folder under releases when a previous delivery binary is in use")
    args = parser.parse_args()
    version = args.version
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?", version):
        raise ValueError("Invalid release version")
    if version == "0.3.0":
        subprocess.run([sys.executable, str(root / "scripts/stable-release-gate.py")], check=True)
    validate_versions(root, version)
    if version == "0.6.12":
        if args.directory_name and args.directory_name != "v0.6.12":
            raise ValueError("The 0.6.12 formal release uses the fixed v0.6.12 directory")
        # Current packaging uses committed sources and nested extension icons.
        subprocess.run([sys.executable, str(root / "scripts/release-v0612-delivery.py")], check=True)
        return
    mappings = {"0.3.0-beta.1": "beta1", "0.3.0-beta.2": "beta2", "0.3.0-beta.3": "beta3", "0.3.0-beta.4": "beta4", "0.3.0-beta.5": "beta5", "0.3.0-beta.6": "beta6", "0.3.0-beta.7": "beta7", "0.3.0-beta.8": "beta8", "0.3.0-beta.9": "beta9"}
    mappings["0.3.0-beta.10"] = "beta10"
    mappings["0.3.0-beta.12"] = "beta12"
    mappings["0.3.0-rc.1"] = "rc1"
    mappings["0.3.0-rc.2"] = "rc2"
    mappings["0.3.0-rc.3"] = "rc3"
    mappings["0.3.0-rc.4"] = "rc4"
    mappings["0.4.0"] = "v040"
    mappings["0.4.1"] = "v041"
    mappings["0.4.2"] = "v042"
    mappings["0.4.3"] = "v043"
    mappings["0.4.4"] = "v044"
    mappings["0.4.5"] = "v045"
    mappings["0.4.6"] = "v046"
    mappings["0.4.7"] = "v047"
    mappings["0.5.0"] = "v050"
    mappings["0.5.1"] = "v051"
    mappings["0.5.2"] = "v052"
    mappings["0.5.3"] = "v053"
    mappings["0.5.4"] = "v054"
    mappings["0.5.5"] = "v055"
    mappings["0.5.6"] = "v056"
    mappings["0.5.7"] = "v057"
    mappings["0.5.8"] = "v058"
    mappings["0.5.9"] = "v059"
    mappings["0.5.10"] = "v0510"
    mappings["0.5.11"] = "v0511"
    mappings["0.6.0"] = "v060"
    mappings["0.6.1"] = "v061"
    mappings["0.6.2"] = "v062"
    mappings["0.6.3"] = "v063"
    mappings["0.6.4"] = "v064"
    mappings["0.6.5"] = "v065"
    mappings["0.6.6"] = "v066"
    mappings["0.6.7"] = "v067"
    mappings["0.6.8"] = "v068"
    mappings["0.6.9"] = "v069"
    mappings["0.6.10"] = "v0610"
    mappings["0.6.11"] = "v0611"
    mappings["0.3.0"] = "stable"
    if version not in mappings:
        raise ValueError("Add matching guide/validation mappings for this release")
    suffix = mappings[version]
    directory = args.directory_name or ("v" + version)
    if not re.fullmatch(r"v[a-zA-Z0-9][a-zA-Z0-9.-]{0,62}", directory) or ".." in directory:
        raise ValueError("Invalid delivery directory name")
    release = root / "releases" / directory
    release.mkdir(parents=True, exist_ok=True)
    prefix = "Shiqian-v" + version
    binary_assets = [
        (prefix + "-Windows-x64.exe", root / "src-tauri/target/release/shiqian.exe"),
        (prefix + "-Windows-x64-setup.exe", root / "src-tauri/target/release/bundle/nsis" / ("拾签_" + version + "_x64-setup.exe")),
    ]
    # Check both outputs and existing assets before replacing any delivery file.
    for destination, source in binary_assets:
        validate_binary(source, version)
        target = release / destination
        if target.exists() and not args.refresh_binaries:
            validate_binary(target, version)
            if digest(target) != digest(source):
                raise RuntimeError("Stale delivery binary: " + destination + "; use --refresh-binaries after verification")
    for destination, source in binary_assets:
        target = release / destination
        if not target.exists() or args.refresh_binaries:
            shutil.copy2(source, target)
    # Documentation names are explicit for the current Beta; new versions must
    # provide their own guide instead of silently copying unrelated release text.
    guide_name = "release-v" + version + ".md" if suffix in {"v040", "v041", "v042", "v043", "v044", "v045", "v046", "v047", "v050", "v051", "v052", "v053", "v054", "v055", "v056", "v057", "v058", "v059", "v0510", "v0511", "v060", "v061", "v062", "v063", "v064", "v065", "v066", "v067", "v068", "v069", "v0610", "v0611"} else "release-v0.3-" + suffix + ".md" if (suffix.startswith("rc") or suffix in {"stable", "v040", "v041", "v042", "v043", "v044", "v045", "v046", "v047", "v050", "v051", "v052", "v053", "v054", "v055", "v056", "v057", "v058", "v059", "v0510", "v0511", "v060", "v061", "v062", "v063", "v064", "v065", "v066", "v067", "v068", "v069", "v0610", "v0611"}) else "beta-v0.3-" + suffix + ".md"
    validation_name = "validation-v" + version + ".md" if suffix in {"v040", "v041", "v042", "v043", "v044", "v045", "v046", "v047", "v050", "v051", "v052", "v053", "v054", "v055", "v056", "v057", "v058", "v059", "v0510", "v0511", "v060", "v061", "v062", "v063", "v064", "v065", "v066", "v067", "v068", "v069", "v0610", "v0611"} else "validation-v0.3-" + suffix + ".md"
    extras = {
        "user-manual-v0.3.zh-CN.md": "manual.zh-CN.md",
        "upgrade-v0.3.zh-CN.md": "upgrade.zh-CN.md",
        "known-issues-v0.3.md": "known-issues.zh-CN.md",
        "stable-release-acceptance.md": "acceptance.zh-CN.md",
        "THIRD_PARTY_NOTICES.md": "third-party-notices.md",
        "colleague-handoff-v0.3.zh-CN.md": "handoff.zh-CN.md",
    } if (suffix.startswith("rc") or suffix in {"stable", "v040", "v041", "v042", "v043", "v044", "v045", "v046", "v047", "v050", "v051", "v052", "v053", "v054", "v055", "v056", "v057", "v058", "v059", "v0510", "v0511", "v060", "v061", "v062", "v063", "v064", "v065", "v066", "v067", "v068", "v069", "v0610", "v0611"}) else {}
    if suffix == "v047":
        extras["performance-report-v0.4.7.md"] = "performance.zh-CN.md"
    if suffix in {"v055", "v056", "v057", "v058", "v059", "v0510", "v0511", "v060", "v061", "v062", "v063", "v064", "v065", "v066", "v067", "v068", "v069", "v0610", "v0611"}:
        extras["unified-search-proposal.md"] = "search-proposal.zh-CN.md"
        extras["development-plan-after-v0.5.5-2026-10-14.md"] = "development-plan.zh-CN.md"
    if suffix in {"v056", "v057", "v058", "v059", "v0510", "v0511", "v060", "v061", "v062", "v063", "v064", "v065", "v066", "v067", "v068", "v069", "v0610", "v0611"}:
        extras["search-quality-acceptance-v0.5.6.md"] = "search-quality.zh-CN.md"
    if suffix in {"v057", "v058", "v059", "v0510", "v0511", "v060", "v061", "v062", "v063", "v064", "v065", "v066", "v067", "v068", "v069", "v0610", "v0611"}:
        extras["dnd-kit-evaluation-2026-10-14.md"] = "dnd-kit-research.zh-CN.md"
        extras["development-plan-after-v0.5.5-2026-10-14.md"] = "development-plan-previous.zh-CN.md"
        extras["development-plan-after-v0.5.7-2026-10-15.md"] = "development-plan.zh-CN.md"
        extras["test-report-v0.5.7-experience.14-2026-10-15.md"] = "test-report.zh-CN.md"
    if suffix == "v058":
        extras.pop("test-report-v0.5.7-experience.14-2026-10-15.md", None)
        extras["test-report-v0.5.8-2026-10-15.md"] = "test-report.zh-CN.md"
        extras["user-manual-v0.5.8.en.md"] = "manual.en.md"
        extras["acceptance-v0.5.8.md"] = "acceptance-v058.zh-CN.md"
    if suffix == "v058" and directory == "v0.5.8-experience.1":
        extras["test-report-v0.5.8-2026-10-15.md"] = "test-report-baseline.zh-CN.md"
        extras["test-report-v0.5.8-experience.1-2026-10-15.md"] = "test-report.zh-CN.md"
    if suffix == "v058" and directory in {"v0.5.8-experience.2", "v0.5.8-experience.3"}:
        extras["user-manual-v0.3.zh-CN.md"] = "manual-previous.zh-CN.md"
        extras["user-manual-v0.5.8.zh-CN.md"] = "manual.zh-CN.md"
        extras["test-report-v0.5.8-2026-10-15.md"] = "test-report-baseline.zh-CN.md"
        extras["test-report-v0.5.8-experience.1-2026-10-15.md"] = "test-report-previous.zh-CN.md"
        extras["test-report-v0.5.8-experience.2-2026-10-15.md"] = "test-report.zh-CN.md"
        extras["cleanup-v0.5.8-experience.2-2026-10-15.md"] = "cleanup.zh-CN.md"
    if suffix == "v058" and directory == "v0.5.8-experience.3":
        extras["test-report-v0.5.8-experience.2-2026-10-15.md"] = "test-report-previous2.zh-CN.md"
        extras["test-report-v0.5.8-experience.3-2026-10-15.md"] = "test-report.zh-CN.md"
    if suffix == "v059":
        extras["user-manual-v0.3.zh-CN.md"] = "manual-previous.zh-CN.md"
        extras.pop("test-report-v0.5.7-experience.14-2026-10-15.md", None)
        extras["user-manual-v0.5.9.zh-CN.md"] = "manual.zh-CN.md"
        extras["user-manual-v0.5.9.en.md"] = "manual.en.md"
        extras["test-report-v0.5.9-2026-10-15.md"] = "test-report.zh-CN.md"
        extras["acceptance-v0.5.9.md"] = "acceptance-v059.zh-CN.md"
        extras["development-plan-after-v0.5.7-2026-10-15.md"] = "development-plan-previous-v057.zh-CN.md"
        extras["development-plan-after-v0.5.8-2026-10-15.md"] = "development-plan.zh-CN.md"
    if suffix == "v0510":
        extras = {
            "THIRD_PARTY_NOTICES.md":"third-party-notices.md",
            "user-manual-v0.5.10.zh-CN.md":"manual.zh-CN.md",
            "user-manual-v0.5.10.en.md":"manual.en.md",
            "test-report-v0.5.10-2026-10-15.md":"test-report.zh-CN.md",
            "cleanup-v0.5.10-2026-10-15.md":"cleanup.zh-CN.md",
        }
    if suffix == "v0511":
        extras = {
            "THIRD_PARTY_NOTICES.md":"third-party-notices.md",
            "user-manual-v0.5.11.zh-CN.md":"manual.zh-CN.md",
            "user-manual-v0.5.11.en.md":"manual.en.md",
            "test-report-v0.5.11-2026-10-15.md":"test-report.zh-CN.md",
            "development-plan-after-v0.5.8-2026-10-15.md":"development-plan.zh-CN.md",
        }
    if suffix == "v060":
        extras = {
            "THIRD_PARTY_NOTICES.md": "third-party-notices.md",
            "user-manual-v0.6.0.zh-CN.md": "manual.zh-CN.md",
            "user-manual-v0.6.0.en.md": "manual.en.md",
            "test-report-v0.6.0-2026-10-16.md": "test-report.zh-CN.md",
            "browser-capture-v0.6.0.md": "browser-capture.zh-CN.md",
            "browser-extension/README.md": "extension-install.zh-CN.md",
            "development-plan-v0.6-series-2026-10-16.md": "development-plan.zh-CN.md",
        }
    if suffix == "v061":
        extras = {
            "THIRD_PARTY_NOTICES.md": "third-party-notices.md",
            "user-manual-v0.6.1.zh-CN.md": "manual.zh-CN.md",
            "user-manual-v0.6.1.en.md": "manual.en.md",
            "test-report-v0.6.1-2026-10-16.md": "test-report.zh-CN.md",
            "browser-capture-v0.6.1.md": "browser-capture.zh-CN.md",
            "browser-extension/README.md": "extension-install.zh-CN.md",
            "development-plan-after-v0.6.1-2026-10-16.md": "development-plan.zh-CN.md",
        }
    if suffix == "v062":
        extras = {
            "THIRD_PARTY_NOTICES.md": "third-party-notices.md",
            "user-manual-v0.6.2.zh-CN.md": "manual.zh-CN.md",
            "user-manual-v0.6.2.en.md": "manual.en.md",
            "test-report-v0.6.2-2026-10-16.md": "test-report.zh-CN.md",
            "browser-capture-v0.6.2.md": "browser-capture.zh-CN.md",
            "browser-extension/README.md": "extension-install.zh-CN.md",
            "development-plan-after-v0.6.2-2026-10-16.md": "development-plan.zh-CN.md",
        }
    if suffix == "v063":
        extras = {
            "THIRD_PARTY_NOTICES.md": "third-party-notices.md",
            "user-manual-v0.6.3.zh-CN.md": "manual.zh-CN.md",
            "user-manual-v0.6.3.en.md": "manual.en.md",
            "test-report-v0.6.3-2026-10-16.md": "test-report.zh-CN.md",
            "browser-capture-v0.6.3.md": "browser-capture.zh-CN.md",
            "browser-extension/README.md": "extension-install.zh-CN.md",
            "development-plan-after-v0.6.3-2026-10-16.md": "development-plan.zh-CN.md",
        }
    if suffix == "v064":
        extras = {
            "THIRD_PARTY_NOTICES.md":"third-party-notices.md",
            "user-manual-v0.6.4.zh-CN.md":"manual.zh-CN.md",
            "user-manual-v0.6.4.en.md":"manual.en.md",
            "test-report-v0.6.4-2026-10-16.md":"test-report.zh-CN.md",
            "browser-capture-v0.6.4.md":"browser-capture.zh-CN.md",
            "browser-extension/README.md":"extension-install.zh-CN.md",
            "development-plan-after-v0.6.4-2026-10-16.md":"development-plan.zh-CN.md",
        }
    if suffix == "v065":
        extras = {
            "THIRD_PARTY_NOTICES.md":"third-party-notices.md",
            "user-manual-v0.6.5.zh-CN.md":"manual.zh-CN.md",
            "user-manual-v0.6.5.en.md":"manual.en.md",
            "test-report-v0.6.5-2026-10-16.md":"test-report.zh-CN.md",
            "browser-capture-v0.6.5.md":"browser-capture.zh-CN.md",
            "browser-extension/README.md":"extension-install.zh-CN.md",
            "development-plan-after-v0.6.5-2026-10-16.md":"development-plan.zh-CN.md",
        }
    if suffix == "v066":
        extras = {
            "THIRD_PARTY_NOTICES.md":"third-party-notices.md",
            "user-manual-v0.6.6.zh-CN.md":"manual.zh-CN.md",
            "user-manual-v0.6.6.en.md":"manual.en.md",
            "test-report-v0.6.6-2026-10-16.md":"test-report.zh-CN.md",
            "browser-capture-v0.6.6.md":"browser-capture.zh-CN.md",
            "browser-extension/README.md":"extension-install.zh-CN.md",
            "development-plan-after-v0.6.6-2026-10-16.md":"development-plan.zh-CN.md",
        }
    if suffix == "v067":
        extras = {
            "THIRD_PARTY_NOTICES.md":"third-party-notices.md",
            "user-manual-v0.6.7.zh-CN.md":"manual.zh-CN.md",
            "user-manual-v0.6.7.en.md":"manual.en.md",
            "test-report-v0.6.7-2026-10-16.md":"test-report.zh-CN.md",
            "browser-capture-v0.6.7.md":"browser-capture.zh-CN.md",
            "browser-extension/README.md":"extension-install.zh-CN.md",
            "development-plan-after-v0.6.7-2026-10-16.md":"development-plan.zh-CN.md",
        }
    if suffix == "v068":
        extras = {
            "THIRD_PARTY_NOTICES.md":"third-party-notices.md",
            "user-manual-v0.6.8.zh-CN.md":"manual.zh-CN.md",
            "user-manual-v0.6.8.en.md":"manual.en.md",
            "test-report-v0.6.8-2026-10-16.md":"test-report.zh-CN.md",
            "browser-capture-v0.6.8.md":"browser-capture.zh-CN.md",
            "browser-extension/README.md":"extension-install.zh-CN.md",
            "development-plan-after-v0.6.8-2026-10-16.md":"development-plan.zh-CN.md",
        }
    if suffix == "v069":
        extras = {
            "THIRD_PARTY_NOTICES.md":"third-party-notices.md",
            "user-manual-v0.6.9.zh-CN.md":"manual.zh-CN.md",
            "user-manual-v0.6.9.en.md":"manual.en.md",
            "test-report-v0.6.9-2026-10-16.md":"test-report.zh-CN.md",
            "browser-capture-v0.6.9.md":"browser-capture.zh-CN.md",
            "browser-extension/README.md":"extension-install.zh-CN.md",
            "development-plan-after-v0.6.9-2026-10-16.md":"development-plan.zh-CN.md",
        }
    if suffix == "v0610":
        extras = {
            "THIRD_PARTY_NOTICES.md":"third-party-notices.md",
            "user-manual-v0.6.10.zh-CN.md":"manual.zh-CN.md",
            "user-manual-v0.6.10.en.md":"manual.en.md",
            "test-report-v0.6.10-2026-10-17.md":"test-report.zh-CN.md",
            "browser-capture-v0.6.10.md":"browser-capture.zh-CN.md",
            "browser-extension/README.md":"extension-install.zh-CN.md",
            "development-plan-after-v0.6.10-2026-10-17.md":"development-plan.zh-CN.md",
        }
    if suffix == "v0611":
        extras = {
            "THIRD_PARTY_NOTICES.md":"third-party-notices.md",
            "user-manual-v0.6.11.zh-CN.md":"manual.zh-CN.md",
            "user-manual-v0.6.11.en.md":"manual.en.md",
            "test-report-v0.6.11-2026-10-17.md":"test-report.zh-CN.md",
            "browser-capture-v0.6.11.md":"browser-capture.zh-CN.md",
            "browser-extension/README.md":"extension-install.zh-CN.md",
            "development-plan-after-v0.6.11-2026-10-17.md":"development-plan.zh-CN.md",
        }
    def delivery_links(text):
        if suffix == "v0511":
            text = re.sub(r"\[([^]]+)\]\(((?:user-manual|acceptance|test-report|release|validation)-v0\.5\.(?:8|9|10)[^)]*\.md)\)", r"\1（源码快照内 `docs/\2`）", text)
        if suffix == "v059":
            text = re.sub(r"\[([^]]+)\]\((user-manual-v0\.5\.8[^)]*\.md|acceptance-v0\.5\.8\.md|test-report-v0\.5\.8[^)]*\.md)\)", r"\1（源码快照内 `docs/\2`）", text)
        if suffix in {"v060", "v061", "v062", "v063", "v064", "v065", "v066", "v067", "v068", "v069", "v0610", "v0611"}:
            text = text.replace("(../browser-extension/README.md)", "(" + prefix + "-extension-install.zh-CN.md)")
        replacements = {guide_name: "guide.zh-CN.md", validation_name: "validation.zh-CN.md", **extras}
        for source, destination in replacements.items():
            text = text.replace("(" + source + ")", "(" + prefix + "-" + destination + ")")
        if suffix in {"v053", "v054", "v055", "v056", "v057", "v058", "v059", "v0510", "v0511", "v060", "v061", "v062", "v063", "v064", "v065", "v066", "v067", "v068", "v069", "v0610", "v0611"}:
            text = re.sub(r"\[([^]]+)\]\((release-v0\.5\.[012345678]\.md|validation-v0\.5\.[012345678]\.md)\)", r"\1（源码快照内 `docs/\2`）", text)
        text = text.replace("[下一阶段计划](v0.5-series-development-plan.md)", "下一阶段计划（源码快照内 `docs/v0.5-series-development-plan.md`）")
        if suffix in {"v040", "v041", "v042", "v043", "v044", "v045", "v046", "v047", "v050", "v051", "v052", "v053", "v054", "v055", "v056", "v057", "v058", "v059", "v0510", "v0511", "v060", "v061", "v062", "v063", "v064", "v065", "v066", "v067", "v068", "v069", "v0610", "v0611"}:
            text = text.replace("(validation-v0.3-rc4.md)", "(https://github.com/SACO1F/Shiqian/blob/f124ed4241b92b2af29ad73bf49865212d8de92d/docs/validation-v0.3-rc4.md)")
        text = re.sub(r"\[([^]]+)\]\((evidence/[^)]+)\)", r"\1（源码快照内 `docs/\2`）", text)
        text = re.sub(r"\[([^]]+)\]\((test-report-v0\.5\.7-experience\.[0-9]+-[0-9-]+\.md)\)", r"\1（源码快照内 `docs/\2`）", text)
        return text
    guide = (root / "docs" / guide_name).read_text(encoding="utf-8")
    guide = delivery_links(guide)
    guide = guide.replace("(validation-v0.3-" + suffix + ".md)", "(" + prefix + "-validation.zh-CN.md)")
    guide = re.sub(r"!\[资料包导入预览\]\(images/[^)]+\)\n\n", "", guide)
    guide = guide.replace("开发格式与 API 约定见 [资料包格式 v1](transfer-package-format-v1.md)。", "开发格式与 API 约定保存在源码快照的 docs/transfer-package-format-v1.md。")
    (release / (prefix + "-guide.zh-CN.md")).write_text(guide, encoding="utf-8")
    validation = (root / "docs" / validation_name).read_text(encoding="utf-8")
    validation = delivery_links(validation)
    validation = validation.replace("(beta-v0.3-" + suffix + ".md)", "(" + prefix + "-guide.zh-CN.md)")
    if suffix == "beta10":
        validation = validation.replace("[RC 实机清单](beta10-rc-checklist.md)", "RC 实机清单（源码快照内 `docs/beta10-rc-checklist.md`）")
    validation = re.sub(r"\[([^]]+)\]\((evidence/[^)]+)\)", r"\1（源码快照内 `docs/\2`）", validation)
    validation = re.sub(r"\[([^]]+)\]\((images/[^)]+)\)", r"\1（源码快照内 `docs/\2`）", validation)
    validation = re.sub(r"\[([^]]+)\]\(\.\./(scripts/[^)]+)\)", r"\1（源码快照内 `\2`）", validation)
    (release / (prefix + "-validation.zh-CN.md")).write_text(validation, encoding="utf-8")
    for source, destination in extras.items():
        path = root / source if source == "THIRD_PARTY_NOTICES.md" or source.startswith("browser-extension/") else root / "docs" / source
        (release / (prefix + "-" + destination)).write_text(delivery_links(path.read_text(encoding="utf-8")), encoding="utf-8")
    files = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root).decode("utf-8").split("\0")
    excluded = {".git", "node_modules", "target", "qa", "local-only", ".build", "releases", "packages", "dist"}
    archive = release / (prefix + "-source.zip")
    count = 0
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
        for name in sorted(set(files)):
            # The delivery verification record is produced after ZIP creation.
            if not name or re.fullmatch(r"docs/evidence/(?:beta\d*|rc\d*|v040|v041|v042|v043|v044|v045|v046|v047|v050|v051|v052|v053|v054|v055|v056|v057|v058|v059|v0510|v0511|v060|v061|v062|v063|v064|v065|v066|v067|v068|v069|v0610|v0611|stable)-delivery\.log", name):
                continue
            path = root / name
            if excluded.intersection(Path(name).parts) or path.suffix.lower() in {".sqlite", ".db", ".dpapi", ".sqtagbackup", ".sqtagpack", ".pem", ".key"} or path.name.startswith(".env"):
                raise RuntimeError("Unexpected private/generated source path: " + name)
            if path.is_file():
                bundle.write(path, "Shiqian/" + name)
                count += 1
    with zipfile.ZipFile(archive) as bundle:
        if bundle.testzip() is not None:
            raise RuntimeError("Invalid source ZIP")
    if suffix in {"v060", "v061", "v062", "v063", "v064", "v065", "v066", "v067", "v068", "v069", "v0610", "v0611"}:
        extension_root = root / "browser-extension"
        manifest = json.loads((extension_root / "manifest.json").read_text(encoding="utf-8"))
        if manifest["version"] != version or manifest["manifest_version"] != 3:
            raise RuntimeError("Chrome extension version/manifest mismatch")
        extension_archive = release / (prefix + "-Chrome-extension.zip")
        with zipfile.ZipFile(extension_archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
            for source in sorted(extension_root.iterdir()):
                if not source.is_file() or source.suffix.lower() not in {".json", ".js", ".html", ".css", ".md", ".txt"}:
                    raise RuntimeError("Unexpected extension asset: " + str(source))
                bundle.write(source, "Shiqian-Chrome-extension/" + source.name)
        with zipfile.ZipFile(extension_archive) as bundle:
            if bundle.testzip() is not None:
                raise RuntimeError("Invalid extension ZIP")
    assets = sorted(path for path in release.iterdir() if path.is_file() and path.name != "SHA256SUMS.txt")
    lines = [digest(path) + "  " + path.name for path in assets]
    (release / "SHA256SUMS.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    for line in lines:
        expected, name = line.split("  ", 1)
        if digest(release / name) != expected:
            raise RuntimeError("Checksum mismatch: " + name)
    report = "Source ZIP integrity: PASS (" + str(count) + " files)\nSHA256 assets: PASS (" + str(len(assets)) + " assets)\n"
    report += "All six source versions and both native binary versions: PASS\nApplication assets match current build outputs; source/docs refreshed.\n"
    for path in assets:
        report += path.name + ": " + str(path.stat().st_size) + " bytes\n"
    (root / ("docs/evidence/beta-delivery.log" if suffix == "beta1" else "docs/evidence/" + suffix + "-delivery.log")).write_text(report, encoding="utf-8")
    print(report)


if __name__ == "__main__":
    main()
