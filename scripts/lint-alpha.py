#!/usr/bin/env python3
"""Alphabetization linter for Rust and Python files.

Checks:
- Rust: match arms, mod declarations, use statements
- Python: class properties and methods (grouped by type)
- TOML: Cargo.toml dependencies

Usage:
    ./scripts/lint-alpha.py [files...]
    ./scripts/lint-alpha.py --all
    ./scripts/lint-alpha.py --staged
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path


class LintError:
    def __init__(self, file: str, line: int, message: str):
        self.file = file
        self.line = line
        self.message = message

    def __str__(self):
        return f"{self.file}:{self.line}: {self.message}"


def get_staged_files() -> list[str]:
    """Get list of staged files from git."""
    result = subprocess.run(
        ["git", "diff", "--cached", "--name-only", "--diff-filter=ACM"],
        capture_output=True,
        text=True,
    )
    return [f for f in result.stdout.strip().split("\n") if f]


def get_all_files() -> list[str]:
    """Get all tracked files from git."""
    result = subprocess.run(
        ["git", "ls-files"],
        capture_output=True,
        text=True,
    )
    return [f for f in result.stdout.strip().split("\n") if f]


def check_rust_match_arms(content: str, filepath: str) -> list[LintError]:
    """Check that match arms are alphabetized."""
    errors = []
    lines = content.split("\n")

    # Find match blocks
    i = 0
    while i < len(lines):
        line = lines[i]
        # Look for match statements
        if re.search(r"\bmatch\b.*\{", line):
            # Collect match arms
            arms = []
            brace_depth = line.count("{") - line.count("}")
            i += 1

            while i < len(lines) and brace_depth > 0:
                arm_line = lines[i]
                brace_depth += arm_line.count("{") - arm_line.count("}")

                # Match arm pattern: starts with variant/pattern =>
                arm_match = re.match(r"\s*([A-Za-z_][A-Za-z0-9_:]*(?:::[A-Za-z_][A-Za-z0-9_]*)*)", arm_line)
                if arm_match and "=>" in arm_line:
                    # Skip wildcard patterns
                    arm_name = arm_match.group(1)
                    if arm_name != "_":
                        arms.append((arm_name, i + 1))  # 1-indexed line number

                i += 1

            # Check if arms are sorted (case-insensitive)
            if len(arms) > 1:
                arm_names = [a[0] for a in arms]
                sorted_names = sorted(arm_names, key=str.lower)
                if arm_names != sorted_names:
                    # Find first out-of-order arm
                    for j, (name, line_num) in enumerate(arms):
                        if name != sorted_names[j]:
                            errors.append(LintError(
                                filepath,
                                line_num,
                                f"Match arm '{name}' is not alphabetized (expected '{sorted_names[j]}')"
                            ))
                            break
        else:
            i += 1

    return errors


def check_rust_mod_statements(content: str, filepath: str) -> list[LintError]:
    """Check that mod declarations are alphabetized."""
    errors = []
    lines = content.split("\n")

    # Collect consecutive mod statements
    mod_groups = []
    current_group = []

    for i, line in enumerate(lines):
        mod_match = re.match(r"\s*(pub\s+)?mod\s+([a-z_][a-z0-9_]*)\s*;", line)
        if mod_match:
            current_group.append((mod_match.group(2), i + 1))
        else:
            if current_group:
                mod_groups.append(current_group)
                current_group = []

    if current_group:
        mod_groups.append(current_group)

    # Check each group is sorted
    for group in mod_groups:
        if len(group) > 1:
            names = [m[0] for m in group]
            sorted_names = sorted(names)
            if names != sorted_names:
                for j, (name, line_num) in enumerate(group):
                    if name != sorted_names[j]:
                        errors.append(LintError(
                            filepath,
                            line_num,
                            f"mod '{name}' is not alphabetized (expected '{sorted_names[j]}')"
                        ))
                        break

    return errors


def check_rust_use_statements(content: str, filepath: str) -> list[LintError]:
    """Check that use statements are alphabetized within groups."""
    errors = []
    lines = content.split("\n")

    # Collect consecutive use statements
    use_groups = []
    current_group = []

    for i, line in enumerate(lines):
        use_match = re.match(r"\s*(pub\s+)?use\s+([a-zA-Z_][a-zA-Z0-9_:{}*,\s]*)\s*;", line)
        if use_match:
            # Get the primary crate/module name for sorting
            use_path = use_match.group(2).strip()
            current_group.append((use_path, i + 1, line.strip()))
        else:
            if current_group:
                use_groups.append(current_group)
                current_group = []

    if current_group:
        use_groups.append(current_group)

    # Check each group is sorted
    for group in use_groups:
        if len(group) > 1:
            # Sort by the full use path
            paths = [u[0] for u in group]
            sorted_paths = sorted(paths)
            if paths != sorted_paths:
                for j, (path, line_num, _) in enumerate(group):
                    if path != sorted_paths[j]:
                        errors.append(LintError(
                            filepath,
                            line_num,
                            f"use statement not alphabetized"
                        ))
                        break

    return errors


def check_python_class_members(content: str, filepath: str) -> list[LintError]:
    """Check Python class properties and methods are alphabetized by group."""
    errors = []
    lines = content.split("\n")

    # Find class definitions
    i = 0
    while i < len(lines):
        line = lines[i]
        class_match = re.match(r"^class\s+(\w+)", line)
        if class_match:
            class_name = class_match.group(1)
            class_indent = len(line) - len(line.lstrip())
            i += 1

            # Collect members by type
            properties = []  # @property methods
            methods = []     # regular methods
            dunders = []     # __xxx__ methods

            while i < len(lines):
                member_line = lines[i]

                # Check if we've left the class
                if member_line.strip() and not member_line.startswith(" " * (class_indent + 1)):
                    if not member_line.startswith(" "):
                        break

                # Check for @property decorator
                if re.match(r"\s+@property\s*$", member_line):
                    i += 1
                    if i < len(lines):
                        def_match = re.match(r"\s+def\s+(\w+)\s*\(", lines[i])
                        if def_match:
                            prop_name = def_match.group(1)
                            if not prop_name.startswith("_"):
                                properties.append((prop_name, i + 1))
                    i += 1
                    continue

                # Check for method definition
                def_match = re.match(r"\s+def\s+(\w+)\s*\(", member_line)
                if def_match:
                    method_name = def_match.group(1)
                    if method_name.startswith("__") and method_name.endswith("__"):
                        if method_name != "__init__":
                            dunders.append((method_name, i + 1))
                    elif not method_name.startswith("_"):
                        methods.append((method_name, i + 1))

                i += 1

            # Check properties are sorted
            if len(properties) > 1:
                names = [p[0] for p in properties]
                sorted_names = sorted(names)
                if names != sorted_names:
                    for j, (name, line_num) in enumerate(properties):
                        if name != sorted_names[j]:
                            errors.append(LintError(
                                filepath,
                                line_num,
                                f"Property '{name}' in {class_name} not alphabetized (expected '{sorted_names[j]}')"
                            ))
                            break

            # Check methods are sorted
            if len(methods) > 1:
                names = [m[0] for m in methods]
                sorted_names = sorted(names)
                if names != sorted_names:
                    for j, (name, line_num) in enumerate(methods):
                        if name != sorted_names[j]:
                            errors.append(LintError(
                                filepath,
                                line_num,
                                f"Method '{name}' in {class_name} not alphabetized (expected '{sorted_names[j]}')"
                            ))
                            break

            # Check dunders are sorted
            if len(dunders) > 1:
                names = [d[0] for d in dunders]
                sorted_names = sorted(names)
                if names != sorted_names:
                    for j, (name, line_num) in enumerate(dunders):
                        if name != sorted_names[j]:
                            errors.append(LintError(
                                filepath,
                                line_num,
                                f"Dunder '{name}' in {class_name} not alphabetized (expected '{sorted_names[j]}')"
                            ))
                            break
        else:
            i += 1

    return errors


def check_cargo_toml_deps(content: str, filepath: str) -> list[LintError]:
    """Check Cargo.toml dependencies are alphabetized."""
    errors = []
    lines = content.split("\n")

    in_deps_section = False
    deps = []

    for i, line in enumerate(lines):
        # Check for dependency section headers
        if re.match(r"\[(.*dependencies.*)\]", line):
            # Check previous section if any
            if len(deps) > 1:
                names = [d[0] for d in deps]
                sorted_names = sorted(names)
                if names != sorted_names:
                    for j, (name, line_num) in enumerate(deps):
                        if name != sorted_names[j]:
                            errors.append(LintError(
                                filepath,
                                line_num,
                                f"Dependency '{name}' not alphabetized (expected '{sorted_names[j]}')"
                            ))
                            break
            deps = []
            in_deps_section = True
            continue

        # Check for other section headers
        if re.match(r"\[.*\]", line):
            # Check previous section
            if in_deps_section and len(deps) > 1:
                names = [d[0] for d in deps]
                sorted_names = sorted(names)
                if names != sorted_names:
                    for j, (name, line_num) in enumerate(deps):
                        if name != sorted_names[j]:
                            errors.append(LintError(
                                filepath,
                                line_num,
                                f"Dependency '{name}' not alphabetized (expected '{sorted_names[j]}')"
                            ))
                            break
            deps = []
            in_deps_section = False
            continue

        # Collect dependencies
        if in_deps_section:
            dep_match = re.match(r"([a-zA-Z_][a-zA-Z0-9_-]*)\s*=", line)
            if dep_match:
                deps.append((dep_match.group(1), i + 1))

    # Check final section
    if in_deps_section and len(deps) > 1:
        names = [d[0] for d in deps]
        sorted_names = sorted(names)
        if names != sorted_names:
            for j, (name, line_num) in enumerate(deps):
                if name != sorted_names[j]:
                    errors.append(LintError(
                        filepath,
                        line_num,
                        f"Dependency '{name}' not alphabetized (expected '{sorted_names[j]}')"
                    ))
                    break

    return errors


def lint_file(filepath: str) -> list[LintError]:
    """Lint a single file for alphabetization issues."""
    errors = []

    path = Path(filepath)
    if not path.exists():
        return errors

    try:
        content = path.read_text()
    except Exception:
        return errors

    if filepath.endswith(".rs"):
        errors.extend(check_rust_match_arms(content, filepath))
        errors.extend(check_rust_mod_statements(content, filepath))
        errors.extend(check_rust_use_statements(content, filepath))
    elif filepath.endswith(".py"):
        errors.extend(check_python_class_members(content, filepath))
    elif filepath.endswith("Cargo.toml"):
        errors.extend(check_cargo_toml_deps(content, filepath))

    return errors


def main():
    parser = argparse.ArgumentParser(description="Alphabetization linter")
    parser.add_argument("files", nargs="*", help="Files to lint")
    parser.add_argument("--all", action="store_true", help="Lint all tracked files")
    parser.add_argument("--staged", action="store_true", help="Lint staged files")
    args = parser.parse_args()

    if args.all:
        files = get_all_files()
    elif args.staged:
        files = get_staged_files()
    elif args.files:
        files = args.files
    else:
        parser.print_help()
        sys.exit(1)

    # Filter to relevant file types
    files = [
        f for f in files
        if f.endswith(".rs") or f.endswith(".py") or f.endswith("Cargo.toml")
    ]

    all_errors = []
    for filepath in files:
        errors = lint_file(filepath)
        all_errors.extend(errors)

    if all_errors:
        print("Alphabetization errors found:\n")
        for error in all_errors:
            print(f"  {error}")
        print(f"\n{len(all_errors)} error(s) found")
        sys.exit(1)
    else:
        if files:
            print(f"Checked {len(files)} file(s), no alphabetization issues found")
        sys.exit(0)


if __name__ == "__main__":
    main()
