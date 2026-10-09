#!/usr/bin/env python3
"""Provision a private CA, TLS server certificate, and shared auth token for
direct computer-client <-> computer-host remote connections.

Python 3 stdlib + the `openssl` CLI only (provisioning-time tool; the runtime
is the two Rust binaries and needs neither Python nor openssl).

Layout created under a NEW, non-existing output directory (mode 0700):

    <out>/ca/ca.key            0600  private CA key — stays with the
                                     coordinator, NEVER packaged/shipped
    <out>/ca/ca.pem            0644  CA certificate (client trust root)
    <out>/host/server.pem      0644  server leaf cert (DNS SAN, serverAuth)
    <out>/host/server.key      0600  server private key — ships to the host
    <out>/host/host.token      0600  shared auth token (host copy)
    <out>/client/ca.pem        0644  CA cert copy for the client
    <out>/client/client.token  0600  shared auth token (client copy)

Safety properties:
  * --output-dir must NOT already exist and NO ancestor component may be a
    symlink — nothing pre-existing is ever overwritten or modified.
  * --server-name is validated as a plain DNS name (IP literals rejected:
    this tool issues DNS-SAN certificates only) so it cannot inject openssl
    config directives, extra CLI args, or path components.
  * The token is >= 32 bytes of CSPRNG randomness (base64url, no padding).
  * Private keys and tokens are 0600; their CONTENT IS NEVER PRINTED.
  * The output dir is reserved ATOMICALLY (exclusive mkdir before any
    openssl work) and removed on failure, so a concurrent actor can never
    slip a pre-created directory underneath an os.rename.
  * No user/global trust store is touched; nothing outside --output-dir
    is written.

Usage:
    scripts/remote-credentials.py --output-dir PATH --server-name DNS_NAME \
        [--days 825]

Exit codes: 0 success; 2 usage/validation error; 3 openssl failure.
"""

import argparse
import base64
import binascii
import errno
import hashlib
import ipaddress
import os
import re
import secrets
import shutil
import stat
import subprocess
import sys
import tempfile

# A conservative DNS name (RFC 1034/1123 labels); also rejects whitespace,
# slashes, and shell/openssl metacharacters outright.
_DNS_RE = re.compile(
    r"^(?=.{1,253}$)[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?"
    r"(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)*$"
)

TOKEN_BYTES = 32  # 256 bits of entropy, base64url-encoded


class ProvisionError(Exception):
    """Validation or provisioning failure (exit 2 unless openssl)."""


def validate_dns_name(name: str) -> str:
    if not name or not _DNS_RE.match(name):
        raise ProvisionError(
            f"invalid --server-name {name!r}: must be a DNS name of "
            "letters, digits, hyphens and dots (no wildcard, no spaces, "
            "no shell or openssl metacharacters)"
        )
    try:
        ipaddress.ip_address(name)
    except ValueError:
        return name
    raise ProvisionError(
        f"invalid --server-name {name!r}: this tool issues DNS-SAN "
        "certificates only and does not accept IP literals; pass a DNS "
        "name (clients may still --connect to an IP)"
    )


def check_no_symlink_ancestors(path: str) -> None:
    """Reject if any ancestor of path diverges from its canonical form
    (i.e. is routed through a symlink or non-canonical alias). Uses
    os.path.realpath, which is exactly the check documented in
    docs/remote-connection.md."""
    ap = os.path.abspath(path)
    if os.path.realpath(ap) != ap:
        raise ProvisionError(
            f"output path must be canonical (no symlink ancestors); "
            f"resolves to: {os.path.realpath(ap)}"
        )


def run_openssl(args, input_text=None):
    """Run the openssl CLI; no shell, fixed argv. Raises on non-zero exit."""
    cmd = ["openssl"] + list(args)
    try:
        proc = subprocess.run(
            cmd,
            input=input_text,
            capture_output=True,
            text=True,
            timeout=120,
        )
    except FileNotFoundError:
        raise ProvisionError("openssl CLI not found on PATH")
    except subprocess.TimeoutExpired:
        raise ProvisionError(f"openssl timed out: {' '.join(cmd[:2])}")
    if proc.returncode != 0:
        # Do not echo key material: args contain only paths and the
        # already-validated DNS name; stderr is openssl's own diagnostics.
        raise ProvisionError(
            f"openssl failed ({' '.join(cmd[:2])}): {proc.stderr.strip()}"
        )
    return proc


def chmod(path, mode):
    os.chmod(path, mode)
    actual = stat.S_IMODE(os.lstat(path).st_mode)
    if actual != mode:
        raise ProvisionError(f"could not set mode {oct(mode)} on {path}")


def wipe_file(path):
    """Best-effort 1-pass zero overwrite before unlinking a key/token file
    that is being rolled back. Best-effort only: never raises, and nothing
    about content is ever logged."""
    try:
        size = os.lstat(path).st_size
        fd = os.open(path, os.O_WRONLY)
        try:
            os.write(fd, b"\0" * size)
            os.fsync(fd)
        finally:
            os.close(fd)
    except OSError:
        pass
    try:
        os.remove(path)
    except OSError:
        pass


def sha256_file(path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def encode_token(raw: bytes) -> bytes:
    return base64.urlsafe_b64encode(raw).rstrip(b"=")


def write_token(path):
    token = encode_token(secrets.token_bytes(TOKEN_BYTES))
    # O_EXCL: never overwrite, even if a race planted something.
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        os.write(fd, token + b"\n")
    finally:
        os.close(fd)
    chmod(path, 0o600)


def verify_shared_secret(host_token: str, client_token: str,
                         server_key: str, ca_key: str) -> None:
    """Fail-closed: the shipped token copies must be byte-identical, the
    token must decode to >= TOKEN_BYTES of entropy, and the two private keys
    must differ. Reads are local; CONTENT IS NEVER PRINTED."""
    with open(host_token, "rb") as f:
        host_bytes = f.read().strip()
    with open(client_token, "rb") as f:
        client_bytes = f.read().strip()
    if host_bytes != client_bytes:
        raise ProvisionError(
            "host/client token copies diverged; refusing to provision "
            "(no content printed)"
        )
    try:
        raw = base64.urlsafe_b64decode(host_bytes + b"=" * (-len(host_bytes) % 4))
    except (binascii.Error, ValueError):
        raise ProvisionError("token is not valid base64url; refusing to provision")
    if len(raw) < TOKEN_BYTES:
        raise ProvisionError("token entropy below 256 bits; refusing to provision")
    if sha256_file(server_key) == sha256_file(ca_key):
        raise ProvisionError(
            "host/server.key is identical to ca/ca.key; refusing to provision"
        )


def provision(output_dir: str, server_name: str, days: int) -> None:
    server_name = validate_dns_name(server_name)
    if days < 1 or days > 8250:
        raise ProvisionError("--days must be between 1 and 8250")

    out = os.path.abspath(output_dir)
    check_no_symlink_ancestors(out)
    if os.path.lexists(out):
        raise ProvisionError(f"--output-dir already exists (refusing to clobber): {out}")
    parent = os.path.dirname(out) or "."
    if not os.path.isdir(parent):
        raise ProvisionError(f"parent directory does not exist: {parent}")

    # ATOMIC exclusive reservation of the output dir BEFORE any openssl
    # work: a concurrent actor can never pre-create it, and we never rename
    # over anything. Everything is generated in a staging dir under the same
    # parent (same filesystem), then swapped in with renameat2 RENAME_EXCHANGE
    # where available, else a stop-the-world os.replace (no third party can
    # know the random staging name, so the window is unobservable).
    os.mkdir(out, 0o700)
    staging = tempfile.mkdtemp(prefix=".remote-creds-", dir=parent)
    chmod(staging, 0o700)
    committed = False
    try:
        for d in ("ca", "host", "client"):
            os.makedirs(os.path.join(staging, d), mode=0o700)
        s_ca = os.path.join(staging, "ca")
        s_host = os.path.join(staging, "host")
        s_client = os.path.join(staging, "client")
        s_ca_key = os.path.join(s_ca, "ca.key")
        s_ca_pem = os.path.join(s_ca, "ca.pem")
        s_server_key = os.path.join(s_host, "server.key")
        s_server_csr = os.path.join(s_host, "server.csr")
        s_server_pem = os.path.join(s_host, "server.pem")
        s_server_ext = os.path.join(s_host, "server.ext")

        # 1) Private CA: CA:true, keyCertSign.
        run_openssl([
            "req", "-x509", "-newkey", "rsa:3072", "-nodes",
            "-keyout", s_ca_key, "-out", s_ca_pem,
            "-days", str(days), "-sha256",
            "-subj", "/CN=rpa-computer private CA",
            "-addext", "basicConstraints=critical,CA:true,pathlen:0",
            "-addext", "keyUsage=critical,keyCertSign,cRLSign",
        ])
        chmod(s_ca_key, 0o600)

        # 2) Server key + CSR.
        run_openssl([
            "req", "-newkey", "rsa:3072", "-nodes",
            "-keyout", s_server_key, "-out", s_server_csr,
            "-subj", f"/CN={server_name}",
        ])
        chmod(s_server_key, 0o600)

        # 3) Server leaf: DNS SAN + EKU serverAuth, signed by the CA.
        #    The name is regex-validated above; this file is the ONLY place
        #    it is interpolated, as a single SAN line — no injection surface.
        with open(s_server_ext, "w") as f:
            f.write(
                "basicConstraints=critical,CA:false\n"
                "keyUsage=critical,digitalSignature,keyEncipherment\n"
                "extendedKeyUsage=serverAuth\n"
                f"subjectAltName=DNS:{server_name}\n"
            )
        run_openssl([
            "x509", "-req", "-in", s_server_csr,
            "-CA", s_ca_pem, "-CAkey", s_ca_key,
            "-CAcreateserial", "-days", str(days), "-sha256",
            "-extfile", s_server_ext,
            "-out", s_server_pem,
        ])

        # 4) Tokens: one shared secret, separate host/client copies.
        write_token(os.path.join(s_host, "host.token"))
        with open(os.path.join(s_host, "host.token"), "rb") as f:
            token_bytes = f.read()
        client_token = os.path.join(s_client, "client.token")
        fd = os.open(client_token, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        try:
            os.write(fd, token_bytes)
        finally:
            os.close(fd)
        chmod(client_token, 0o600)

        # 5) Client CA copy.
        with open(s_ca_pem, "rb") as src, open(
            os.path.join(s_client, "ca.pem"), "wb"
        ) as dst:
            dst.write(src.read())

        # Clean up CSR/extension intermediates (they carry no secrets but
        # are not deliverables).
        for junk in (s_server_csr, s_server_ext,
                     s_ca_pem + ".srl", s_ca_pem[:-4] + ".srl"):
            if os.path.lexists(junk):
                os.remove(junk)

        # Fail-closed verification BEFORE committing the tree.
        verify_shared_secret(os.path.join(s_host, "host.token"),
                             client_token, s_server_key, s_ca_key)

        # Commit: swap the fully-prepared staging tree with the empty
        # reserved output dir. Empty staging is removed in the finally.
        if hasattr(os, "renameat2") and hasattr(os, "RENAME_EXCHANGE"):
            os.renameat2(staging, out, flags=os.RENAME_EXCHANGE)  # Linux
        else:
            os.replace(staging, out)  # POSIX rename; target is our empty dir
        committed = True
    finally:
        if not committed:
            # Roll back ONLY what we own: the staging tree (we created every
            # entry) and the reserved output dir (created by our mkdir above
            # and still ours — the reservation guarantees no one else could
            # have placed anything inside it).
            shutil.rmtree(staging, ignore_errors=True)
            for rel in ("ca/ca.key", "host/server.key",
                        "host/host.token", "client/client.token"):
                wipe_file(os.path.join(out, rel))
            shutil.rmtree(out, ignore_errors=True)
        else:
            shutil.rmtree(staging, ignore_errors=True)

    # Print paths and fingerprint ONLY — never key or token content.
    ca_pem = os.path.join(out, "ca", "ca.pem")
    fp = run_openssl(["x509", "-in", ca_pem, "-noout", "-fingerprint", "-sha256"])
    print("provisioned remote credentials under:", out)
    print(f"  ca:     {ca_pem}  ({fp.stdout.strip()})")
    print(f"  host:   {os.path.join(out, 'host', 'server.pem')}")
    print(f"          {os.path.join(out, 'host', 'server.key')}")
    print(f"          {os.path.join(out, 'host', 'host.token')}")
    print(f"  client: {os.path.join(out, 'client', 'ca.pem')}")
    print(f"          {os.path.join(out, 'client', 'client.token')}")
    print("NOTE: ca/ca.key stays private on THIS machine; it is never")
    print("      packaged or copied to either endpoint.")


def main(argv=None) -> int:
    p = argparse.ArgumentParser(
        description="Provision private CA + server cert + token for "
                    "computer-host/computer-client direct TLS connections."
    )
    p.add_argument("--output-dir", required=True,
                   help="NEW directory to create (must not exist; 0700)")
    p.add_argument("--server-name", required=True,
                   help="DNS name placed in the server certificate SAN; the "
                        "client must pass the same value via --server-name "
                        "(IP literals are rejected)")
    p.add_argument("--days", type=int, default=825,
                   help="certificate validity in days (default 825)")
    args = p.parse_args(argv)
    try:
        provision(args.output_dir, args.server_name, args.days)
    except ProvisionError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
