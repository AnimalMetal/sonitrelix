"""Archive the source used by the selected Windows FastPlugs build."""

import argparse
import io
from pathlib import Path
import subprocess
import tempfile
from zipfile import ZIP_DEFLATED, ZipFile


def git_archive(repo, *paths):
    return subprocess.check_output(
        ["git", "-C", str(repo), "archive", "--format=zip", "HEAD", *paths]
    )


def copy_git_tree(archive, repo, destination, *paths):
    with ZipFile(io.BytesIO(git_archive(repo, *paths))) as tree:
        for member in tree.infolist():
            if not member.is_dir():
                archive.writestr(f"{destination}/{member.filename}", tree.read(member))
    revision = subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True
    )
    archive.writestr(f"{destination}/SOURCE-REVISION.txt", revision)


def fastplay_files(build):
    # Include all of the FastPlay code compiled into FastPlugs, plus the source
    # of its generated parameter tables and the HRTF data's notices.
    files = {"src/effect_chain.cpp", "src/spatial/SADIE-README.txt",
             "src/spatial/SADIE-LICENSE.txt"}
    for log in build.glob("*.dir/Release/*.tlog/CL.*.1.tlog"):
        if log.name not in ("CL.read.1.tlog", "CL.write.1.tlog"):
            continue
        for line in log.read_text(encoding="utf-16").splitlines():
            path = line.removeprefix("^").split("|", 1)[0].replace("\\", "/").lower()
            if "/_deps/fastplay-src/" in path and Path(path).suffix in (".c", ".cpp", ".h"):
                files.add(path.split("/_deps/fastplay-src/", 1)[1])
    if "src/reverb/reverb.cpp" not in files or "src/spatial/hrtf_data.cpp" not in files:
        raise ValueError("Build the FastPlugs CLAP and smoke targets before packaging")
    return sorted(files)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fastplugs_repo", type=Path)
    parser.add_argument("cmake_build", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    deps = args.cmake_build / "_deps"
    with tempfile.TemporaryDirectory() as temp:
        vendor = Path(temp) / "vendor"
        subprocess.run(["cargo", "vendor", "--locked", str(vendor)], cwd=root, check=True)
        with ZipFile(args.output, "x", compression=ZIP_DEFLATED, strict_timestamps=False) as archive:
            copy_git_tree(archive, root, "Sonitrelix")
            archive.writestr(
                "Sonitrelix/.cargo/config.toml",
                '[source.crates-io]\nreplace-with = "vendored-sources"\n'
                '[source.vendored-sources]\ndirectory = "vendor"\n',
            )
            for file in vendor.rglob("*"):
                if file.is_file():
                    archive.write(file, f"Sonitrelix/vendor/{file.relative_to(vendor).as_posix()}")
            archive.write(root / "docs/FASTPLUGS_BUILD.md", "BUILD.md")
            copy_git_tree(archive, args.fastplugs_repo, "FastPlugs")
            for name in ("fastroute", "rnnoise", "speexdsp", "signalsmith_stretch", "clap-wrapper"):
                copy_git_tree(archive, deps / f"{name}-src", f"deps/{name}")
            copy_git_tree(archive, deps / "fastplay-src", "deps/fastplay",
                          *fastplay_files(args.cmake_build))
            copy_git_tree(archive, deps / "clap-wrapper-build/cpm/clap", "deps/clap")
            vst3 = args.cmake_build / "cpm/vst3sdk"
            copy_git_tree(archive, vst3, "deps/vst3sdk")
            for name in ("base", "cmake", "pluginterfaces", "public.sdk"):
                copy_git_tree(archive, vst3 / name, f"deps/vst3sdk/{name}")


if __name__ == "__main__":
    main()
