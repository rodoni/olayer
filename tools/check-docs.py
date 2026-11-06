#!/usr/bin/env python3
"""Check local Markdown links and navigation without network access or dependencies."""
from collections import deque
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
LINK = re.compile(r"\]\((<?[^\s)]+>?)\)")


def prose(text):
    """Ignore fenced code examples; keep actual Markdown headings/links."""
    lines = []
    fence = None
    for line in text.splitlines():
        match = re.match(r"^\s*(`{3,}|~{3,})", line)
        if match:
            marker = match[1]
            if fence is None:
                fence = marker
            elif marker[0] == fence[0] and len(marker) >= len(fence):
                fence = None
            continue
        if fence is None:
            lines.append(line)
    return lines


def anchors(text):
    found = set(re.findall(r'<a\s+(?:id|name)=[\"\']([^\"\']+)', text))
    counts = {}
    for line in prose(text):
        match = re.match(r"^#{1,6}\s+(.+?)(?:\s+#+)?$", line)
        if not match:
            continue
        heading = re.sub(r"<[^>]*>", "", match[1]).lower()
        heading = re.sub(r"[^\w\- ]", "", heading, flags=re.UNICODE).replace(" ", "-")
        count = counts.get(heading, 0)
        counts[heading] = count + 1
        found.add(heading + (f"-{count}" if count else ""))
    return found


def check():
    docs = sorted(DOCS.rglob("*.md"))
    files = [ROOT / "README.md", ROOT / "AGENTS.md", *docs]
    graph = {path.resolve(): set() for path in docs}
    failures = []
    checked = 0
    for path in files:
        for number, line in enumerate(prose(path.read_text()), 1):
            for match in LINK.finditer(line):
                raw = match[1].strip("<>")
                url = urlsplit(raw)
                if url.scheme or url.netloc or raw.startswith("/"):
                    continue
                target = (path.parent / unquote(url.path)).resolve() if url.path else path.resolve()
                checked += 1
                if not target.exists():
                    failures.append(f"{path.relative_to(ROOT)}: broken target {raw}")
                    continue
                if target in graph and path.resolve() in graph:
                    graph[path.resolve()].add(target)
                if url.fragment and target.suffix == ".md" and unquote(url.fragment) not in anchors(target.read_text()):
                    failures.append(f"{path.relative_to(ROOT)}: missing anchor {raw}")
    entry = DOCS / "README.md"
    visited = set()
    pending = deque([entry.resolve()])
    while pending:
        current = pending.popleft()
        if current in visited:
            continue
        visited.add(current)
        pending.extend(graph.get(current, set()) - visited)
    for path in graph:
        if path not in visited:
            failures.append(f"{path.relative_to(ROOT)}: unreachable from docs/README.md")
    if failures:
        print("\n".join(failures))
        print(f"\n{len(failures)} documentation issues ({checked} local links checked).")
        return 1
    print(f"Documentation OK: {len(docs)} pages, {checked} local links, all pages reachable.")
    return 0


if __name__ == "__main__":
    sys.exit(check())
