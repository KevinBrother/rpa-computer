#!/usr/bin/env python3
"""Regression tests for the direct-remote PACKAGING sidecar:

    scripts/remote-credentials.py      (private CA + server cert + token)
    scripts/package-remote-client.sh   (source-free, credential-free client release)
    scripts/windows-remote-start.ps1 / windows-remote-stop.ps1  (static parse only)

These tests are hermetic: they use temp dirs (canonicalized, because the
tools rightly refuse symlink-routed output paths), a CLEARLY-TEST-ONLY fake
client binary (a shell script that just echoes), real openssl CLI calls for
credential verification, and only STATIC inspection of the PowerShell
scripts (they are never executed here — no remote Windows is touched).

Every subprocess has a bounded timeout and exit codes are asserted.
Python 3 stdlib only. Run:  python3 tests/remote_packaging.py
"""

import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CREDS = os.path.join(ROOT, "scripts", "remote-credentials.py")
PKG = os.path.join(ROOT, "scripts", "package-remote-client.sh")
PS_START = os.path.join(ROOT, "scripts", "windows-remote-start.ps1")
PS_STOP = os.path.join(ROOT, "scripts", "windows-remote-stop.ps1")
TIMEOUT = 120

# PowerShell single-quoted format string that wraps a value in double
# quotes: '"{0}"' (kept as a constant so the quoting regression tests
# spell it exactly once).
QUOTED_FMT = "'" + '"{0}"' + "'"


def run(argv, timeout=TIMEOUT, **kw):
    return subprocess.run(
        argv, capture_output=True, text=True, timeout=timeout, **kw
    )


def openssl(*args, timeout=TIMEOUT):
    proc = run(["openssl"] + list(args), timeout=timeout)
    assert proc.returncode == 0, f"openssl {' '.join(args[:2])} failed: {proc.stderr}"
    return proc.stdout


def mode_of(path):
    return stat.S_IMODE(os.lstat(path).st_mode)


def code_lines(src):
    """Source lines with trailing comments stripped (naive but sufficient
    for these scripts: no '#' inside string literals on code lines)."""
    for i, line in enumerate(src.splitlines(), 1):
        yield i, line.split("#", 1)[0]


class CredentialsTest(unittest.TestCase):
    def setUp(self):
        # realpath: the tool refuses symlink-routed output paths, and macOS
        # temp dirs live under the /var -> /private/var alias.
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="rpa-creds-test-"))
        self.out = os.path.join(self.tmp, "creds")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def provision(self, name="rpa-host.lan", extra=None, out=None):
        argv = [sys.executable, CREDS, "--output-dir", out or self.out,
                "--server-name", name]
        if extra:
            argv += extra
        return run(argv)

    def test_happy_path_layout_and_permissions(self):
        proc = self.provision()
        self.assertEqual(proc.returncode, 0, proc.stderr)
        expected = [
            "ca/ca.key", "ca/ca.pem",
            "host/server.pem", "host/server.key", "host/host.token",
            "client/ca.pem", "client/client.token",
        ]
        for rel in expected:
            self.assertTrue(
                os.path.isfile(os.path.join(self.out, rel)), f"missing {rel}"
            )
        self.assertEqual(mode_of(self.out), 0o700)
        for rel in ("ca/ca.key", "host/server.key", "host/host.token",
                    "client/client.token"):
            self.assertEqual(mode_of(os.path.join(self.out, rel)), 0o600,
                             f"{rel} must be 0600")
        # token: >= 32 bytes of entropy material, single line, never printed
        with open(os.path.join(self.out, "host/host.token"), "rb") as f:
            token = f.read().strip()
        self.assertGreaterEqual(len(token), 32)
        self.assertNotIn(token.decode(), proc.stdout)
        self.assertNotIn(token.decode(), proc.stderr)
        # host/client token copies match (shared secret)
        with open(os.path.join(self.out, "client/client.token"), "rb") as f:
            self.assertEqual(f.read().strip(), token)
        # stdout must not leak private key material either
        with open(os.path.join(self.out, "host/server.key")) as f:
            key_line = f.read().splitlines()[1]
        self.assertNotIn(key_line, proc.stdout + proc.stderr)

    def test_certificate_verifies_against_ca_and_has_dns_san(self):
        self.assertEqual(self.provision().returncode, 0)
        ca = os.path.join(self.out, "ca", "ca.pem")
        server = os.path.join(self.out, "host", "server.pem")
        verify = run(["openssl", "verify", "-CAfile", ca, server])
        self.assertEqual(verify.returncode, 0, verify.stderr)
        self.assertIn(": OK", verify.stdout)
        text = openssl("x509", "-in", server, "-noout", "-text")
        self.assertIn("DNS:rpa-host.lan", text)
        self.assertIn("TLS Web Server Authentication", text)
        self.assertRegex(text, r"CA:FALSE")
        ca_text = openssl("x509", "-in", ca, "-noout", "-text")
        self.assertRegex(ca_text, r"CA:TRUE")
        self.assertIn("Certificate Sign", ca_text)

    def test_leaf_does_not_verify_with_wrong_ca(self):
        self.assertEqual(self.provision().returncode, 0)
        other = os.path.join(self.tmp, "other")
        self.assertEqual(self.provision(out=other).returncode, 0)
        verify = run(["openssl", "verify",
                      "-CAfile", os.path.join(other, "ca", "ca.pem"),
                      os.path.join(self.out, "host", "server.pem")])
        self.assertNotEqual(verify.returncode, 0)

    def test_tls_handshake_wrong_server_name_fails(self):
        """Real openssl s_server/s_client: cert verifies for the right DNS
        name and FAILS hostname checks for a different name."""
        self.assertEqual(self.provision().returncode, 0)
        ca = os.path.join(self.out, "ca", "ca.pem")
        pem = os.path.join(self.out, "host", "server.pem")
        key = os.path.join(self.out, "host", "server.key")
        port = 18443
        server = subprocess.Popen(
            ["openssl", "s_server", "-accept", str(port),
             "-cert", pem, "-key", key, "-quiet", "-naccept", "2"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        try:
            import time
            for _ in range(50):
                time.sleep(0.1)
                probe = run(["openssl", "s_client", "-connect",
                             f"127.0.0.1:{port}", "-CAfile", ca,
                             "-verify_return_error",
                             "-verify_hostname", "rpa-host.lan"],
                            input="", timeout=10)
                if probe.returncode == 0:
                    break
            self.assertEqual(probe.returncode, 0,
                             "right server-name must verify: " + probe.stderr[-400:])
            bad = run(["openssl", "s_client", "-connect", f"127.0.0.1:{port}",
                       "-CAfile", ca, "-verify_return_error",
                       "-verify_hostname", "wrong-name.lan"],
                      input="", timeout=10)
            self.assertNotEqual(bad.returncode, 0,
                                "wrong server-name must be rejected")
        finally:
            server.kill()
            server.wait(timeout=10)

    def test_refuses_existing_output_dir(self):
        os.makedirs(self.out)
        proc = self.provision()
        self.assertEqual(proc.returncode, 2)
        self.assertIn("already exists", proc.stderr)

    def test_refuses_symlink_output_dir(self):
        target = os.path.join(self.tmp, "victim")
        os.makedirs(target)
        os.symlink(target, self.out)
        proc = self.provision()
        self.assertEqual(proc.returncode, 2)
        self.assertEqual(os.listdir(target), [], "victim dir must be untouched")

    def test_refuses_symlink_parent_component(self):
        """Docs claim no ancestor of --output-dir may be a symlink: with
        realdir/sneaky -> victim and output realdir/sneaky/creds, the script
        must refuse and the victim must stay empty (the old code only tested
        the leaf, which does not exist yet, and happily followed the parent
        link)."""
        realdir = os.path.join(self.tmp, "realdir")
        victim = os.path.join(self.tmp, "victim2")
        os.makedirs(realdir)
        os.makedirs(victim)
        os.symlink(victim, os.path.join(realdir, "sneaky"))
        proc = self.provision(out=os.path.join(realdir, "sneaky", "creds"))
        self.assertEqual(proc.returncode, 2)
        self.assertIn("symlink", proc.stderr)
        self.assertEqual(os.listdir(victim), [], "victim dir must be untouched")
        self.assertFalse(os.path.lexists(os.path.join(victim, "creds")))

    def test_failure_leaves_no_output_dir_and_no_secrets(self):
        # Make the PARENT read-only so the exclusive reservation mkdir
        # fails, and confirm nothing is left behind.
        readonly = os.path.join(self.tmp, "ro")
        os.makedirs(readonly)
        os.chmod(readonly, 0o500)
        try:
            proc = run([sys.executable, CREDS,
                        "--output-dir", os.path.join(readonly, "creds"),
                        "--server-name", "rpa-host.lan"])
            self.assertNotEqual(proc.returncode, 0)
            self.assertFalse(os.path.lexists(os.path.join(readonly, "creds")))
        finally:
            os.chmod(readonly, 0o700)

    def test_rejects_invalid_server_names(self):
        for bad in ("", "a b.lan", "host/name", "*.lan", "-evil.lan",
                    "evil.lan.", "evil..lan", "ho st", "a;b.lan", "a\nb.lan",
                    "x" * 254, "under_score.lan"):
            proc = self.provision(name=bad)
            self.assertEqual(proc.returncode, 2, f"name {bad!r} must be rejected")
            self.assertFalse(os.path.lexists(self.out),
                             f"nothing may be created for name {bad!r}")

    def test_rejects_ip_literal_server_name(self):
        """This tool issues DNS-SAN certificates only; an IP literal must be
        rejected, not silently baked into a DNS SAN the client then expects
        to be a hostname."""
        for ip in ("192.168.1.50", "10.0.0.1", "::1", "2001:db8::1"):
            proc = self.provision(name=ip)
            self.assertEqual(proc.returncode, 2, f"IP literal {ip!r} must be rejected")
            self.assertFalse(os.path.lexists(self.out))


class PackageRemoteClientTest(unittest.TestCase):
    FAKE = "#!/bin/sh\n# TEST-ONLY fake computer-client (not a real binary)\necho fake-client\n"

    def setUp(self):
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix="rpa-pkg-test-"))
        self.release = os.path.join(self.tmp, "release")
        self.binary = os.path.join(self.tmp, "fake-client")
        self.ca = os.path.join(self.tmp, "ca.pem")
        self.token = os.path.join(self.tmp, "client.token")
        with open(self.binary, "w") as f:
            f.write(self.FAKE)
        os.chmod(self.binary, 0o755)
        with open(self.ca, "w") as f:
            f.write("TEST-ONLY not a real certificate\n")
        with open(self.token, "w") as f:
            f.write("TEST-ONLY not a real token\n")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def package(self, **over):
        argv = ["bash", PKG,
                "--binary", over.get("binary", self.binary),
                "--release-dir", over.get("release_dir", self.release),
                "--connect", over.get("connect", "192.168.1.50:8399"),
                "--server-name", over.get("server_name", "rpa-host.lan"),
                "--ca-cert", over.get("ca_cert", self.ca),
                "--token-file", over.get("token_file", self.token)]
        return run(argv, cwd=over.get("cwd"))

    def test_happy_path_contents_and_manifest(self):
        proc = self.package()
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertTrue(os.path.isfile(os.path.join(self.release, "bin", "computer-client")))
        cfg_path = os.path.join(self.release, "mcp", "mcp.json")
        with open(cfg_path) as f:
            cfg = json.load(f)
        server = cfg["mcpServers"]["computer"]
        self.assertEqual(server["command"],
                         os.path.join(self.release, "bin", "computer-client"))
        args = server["args"]
        self.assertEqual(args, [
            "--connect", "192.168.1.50:8399",
            "--ca-cert", self.ca,
            "--server-name", "rpa-host.lan",
            "--token-file", self.token,
        ])
        # Acceptance prompts staged for run-blackbox-claude.sh compatibility.
        tasks = os.listdir(os.path.join(self.release, "tasks"))
        self.assertIn("01-image-recognition.md", tasks)
        # Manifest covers every shipped file except itself, and verifies.
        manifest = os.path.join(self.release, "MANIFEST.sha256")
        self.assertTrue(os.path.isfile(manifest))
        check = run(["shasum", "-a", "256", "-c", "MANIFEST.sha256"],
                    cwd=self.release)
        self.assertEqual(check.returncode, 0, check.stdout + check.stderr)
        # No source, no credential material, no symlinks inside the release.
        for dirpath, _, files in os.walk(self.release):
            for name in files:
                p = os.path.join(dirpath, name)
                self.assertFalse(os.path.islink(p))
                self.assertFalse(name.endswith((".rs", ".token", ".key", ".pem")),
                                 f"credential/source material leaked: {p}")
                self.assertNotIn(name, ("Cargo.toml", "Cargo.lock"))
        # Metadata carries hashes only, never secret content.
        with open(os.path.join(self.release, "RELEASE.json")) as f:
            meta = json.load(f)
        self.assertEqual(meta["kind"], "computer-remote-client")
        self.assertNotIn("TEST-ONLY not a real token",
                         json.dumps(meta))
        with open(cfg_path) as f:
            self.assertNotIn("TEST-ONLY not a real token", f.read())

    def test_release_json_is_json_escaped(self):
        """Paths containing a double quote or backslash must still produce
        VALID RELEASE.json metadata (the old shell heredoc broke the JSON)."""
        weird = os.path.join(self.tmp, 'wei"rd\\dir')
        os.makedirs(weird)
        ca = os.path.join(weird, "ca.pem")
        with open(ca, "w") as f:
            f.write("TEST-ONLY not a real certificate\n")
        proc = self.package(ca_cert=ca)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        with open(os.path.join(self.release, "RELEASE.json")) as f:
            meta = json.load(f)  # must parse
        self.assertEqual(meta["ca_cert_path"], os.path.realpath(ca))
        self.assertEqual(meta["connect"], "192.168.1.50:8399")

    def test_relative_output_command_is_absolute(self):
        """A relative --release-dir staged from another cwd must produce an
        ABSOLUTE command path in mcp.json (the old code left it relative, so
        the MCP client could not be launched from any other directory)."""
        work = os.path.join(self.tmp, "work")
        os.makedirs(work)
        proc = self.package(release_dir="rel-release", cwd=work)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        cfg_path = os.path.join(work, "rel-release", "mcp", "mcp.json")
        with open(cfg_path) as f:
            cfg = json.load(f)
        command = cfg["mcpServers"]["computer"]["command"]
        self.assertTrue(os.path.isabs(command),
                        f"command must be absolute, got {command!r}")
        self.assertTrue(os.path.isfile(command))
        args = cfg["mcpServers"]["computer"]["args"]
        for flag in ("--ca-cert", "--token-file"):
            v = args[args.index(flag) + 1]
            self.assertTrue(os.path.isabs(v), f"{flag} must be absolute: {v!r}")

    def test_refuses_source_overlap(self):
        """--release-dir must not be the repo root, inside it, or an ancestor
        of it (the black-box sandbox launcher refuses/denies such trees)."""
        owned = os.path.join(ROOT, "evil-release")
        try:
            for rel in (ROOT, owned, os.path.dirname(ROOT)):
                proc = self.package(release_dir=rel)
                self.assertEqual(
                    proc.returncode, 2,
                    f"release dir {rel!r} overlapping the source tree must be refused")
        finally:
            # Clean up ONLY this exact test-owned directory, and only while
            # it still carries OUR test release metadata — never arbitrary
            # user paths. (A RED run leaves it behind; a stale leftover must
            # not poison later runs.)
            meta = None
            try:
                with open(os.path.join(owned, "RELEASE.json")) as f:
                    meta = json.load(f)
            except (OSError, ValueError):
                meta = None
            if isinstance(meta, dict) and meta.get("kind") == "computer-remote-client":
                shutil.rmtree(owned, ignore_errors=True)

    def test_refuses_symlink_ancestor_of_release_dir(self):
        """An ancestor symlink of --release-dir must be refused (the old code
        only rejected a symlink AT the final path, which does not exist yet
        by construction)."""
        victim = os.path.join(self.tmp, "victim")
        os.makedirs(victim)
        os.symlink(victim, os.path.join(self.tmp, "sneak"))
        proc = self.package(release_dir=os.path.join(self.tmp, "sneak", "release"))
        self.assertEqual(proc.returncode, 2)
        self.assertEqual(os.listdir(victim), [], "victim dir must be untouched")

    def test_refuses_existing_release_dir(self):
        os.makedirs(self.release)
        proc = self.package()
        self.assertEqual(proc.returncode, 2)
        self.assertIn("already exists", proc.stderr)

    def test_rejects_bad_connect(self):
        for bad in ("no-port", "host:0", "host:99999", "host:abc",
                    "tcp://host:1", "ho st:8399", "a/b:8399",
                    "fe80::1:8399"):  # IPv6 literal without brackets
            proc = self.package(connect=bad)
            self.assertEqual(proc.returncode, 2, f"connect {bad!r} must be rejected")
            self.assertFalse(os.path.lexists(self.release))

    def test_accepts_bracketed_ipv6_connect(self):
        proc = self.package(connect="[fe80::1]:8399")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        with open(os.path.join(self.release, "mcp", "mcp.json")) as f:
            cfg = json.load(f)
        args = cfg["mcpServers"]["computer"]["args"]
        self.assertIn("[fe80::1]:8399", args)

    def test_rejects_bad_server_name(self):
        for bad in ("bad name", "a/b", "a;b.lan", "*.lan", ".lan", ""):
            proc = self.package(server_name=bad)
            self.assertEqual(proc.returncode, 2, f"name {bad!r} must be rejected")

    def test_rejects_symlink_binary(self):
        link = os.path.join(self.tmp, "link-client")
        os.symlink(self.binary, link)
        proc = self.package(binary=link)
        self.assertEqual(proc.returncode, 2)

    def test_requires_all_args(self):
        proc = run(["bash", PKG, "--binary", self.binary])
        self.assertEqual(proc.returncode, 2)


class PowerShellStaticTest(unittest.TestCase):
    """Static inspection ONLY — the scripts are never executed here, and no
    remote Windows machine is touched by this test run. These checks guard
    known-bad PowerShell binding traps; they do NOT prove runtime behavior
    on Windows (a coordinator-run live deployment is still required)."""

    def read(self, path):
        with open(path, encoding="utf-8") as f:
            return f.read()

    def test_start_script_shape(self):
        src = self.read(PS_START)
        for needle in (
            "WTSGetActiveConsoleSessionId",
            "-LogonType Interactive",
            "New-ScheduledTaskAction -Execute $exe -Argument $argList",
            "--remote-listen",
            "--tls-cert",
            "--tls-key",
            "--token-file",
            "--log-file",
            "Set-Acl",
            "S-1-5-18",
            "S-1-5-32-544",
            "remote-host-instance.json",
            "Get-NetTCPConnection",
        ):
            self.assertIn(needle, src)
        for banned in ("Invoke-WmiMethod", "Win32_Process Create",
                       "New-NetFirewallRule", "Set-NetFirewallRule",
                       "Enable-NetFirewallRule"):
            self.assertNotIn(banned, src)
        # Start-Process must never appear as an actual command (the word in
        # header comments explaining what we avoid is fine).
        for _, code in code_lines(src):
            self.assertNotIn("Start-Process", code)
        # 0.0.0.0 must be explicitly refused.
        self.assertIn("0.0.0.0", src)

    def test_stop_script_shape(self):
        src = self.read(PS_STOP)
        for needle in (
            "remote-host-instance.json",
            "CreationDate",
            "ExecutablePath",
            "Stop-ScheduledTask",
            "Unregister-ScheduledTask",
            "Stop-Process -Id",
            "action_arguments",
            "principal_user",
        ):
            self.assertIn(needle, src)
        for banned in ("Invoke-WmiMethod", "New-NetFirewallRule",
                       "Set-NetFirewallRule", "taskkill"):
            self.assertNotIn(banned, src)

    def test_formatted_strings_are_parenthesized(self):
        """Regression: `Write-Output ("...{0}...") -f args` applied -f to the
        COMMAND, not the string (PS binds -f to Write-Output's -Format param
        and silently stringifies the args array). The whole formatted string
        must be parenthesized before -f, e.g. Write-Output ((...) -f a, b)."""
        for path in (PS_START, PS_STOP):
            for i, code in code_lines(self.read(path)):
                m = re.search(r'Write-(?:Output|Error)\s*\(', code)
                if m:
                    self.assertEqual(code[m.end():m.end() + 1], "(",
                                     f"{path}:{i} must parenthesize the entire "
                                     "formatted string: Write-X ((...) -f args)")

    def test_start_uses_ipaddress_parsing_and_guid_taskname(self):
        src = self.read(PS_START)
        # Real IP parsing instead of a character regex.
        self.assertIn("[System.Net.IPAddress]::Parse", src)
        self.assertNotIn("'^[0-9a-fA-F.:]+$'", src)
        # Omitted -TaskName must be GUID-unique, never shared per-port.
        self.assertIn("[Guid]::NewGuid()", src)
        self.assertNotIn("RpaComputerRemoteHost-$Port", src)
        # IPv6 listen addresses must be bracketed for Rust SocketAddr.
        self.assertIn("InterNetworkV6", src)

    def test_start_writes_provisional_record_before_start_and_unhealthy_first(self):
        src = self.read(PS_START)
        codes = [code for _, code in code_lines(src)]
        joined = "\n".join(codes)
        # A provisional instance record must be written BEFORE
        # Start-ScheduledTask so a failed start is still owned/stoppable.
        prov_idx = joined.index("provisional_record")
        self.assertIn("'provisional'", src)
        self.assertLess(prov_idx, joined.index("Start-ScheduledTask"),
                        "the provisional record write must precede Start-ScheduledTask")
        # On failure (no process / no TCP LISTEN) the record must be marked
        # unhealthy BEFORE the failure exit — the old code Write-Error'd
        # with $ErrorActionPreference=Stop BEFORE ever writing the record,
        # so a failed start left NOTHING for stop to clean up.
        for marker in ("no_tcp_listen", "process_not_found"):
            self.assertIn(marker, src)
        # Semantic ordering on ACTUAL code (comments stripped): set status
        # 'unhealthy' -> rewrite the record -> failure exit. The old check
        # only located the word "mark_unhealthy" ANYWHERE in the source; it
        # lived solely in a comment, so next() hit StopIteration (and even
        # with a comment match the ordering assertion would be vacuous).
        status_idx = next(i for i, c in enumerate(codes)
                          if re.search(r"\.status\s*=\s*'unhealthy'", c))
        write_idx = next(i for i, c in enumerate(codes)
                         if i > status_idx
                         and "Set-Content -LiteralPath $record" in c)
        exit_idx = next(i for i, c in enumerate(codes)
                        if i > write_idx and re.search(r"\bexit 1\b", c))
        self.assertLess(status_idx, write_idx,
                        "status 'unhealthy' must be set before the record write")
        self.assertLess(write_idx, exit_idx,
                        "the unhealthy record write must precede the failure exit")
        # The success message must appear only after listen verification.
        self.assertLess(src.index("Get-NetTCPConnection"),
                        src.index("remote host listening"))
        # No unrelated kill: the start script must never Stop-Process.
        for _, code in code_lines(src):
            self.assertNotIn("Stop-Process", code)

    def test_start_verify_predicate_is_statement_try_and_fails_closed(self):
        """Regression: the post-start process predicate used a PARENTHESIZED
        `(try { ... } catch { $true })` expression. PowerShell parses `(try`
        as an invocation of a command named `try` — ParseFile accepts it,
        but at runtime the whole Where-Object errors with "The term 'try'
        is not recognized", so EVERY start failed verification. try/catch
        must be a STATEMENT inside the scriptblock, and its catch must FAIL
        CLOSED ($false): a `$true` fallback would accept any computer-host
        process regardless of creation time (PID-reuse mis-attribution)."""
        src = self.read(PS_START)
        codes = [code for _, code in code_lines(src)]
        joined = "\n".join(codes)
        for i, code in code_lines(src):
            self.assertNotIn("(try", code,
                             f"{PS_START}:{i} must not use a parenthesized "
                             "(try ...) expression; use a statement try/catch")
            self.assertNotRegex(code, r"catch\s*\{\s*\$true\s*\}",
                                f"{PS_START}:{i} catch must fail closed, "
                                "never fall back to $true")
        # The ACTUAL predicate block (comments stripped) must carry the
        # strict identity checks: exact exe, exact session, creation time
        # at/after the start mark, and a fail-closed catch.
        block_idx = next(i for i, c in enumerate(codes)
                         if "Name='computer-host.exe'" in c)
        block = "\n".join(codes[block_idx:block_idx + 16])
        self.assertIn("$_.ExecutablePath -eq $exe", block)
        self.assertIn("$_.SessionId -eq [int]$consoleSession", block)
        self.assertRegex(block, r"CreationDate\.ToUniversalTime\(\)\s*-ge\s*"
                                r"\$startMark\.AddSeconds\(-2\)")
        self.assertRegex(block, r"catch\s*\{\s*\$false\s*\}")

    def test_start_rejects_quotes_and_reparse_points(self):
        src = self.read(PS_START)
        self.assertIn("double-quote or control characters", src)
        self.assertIn("ReparsePoint", src)

    def test_start_quotes_path_arguments(self):
        """-RemoteDir permits spaces (only quotes/control chars and listen/
        task-name whitespace are rejected), so every PATH value in the
        scheduled-task argument string must be double-quoted — the old raw
        join split C:\\rpa remote\\host.token into two argv words. The
        quoted construction must be ACTUAL CODE (not a comment mention),
        and the recorded action_arguments must remain exactly $argList."""
        codes = [code for _, code in code_lines(self.read(PS_START))]
        joined = "\n".join(codes)
        for var in ("$serverPem", "$serverKey", "$tokenFile", "$logFile"):
            needle = "(" + QUOTED_FMT + " -f " + var + ")"
            self.assertIn(needle, joined,
                          f"path argument {var} must be double-quoted in the "
                          "task argument string")
        # Every value handed to the exe must still be reachable in $argList
        # and the single recorded copy must be exactly the action string.
        self.assertIn("$argList = @(", joined)
        arg_refs = [c for c in codes if "action_arguments" in c]
        self.assertEqual(len(arg_refs), 1,
                         "action_arguments must be recorded exactly once")
        self.assertIn("action_arguments     = $argList", arg_refs[0])

    def test_start_refuses_any_existing_record_fail_closed(self):
        """Regression: an omitted -TaskName becomes a NEW GUID before the
        existing-task check, so a prior deployment could NEVER be found via
        the task — and the old code DELETED a terminal-looking record
        (pid 0 / provisional / unhealthy), orphaning the prior task and
        losing ownership. Fail-closed policy: if remote-host-instance.json
        exists at all, refuse with an actionable stop instruction; start
        NEVER removes or rewrites a pre-existing record. Checks run on
        comment-stripped code so a comment mention cannot satisfy them."""
        codes = [code for _, code in code_lines(self.read(PS_START))]
        joined = "\n".join(codes)
        # The existence check must be a real Test-Path on the record file.
        self.assertIn("Test-Path -LiteralPath $record -PathType Leaf", joined)
        # Actionable instruction: point the operator at windows-remote-stop.
        self.assertIn("windows-remote-stop.ps1", joined)
        # Start must NEVER remove the record file or overwrite the
        # pre-existing record: no Remove-Item on the record anywhere, and
        # the only record writes are the provisional/healthy/unhealthy
        # lifecycle AFTER the refusal gate (i.e. none before it).
        for c in codes:
            self.assertNotRegex(c, r"Remove-Item.*\$record",
                                "start must never remove the instance record")
        refuse_idx = next(i for i, c in enumerate(codes)
                          if "Test-Path -LiteralPath $record -PathType Leaf" in c)
        write_idx = next(i for i, c in enumerate(codes)
                         if "Set-Content -LiteralPath $record" in c)
        self.assertLess(refuse_idx, write_idx,
                        "the refuse-if-record-exists gate must precede any "
                        "record write")
        # The record must exist as a REFUSAL (exit 1) before Register-
        # ScheduledTask, not merely as a post-facto check.
        register_idx = next(i for i, c in enumerate(codes)
                            if "Register-ScheduledTask" in c)
        self.assertLess(refuse_idx, register_idx,
                        "the record-existence refusal must precede task "
                        "registration")

    def test_stop_requires_full_identity_and_exact_creation(self):
        src = self.read(PS_STOP)
        # ALL identity fields verified before any mutation.
        for field in ("task_name", "pid", "exe", "creation_date_utc",
                      "session_id", "listen_address", "action_execute",
                      "action_arguments", "principal_user", "principal_sid",
                      "principal_run_level"):
            self.assertIn(f"'{field}'", src)
        # Creation-date comparison must be EXACT (UTC ticks), never a
        # fuzzy <2s window that invites PID-reuse kills.
        self.assertIn(".Ticks", src)
        for _, code in code_lines(src):
            self.assertNotIn("TotalSeconds", code)
        # Task RunLevel and principal SID must be checked too.
        self.assertIn("RunLevel", src)
        self.assertIn("principal_sid", src)
        # Revalidation before the force kill after the 15s wait.
        self.assertIn("REVALIDATE", src)

    def test_ps_scripts_contain_no_control_characters(self):
        """A literal NUL/control byte (e.g. a Python-side edit that emitted
        the raw character from a \\x00 escape) makes the file 'binary' and
        breaks PowerShell parsing; only tab/CR/LF are allowed."""
        for path in (PS_START, PS_STOP):
            with open(path, "rb") as f:
                data = f.read()
            bad = sorted({b for b in data
                          if b < 0x20 and b not in (0x09, 0x0A, 0x0D)})
            self.assertEqual(bad, [],
                             f"{path} contains control bytes: "
                             f"{[hex(b) for b in bad]}")

    def test_powershell_parser_if_available(self):
        pwsh = shutil.which("pwsh") or shutil.which("powershell")
        if not pwsh:
            self.skipTest("no PowerShell available for parse check")
        for path in (PS_START, PS_STOP):
            proc = run([
                pwsh, "-NoProfile", "-Command",
                "$e=$null;[System.Management.Automation.Language.Parser]"
                f"::ParseFile('{path}',[ref]$null,[ref]$e)|Out-Null;"
                "if($e.Count){$e|ForEach-Object{$_.Message};exit 1}else{exit 0}"
            ])
            self.assertEqual(proc.returncode, 0,
                             f"{path} must parse: {proc.stdout}{proc.stderr}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
