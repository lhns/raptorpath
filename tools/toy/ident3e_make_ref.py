"""Make ident3e_ref.json from the base commit's code (git archive), one run per configuration of ident3e_cfgs.cfgs.

    python ident3e_make_ref.py [BASE_REV]        # default ba3cf31c (proof stage 1's head, the base of stage 3e)
"""
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))


def emit(root, out):
    sys.path.insert(0, root)
    import toy2
    import batch3c
    import batch3d
    import ident3e_cfgs
    assert os.path.dirname(os.path.abspath(toy2.__file__)) == os.path.abspath(root), toy2.__file__
    batch3d.build()
    res = {}
    for nm, c in ident3e_cfgs.cfgs(toy2, batch3d, batch3c).items():
        res[nm] = ident3e_cfgs.digest(toy2.run(c))
        print("ran", nm, flush=True)
    with open(out, "w") as f:
        json.dump(res, f, indent=0, sort_keys=True)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--emit":
        emit(sys.argv[2], sys.argv[3])
        return
    base = sys.argv[1] if len(sys.argv) > 1 else "ba3cf31c"
    root = os.path.join(HERE, "..", "..")
    tar = subprocess.run(["git", "archive", base + ":tools/toy"], cwd=root, check=True, capture_output=True).stdout
    with tempfile.TemporaryDirectory() as td:
        os.makedirs(os.path.join(td, "old"))
        tarfile.open(fileobj=io.BytesIO(tar)).extractall(os.path.join(td, "old"))
        shutil_copy = os.path.join(HERE, "ident3e_cfgs.py")
        with open(shutil_copy, "rb") as f, open(os.path.join(td, "old", "ident3e_cfgs.py"), "wb") as g:
            g.write(f.read())
        subprocess.run([sys.executable, os.path.abspath(__file__), "--emit", os.path.join(td, "old"), os.path.join(HERE, "ident3e_ref.json")],
                       check=True, cwd=td)
    print("wrote ident3e_ref.json from", base)


if __name__ == "__main__":
    main()
