"""Private, resumable review checkpoints. Never writes photos or XMP."""
import argparse
from contextlib import closing
import json
import math
import os
from pathlib import Path
import sqlite3
import tempfile

def require(ok, message):
    if not ok:
        raise ValueError(message)

def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))

def validate_manifest(m):
    require(isinstance(m, dict), "manifest must be an object")
    root = Path(m["root"])
    require(root.is_absolute() and root.is_dir(), "root must be an existing absolute directory")
    root = root.resolve()
    require(isinstance(m["photos"], list) and m["photos"], "photos must be a nonempty list")
    ids, paths, targets = set(), set(), set()
    for p in m["photos"]:
        require(isinstance(p["id"], str) and p["id"].strip() and p["id"] not in ids, "invalid/duplicate photo id")
        ids.add(p["id"])
        require(isinstance(p["group"], str) and p["group"].strip(), "missing group")
        require(type(p["protected"]) is bool, "protected must be boolean")
        r = p["existing_rating"]
        require(r is None or type(r) is int and 0 <= r <= 5, "invalid existing rating")
        require(p["existing_label"] is None or isinstance(p["existing_label"], str), "invalid existing label")
        require(isinstance(p["files"], list) and p["files"], "files must be nonempty")
        stem = None
        for name in p["files"]:
            require(isinstance(name, str) and not Path(name).is_absolute(), "file must be relative")
            f = (root / name).resolve()
            require(f.is_relative_to(root) and f.is_file(), "file missing or outside root")
            key = str(f).casefold()
            require(key not in paths, "source path reused")
            paths.add(key)
            target = str(f.with_suffix(".xmp")).casefold()
            if stem is None:
                require(target not in targets, "sidecar target reused across photos")
                targets.add(target)
                stem = target
            require(target == stem, "one photo must contain only same-directory same-stem pairs")
    return m

def validate_decisions(m, decisions):
    photos = {p["id"]: p for p in m["photos"]}
    for ident, d in decisions.items():
        require(ident in photos and d["id"] == ident, "unknown/mismatched photo id")
        p = photos[ident]
        r, label = d["rating"], d["label"]
        require(r is None or type(r) is int and r in (3, 4, 5), "rating must preserve or be 3/4/5")
        require(label in (None, "red", "green"), "invalid label")
        require(isinstance(d["reason"], str) and d["reason"].strip(), "reason required")
        require(isinstance(d["evidence"], list) and d["evidence"] and
                all(isinstance(e, str) and e.strip() for e in d["evidence"]), "evidence references required")
        for k in ("full_reviewed", "native_detail", "independent_value"):
            require(type(d[k]) is bool, k + " must be boolean")
        score = d["artistic_score"]
        require(score is None or type(score) in (int, float) and math.isfinite(score) and 0 <= score <= 10, "invalid artistic score")
        require(d["reject_kind"] in (None, "technical", "artistic", "superseded"), "invalid rejection kind")
        require(d["alternative_id"] is None or isinstance(d["alternative_id"], str), "invalid alternative")
        require(d["edit"] is None or isinstance(d["edit"], str) and d["edit"].strip(), "invalid edit")
        require(not p["protected"] or r is None and label is None, "protected sequence must remain unmarked")
        require(p["existing_rating"] in (None, 0) or r in (None, p["existing_rating"]), "preserve existing user rating")
        require(not p["existing_label"] or label is None, "preserve existing user label; reconcile conflict separately")
        if r in (4, 5):
            require(d["full_reviewed"] and d["native_detail"], "4/5 stars require full and native detail review")
        if label == "green":
            require(d["edit"] is not None and d["full_reviewed"], "green requires a concrete edit and full review")
        if label == "red":
            require(r is None and p["existing_rating"] in (None, 0), "red cannot coexist with keeper/user stars")
            require(d["full_reviewed"], "red requires full image review")
            kind = d["reject_kind"]
            require(kind is not None, "red requires reject_kind")
            if kind == "technical":
                require(d["native_detail"], "technical red requires native subject detail")
            else:
                require(not d["independent_value"] and score is not None, "weak image with unique value must be preserved")
                require(score <= (4 if kind == "artistic" else 5), "rejection score exceeds threshold")
            if kind == "superseded":
                alt = d["alternative_id"]
                require(alt != ident and alt in decisions and alt in photos, "retained alternative required")
                other = decisions[alt]
                require(photos[alt]["group"] == p["group"] and not photos[alt]["protected"], "alternative must be in same unprotected group")
                require(other["label"] != "red" and other["full_reviewed"] and
                        (other["rating"] in (3, 4, 5) or photos[alt]["existing_rating"] in (3, 4, 5)),
                        "alternative must be a reviewed keeper")
        else:
            require(d["reject_kind"] is None and d["alternative_id"] is None, "non-red cannot carry rejection fields")
    for group in {p["group"] for p in m["photos"]}:
        members = [p["id"] for p in m["photos"] if p["group"] == group]
        require(len(members) == 1 or not all(i in decisions and decisions[i]["label"] == "red" for i in members),
                "entire group rejected; retain fallback or leave uncertain")

def init(db, manifest):
    m = validate_manifest(load(manifest))
    db = Path(db)
    require(not db.resolve().is_relative_to(Path(m["root"]).resolve()), "checkpoint must be outside photo root")
    # O_EXCL prevents accidental replacement of an existing checkpoint.
    fd = os.open(db, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    os.close(fd)
    with closing(sqlite3.connect(db)) as conn, conn:
        conn.execute("CREATE TABLE run (manifest TEXT NOT NULL)")
        conn.execute("CREATE TABLE decisions (id TEXT PRIMARY KEY, body TEXT NOT NULL)")
        conn.execute("INSERT INTO run VALUES (?)", (json.dumps(m, ensure_ascii=False),))

def read_run(conn):
    m = json.loads(conn.execute("SELECT manifest FROM run").fetchone()[0])
    ds = {i: json.loads(b) for i, b in conn.execute("SELECT id, body FROM decisions")}
    return m, ds

def connect_existing(db):
    require(Path(db).is_file(), "checkpoint does not exist")
    return sqlite3.connect(db)

def record(db, batch_path):
    batch = load(batch_path)
    require(isinstance(batch, list) and batch, "decision batch must be a nonempty list")
    with closing(connect_existing(db)) as conn, conn:
        conn.execute("BEGIN IMMEDIATE")
        m, ds = read_run(conn)
        seen = set()
        for d in batch:
            require(d["id"] not in seen, "duplicate id in decision batch")
            seen.add(d["id"])
            ds[d["id"]] = d
        validate_decisions(m, ds)
        for d in batch:
            conn.execute("INSERT INTO decisions VALUES (?, ?) ON CONFLICT(id) DO UPDATE SET body=excluded.body",
                         (d["id"], json.dumps(d, ensure_ascii=False)))

def status(db):
    with closing(connect_existing(db)) as conn:
        m, ds = read_run(conn)
    pending = [p["id"] for p in m["photos"] if p["id"] not in ds]
    return {"total": len(m["photos"]), "reviewed": len(ds), "pending": pending,
            "red": sum(d["label"] == "red" for d in ds.values()),
            "green": sum(d["label"] == "green" for d in ds.values())}

def export(db, dest):
    with closing(connect_existing(db)) as conn:
        m, ds = read_run(conn)
    validate_decisions(m, ds)
    require(len(ds) == len(m["photos"]), "incomplete review; use status to find pending photos")
    dest = Path(dest)
    require(not dest.exists(), "export destination already exists")
    root = Path(m["root"]).resolve()
    require(not dest.resolve().is_relative_to(root), "export must be outside photo root")
    payload = {"schema_version": 1, "state": "reviewed_not_applied",
               "manifest": m, "decisions": [ds[p["id"]] for p in m["photos"]]}
    # Publish a fully flushed artifact without clobber. Windows rename fails if
    # the destination exists; POSIX rename would overwrite, so use a hard link.
    fd, name = tempfile.mkstemp(prefix=".review-", suffix=".json", dir=dest.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as f:
            json.dump(payload, f, ensure_ascii=False, indent=2, allow_nan=False)
            f.flush()
            os.fsync(f.fileno())
        if os.name == "nt":
            os.rename(name, dest)
        else:
            os.link(name, dest)
    finally:
        Path(name).unlink(missing_ok=True)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for command in ("init", "record", "status", "export"):
        p = sub.add_parser(command)
        p.add_argument("db")
        if command != "status":
            p.add_argument("path")
    args = parser.parse_args()
    try:
        if args.command == "status":
            print(json.dumps(status(args.db), ensure_ascii=False))
        else:
            globals()[args.command](args.db, args.path)
    except (ValueError, KeyError, TypeError, OSError, sqlite3.Error) as e:
        parser.exit(2, f"{type(e).__name__}: {e}\n")

if __name__ == "__main__":
    main()
