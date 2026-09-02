"""Prints the newest off site backup object and its age in hours.

Output is one line: "<key> <age_hours>", or "none none" when the bucket holds
no backup object. Prints nothing secret.
"""
import datetime
import sys

import boto3
from botocore.config import Config


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
    c = load_creds()
    client = boto3.client(
        "s3",
        endpoint_url=c["R2_ENDPOINT"],
        aws_access_key_id=c["R2_ACCESS_KEY_ID"],
        aws_secret_access_key=c["R2_SECRET_ACCESS_KEY"],
        region_name="auto",
        config=Config(signature_version="s3v4"),
    )
    newest = None
    paginator = client.get_paginator("list_objects_v2")
    for page in paginator.paginate(Bucket=c["R2_BUCKET"]):
        for obj in page.get("Contents", []):
            if not obj["Key"].startswith("astraeus_"):
                continue
            if newest is None or obj["LastModified"] > newest["LastModified"]:
                newest = obj
    if newest is None:
        print("none none")
        return
    now = datetime.datetime.now(datetime.timezone.utc)
    age_h = int((now - newest["LastModified"]).total_seconds() // 3600)
    print(f"{newest['Key']} {age_h}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001
        print(f"error {exc}", file=sys.stderr)
        sys.exit(1)
