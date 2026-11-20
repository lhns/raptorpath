"""A tiny throwaway git repository for tests: fixed identity, fixed dates, no signing."""
import os
import subprocess
from pathlib import Path

IDENTITY = ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false",
            "-c", "tag.gpgsign=false", "-c", "core.autocrlf=false"]


class GitRepo:
    def __init__(self, path):
        self.path = Path(path)
        self.path.mkdir(parents=True, exist_ok=True)
        self.git("init", "-q", "-b", "main")

    def git(self, *args, date=None):
        env = dict(os.environ)
        if date:
            env["GIT_AUTHOR_DATE"] = env["GIT_COMMITTER_DATE"] = f"{date}T12:00:00+0000"
        r = subprocess.run(["git", "-C", str(self.path), *IDENTITY, *args], capture_output=True, text=True,
                           encoding="utf-8", env=env)
        if r.returncode:
            raise RuntimeError(f"git {' '.join(args)} failed: {r.stderr}")
        return r.stdout.strip()

    def commit(self, message, date):
        """An empty commit (one git call, then one for its sha); returns the full sha."""
        self.git("commit", "-q", "--allow-empty", "-m", message, date=date)
        return self.git("rev-parse", "HEAD")

    def branch(self, name, start="HEAD"):
        self.git("checkout", "-q", "-b", name, start)

    def checkout(self, name):
        self.git("checkout", "-q", name)

    def merge(self, branch, message, date):
        self.git("merge", "-q", "--no-ff", "-m", message, branch, date=date)
        return self.git("rev-parse", "HEAD")

    def tag(self, name):
        self.git("tag", name)
