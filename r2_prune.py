"""Off site retention for the backup bucket.

Deletes backup objects older than RETENTION_DAYS. Two guards:

  the keep marker, an object named retention-keep.txt holding one key per line,
  which is never deleted whatever its age. The marker lives in the bucket so it
  survives a host rebuild, and the exemption is data rather than a filename
  written into this script.

  dry run by default. Deleting requires --apply, so the listing can be reviewed
  first.

The previous version matched keys ending in .duckdb.gz while objects are named
astraeus_YYYYMMDD.gz, so it never matched and never deleted anything.

Usage: r2_prune.py [--apply]
"""
import datetime
import sys

import boto3
from botocore.config import Config

RETENTION_DAYS = 30
KEEP_MARKER = "retention-keep.txt"
PREFIX = "astraeus_"
SUFFIX = ".gz"


def load_creds(path="/opt/astraeusio/.r2-s3-key"):
    creds = {}
    with open(path) as fh:
        for line in fh:
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                k, v = line.split("=", 1)
                creds[k] = v
    return creds


def main():
    apply = "--apply" in sys.argv
    c = load_creds()
    client = boto3.client(
        "s3",
        endpoint_url=c["R2_ENDPOINT"],
        aws_access_key_id=c["R2_ACCESS_KEY_ID"],
        aws_secret_access_key=c["R2_SECRET_ACCESS_KEY"],
        region_name="auto",
        config=Config(signature_version="s3v4"),
    )
    bucket = c["R2_BUCKET"]

    keep = set()
    try:
        body = client.get_object(Bucket=bucket, Key=KEEP_MARKER)["Body"].read().decode()
        for line in body.splitlines():
            line = line.strip()
            if line and not line.startswith("#"):
                keep.add(line)
    except client.exceptions.NoSuchKey:
        print(f"keep marker {KEEP_MARKER} not found, refusing to delete anything")
        sys.exit(1)

    print(f"keep marker lists {len(keep)} protected object(s):")
    for k in sorted(keep):
        print("   ", k)

    cutoff = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(days=RETENTION_DAYS)
    objects = []
    paginator = client.get_paginator("list_objects_v2")
    for page in paginator.paginate(Bucket=bucket):
        objects.extend(page.get("Contents", []))

    to_delete, protected, retained = [], [], []
    for o in objects:
        k = o["Key"]
        if not (k.startswith(PREFIX) and k.endswith(SUFFIX)):
            continue
        if k in keep:
            protected.append(k)
        elif o["LastModified"] < cutoff:
            to_delete.append((k, o["Size"], o["LastModified"]))
        else:
            retained.append(k)

    print()
    print(f"retention {RETENTION_DAYS} days, cutoff {cutoff.isoformat()}")
    print(f"  within retention, kept: {len(retained)}")
    print(f"  protected by marker:    {len(protected)}  {protected}")
    print(f"  older than retention:   {len(to_delete)}")
    total = 0
    for k, size, lm in sorted(to_delete):
        total += size
        print(f"    would delete  {k}  {size} bytes  {lm.isoformat()}")
    print(f"  total that would be freed: {total} bytes")

    if not apply:
        print()
        print("DRY RUN. Nothing was deleted. Re-run with --apply to delete.")
        return

    for k, _size, _lm in to_delete:
        client.delete_object(Bucket=bucket, Key=k)
        print(f"deleted {k}")
    print(f"deleted {len(to_delete)} object(s)")


if __name__ == "__main__":
    main()
