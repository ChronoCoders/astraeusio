"""Uploads a file to R2 with multipart, then verifies it.

The multipart ETag is the md5 of the concatenated part md5s with a dash and the
part count, not the md5 of the file, so it is never compared against the local
file. Verification is the sha256 stored as object metadata at upload time, read
back with head_object, plus the object size.

Usage: r2_upload.py <local file> <object key> [--download-verify <scratch dir>]
"""
import hashlib
import os
import sys

import boto3
from boto3.s3.transfer import TransferConfig
from botocore.config import Config


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(8 * 1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


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
    local = sys.argv[1]
    key = sys.argv[2]
    download_dir = None
    if "--download-verify" in sys.argv:
        download_dir = sys.argv[sys.argv.index("--download-verify") + 1]

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

    local_size = os.path.getsize(local)
    local_sha = sha256_of(local)
    print(f"local: {local} size={local_size} sha256={local_sha}")

    # 64 MiB parts, well under the 5 GiB per part cap and far under the 300 MiB
    # single request limit that broke the previous REST upload.
    cfg = TransferConfig(
        multipart_threshold=64 * 1024 * 1024,
        multipart_chunksize=64 * 1024 * 1024,
        max_concurrency=2,
    )
    client.upload_file(
        local, bucket, key,
        ExtraArgs={"Metadata": {"sha256": local_sha}, "ContentType": "application/gzip"},
        Config=cfg,
    )
    print("upload complete")

    head = client.head_object(Bucket=bucket, Key=key)
    remote_size = head["ContentLength"]
    remote_sha = head.get("Metadata", {}).get("sha256", "")
    parts = head.get("ETag", "").strip('"')
    multipart = "-" in parts
    print(f"remote: size={remote_size} sha256_metadata={remote_sha} multipart_etag={multipart}")

    ok = True
    if remote_size != local_size:
        print(f"FAIL size mismatch: local {local_size} remote {remote_size}")
        ok = False
    else:
        print("size matches")
    if remote_sha != local_sha:
        print(f"FAIL sha256 metadata mismatch: local {local_sha} remote {remote_sha}")
        ok = False
    else:
        print("sha256 metadata matches")

    if download_dir:
        tmp = os.path.join(download_dir, "verify_download.tmp")
        client.download_file(bucket, key, tmp)
        back = sha256_of(tmp)
        size_back = os.path.getsize(tmp)
        print(f"downloaded: size={size_back} sha256={back}")
        if back != local_sha:
            print("FAIL downloaded sha256 does not match local")
            ok = False
        else:
            print("downloaded sha256 matches local")
        os.remove(tmp)
        print("scratch download removed")

    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
