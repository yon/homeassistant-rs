#!/usr/bin/env python3
"""Quality score calculator for homeassistant-rs.

Evaluates the project against quality gates (0-100 scale).

Thresholds:
  80+ = Commit-ready (tests pass, no lint errors, no security issues)
  90+ = PR-ready (high coverage, clean architecture, documented)
  95+ = Release-ready (production quality, performance validated)

Usage:
  python3 scripts/quality_score.py --summary
  python3 scripts/quality_score.py --verbose
  python3 scripts/quality_score.py --json
"""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path


class QualityScore:
    def __init__(self):
        self.score = 100
        self.deductions = []
        self.bonuses = []
        self.checks = {}
        self.auto_fail = False

    def bonus(self, points, reason):
        self.score = min(100, self.score + points)
        self.bonuses.append({"points": points, "reason": reason})

    def deduct(self, points, category, reason, severity="major"):
        self.score = max(0, self.score - points)
        self.deductions.append({
            "points": points,
            "category": category,
            "reason": reason,
            "severity": severity,
        })

    def fail(self, category, reason):
        self.auto_fail = True
        self.score = 0
        self.deductions.append({
            "points": 100,
            "category": category,
            "reason": reason,
            "severity": "critical",
        })


def run_cmd(cmd, timeout=120):
    """Run a command and return (exit_code, stdout, stderr)."""
    try:
        result = subprocess.run(
            cmd,
            shell=True,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return result.returncode, result.stdout, result.stderr
    except subprocess.TimeoutExpired:
        return -1, "", "Command timed out"
    except Exception as e:
        return -1, "", str(e)


def check_build(qs):
    """Check if the project builds."""
    code, stdout, stderr = run_cmd("make build")
    if code != 0:
        qs.fail("build", "Build failed")
        qs.checks["build"] = {"status": "FAIL", "output": stderr}
        return False
    qs.checks["build"] = {"status": "PASS"}
    return True


def check_tests(qs):
    """Check if tests pass."""
    code, stdout, stderr = run_cmd("make test-rust", timeout=300)
    output = stdout + stderr
    if code != 0:
        # Count failures
        fail_match = re.search(r"(\d+) failed", output)
        fail_count = int(fail_match.group(1)) if fail_match else 1
        qs.fail("tests", f"Tests failed ({fail_count} failure(s))")
        qs.checks["tests"] = {"status": "FAIL", "failures": fail_count, "output": output[-500:]}
        return False

    # Count test results
    pass_match = re.search(r"(\d+) passed", output)
    pass_count = int(pass_match.group(1)) if pass_match else 0
    qs.checks["tests"] = {"status": "PASS", "passed": pass_count}
    return True


def check_lint(qs):
    """Check linting."""
    code, stdout, stderr = run_cmd("make lint")
    output = stdout + stderr
    if code != 0:
        # Count errors vs warnings
        error_count = output.lower().count("error")
        warning_count = output.lower().count("warning")
        qs.deduct(5 * max(1, error_count), "lint", f"Lint errors: {error_count}", "critical")
        qs.deduct(1 * warning_count, "lint", f"Lint warnings: {warning_count}", "minor")
        qs.checks["lint"] = {"status": "FAIL", "errors": error_count, "warnings": warning_count}
        return False
    qs.checks["lint"] = {"status": "PASS"}
    return True


def check_alphabetization(qs):
    """Check alphabetization linting."""
    code, stdout, stderr = run_cmd("./scripts/lint-alpha.py --all")
    output = stdout + stderr
    if code != 0:
        error_match = re.search(r"(\d+) error", output)
        error_count = int(error_match.group(1)) if error_match else 1
        qs.deduct(2 * error_count, "alphabetization", f"Alphabetization errors: {error_count}", "major")
        qs.checks["alphabetization"] = {"status": "FAIL", "errors": error_count}
        return False
    qs.checks["alphabetization"] = {"status": "PASS"}
    return True


def check_security(qs):
    """Check for security issues."""
    code, stdout, stderr = run_cmd("cargo audit 2>/dev/null || true")
    output = stdout + stderr
    if "vulnerability" in output.lower() or "Vulnerability" in output:
        vuln_match = re.search(r"(\d+) vulnerabilit", output)
        vuln_count = int(vuln_match.group(1)) if vuln_match else 1
        qs.deduct(10 * vuln_count, "security", f"Dependency vulnerabilities: {vuln_count}", "critical")
        qs.checks["security"] = {"status": "WARN", "vulnerabilities": vuln_count}
        return False
    qs.checks["security"] = {"status": "PASS"}
    return True


def check_source_heuristics(qs):
    """Scan source code for quality heuristics."""
    src_dirs = [Path("crates")]
    extensions = {".rs", ".py"}
    issues = []

    for src_dir in src_dirs:
        if not src_dir.exists():
            continue
        for path in src_dir.rglob("*"):
            if path.suffix not in extensions or "target" in str(path) or "vendor" in str(path):
                continue
            try:
                content = path.read_text()
                lines = content.split("\n")
            except Exception:
                continue

            # Check for long functions (Rust)
            if path.suffix == ".rs":
                fn_start = None
                fn_name = ""
                brace_depth = 0
                for i, line in enumerate(lines):
                    fn_match = re.match(r"\s*(pub\s+)?(async\s+)?fn\s+(\w+)", line)
                    if fn_match and "{" in line:
                        fn_start = i
                        fn_name = fn_match.group(3)
                        brace_depth = line.count("{") - line.count("}")
                    elif fn_start is not None:
                        brace_depth += line.count("{") - line.count("}")
                        if brace_depth <= 0:
                            fn_len = i - fn_start
                            if fn_len > 50:
                                issues.append(f"{path}:{fn_start+1}: function '{fn_name}' is {fn_len} lines")
                            fn_start = None

            # Check for hardcoded secrets
            for i, line in enumerate(lines):
                secret_patterns = [
                    r'(?i)(api[_-]?key|secret|password|token)\s*=\s*["\'][^"\']{8,}',
                    r'(?i)AKIA[0-9A-Z]{16}',  # AWS access key
                ]
                for pattern in secret_patterns:
                    if re.search(pattern, line) and "test" not in str(path).lower():
                        qs.fail("security", f"Possible hardcoded secret at {path}:{i+1}")
                        return

            # Check for TODO/FIXME without ticket
            for i, line in enumerate(lines):
                if re.search(r'(?i)(TODO|FIXME|HACK)\b', line):
                    if not re.search(r'(#\d+|plan:T-\d+)', line):  # No ticket/plan reference
                        issues.append(f"{path}:{i+1}: TODO/FIXME without ticket reference")

    # Deduct for long functions
    long_fns = [i for i in issues if "function" in i and "lines" in i]
    if long_fns:
        qs.deduct(min(10, 3 * len(long_fns)), "complexity", f"God functions: {len(long_fns)}", "major")

    # Deduct for TODOs without tickets
    todos = [i for i in issues if "TODO" in i or "FIXME" in i]
    if todos:
        qs.deduct(min(5, len(todos)), "maintainability", f"TODOs without ticket: {len(todos)}", "minor")

    qs.checks["heuristics"] = {"status": "DONE", "issues": len(issues)}


def main():
    parser = argparse.ArgumentParser(description="Quality score calculator")
    parser.add_argument("--summary", action="store_true", help="One-line summary")
    parser.add_argument("--verbose", action="store_true", help="Detailed breakdown")
    parser.add_argument("--json", action="store_true", help="JSON output")
    parser.add_argument("--skip-build", action="store_true", help="Skip build check")
    parser.add_argument("--skip-tests", action="store_true", help="Skip test check")
    args = parser.parse_args()

    qs = QualityScore()

    # Run checks in order (stop on critical failure)
    if not args.skip_build:
        if not check_build(qs):
            if qs.auto_fail:
                _output(qs, args)
                sys.exit(2)

    if not args.skip_tests:
        if not check_tests(qs):
            if qs.auto_fail:
                _output(qs, args)
                sys.exit(2)

    check_lint(qs)
    check_alphabetization(qs)
    check_source_heuristics(qs)

    # Security is non-blocking
    check_security(qs)

    _output(qs, args)

    if qs.score < 80:
        sys.exit(1)
    sys.exit(0)


def _output(qs, args):
    """Output results in requested format."""
    gate = "RELEASE" if qs.score >= 95 else "PR" if qs.score >= 90 else "COMMIT" if qs.score >= 80 else "BLOCKED"

    if args.json:
        print(json.dumps({
            "score": qs.score,
            "gate": gate,
            "auto_fail": qs.auto_fail,
            "checks": qs.checks,
            "deductions": qs.deductions,
            "bonuses": qs.bonuses,
        }, indent=2))
    elif args.verbose:
        print(f"\n{'='*60}")
        print(f"Quality Score: {qs.score}/100 ({gate})")
        print(f"{'='*60}\n")

        print("Checks:")
        for name, result in qs.checks.items():
            status = result.get("status", "?")
            print(f"  {name}: {status}")

        if qs.deductions:
            print("\nDeductions:")
            for d in sorted(qs.deductions, key=lambda x: x["points"], reverse=True):
                print(f"  -{d['points']:>3}  [{d['severity'].upper()}] {d['category']}: {d['reason']}")

        if qs.bonuses:
            print("\nBonuses:")
            for b in qs.bonuses:
                print(f"  +{b['points']:>3}  {b['reason']}")

        print(f"\nGate: {gate}")
        if qs.score < 80:
            print("STATUS: BLOCKED — fix critical/major issues before committing")
        elif qs.score < 90:
            print("STATUS: Commit OK — address issues before PR")
        elif qs.score < 95:
            print("STATUS: PR OK — polish for release")
        else:
            print("STATUS: Release ready")
    else:
        # Summary (default)
        print(f"Quality: {qs.score}/100 ({gate})")


if __name__ == "__main__":
    main()
